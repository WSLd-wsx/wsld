/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Standard virtio-msg frames over one kernel AF_VSOCK stream. */
#include <linux/completion.h>
#include <linux/kthread.h>
#include <linux/module.h>
#include <linux/net.h>
#include <linux/sched/task.h>
#include <linux/skbuff.h>
#include <linux/slab.h>
#include <linux/vm_sockets.h>
#include <linux/virtio_config.h>
#include <linux/workqueue.h>
#include <net/net_namespace.h>
#include <net/sock.h>
#include "vsock.h"
#include "pci-shm.h"

#define VMSG_QUEUE_LIMIT 64

struct vmsg_vsock {
    struct virtio_msg_device *device; /* Borrowed from the existing device owner. */
    struct socket *socket;
    struct task_struct *receiver;
    struct workqueue_struct *send_queue, *event_queue;
    struct work_struct send_work, event_work, shutdown_work;
    struct sk_buff_head send_frames, event_frames;
    spinlock_t lock;
    struct completion response_ready;
    struct virtio_msg *response;
    u8 expected[sizeof(struct virtio_msg)];
    __le16 original_token;
    u16 next_token, maximum;
    unsigned long timeout;
    int error;
    bool events_enabled, started;
    struct vmsg_pci_shm shm;
};

static int vmsg_full_io(struct vmsg_vsock *bus, void *buffer, size_t length, bool send)
{
    size_t done = 0;
    while (done < length) {
        struct msghdr msg = {.msg_flags = send ? MSG_NOSIGNAL : 0};
        struct kvec vec = {.iov_base = (u8 *)buffer + done, .iov_len = length - done};
        int result;
        if (READ_ONCE(bus->error))
            return READ_ONCE(bus->error);
        result = send ? kernel_sendmsg(bus->socket, &msg, &vec, 1, vec.iov_len)
                      : kernel_recvmsg(bus->socket, &msg, &vec, 1, vec.iov_len, 0);
        if (result < 0)
            return result;
        if (!result)
            return send ? -EPIPE : -ECONNRESET;
        done += result;
    }
    return 0;
}

static void vmsg_shutdown_work(struct work_struct *work)
{
    struct vmsg_vsock *bus = container_of(work, struct vmsg_vsock, shutdown_work);
    if (bus->socket)
        kernel_sock_shutdown(bus->socket, SHUT_RDWR);
}

/* Callable from notification/atomic context: no socket operation or waiting. */
static void vmsg_fail(struct vmsg_vsock *bus, int error)
{
    unsigned long flags;
    spin_lock_irqsave(&bus->lock, flags);
    if (!bus->error) {
        bus->error = error;
        bus->response = NULL;
        complete_all(&bus->response_ready);
        schedule_work(&bus->shutdown_work);
    }
    spin_unlock_irqrestore(&bus->lock, flags);
}

static int vmsg_enqueue(struct vmsg_vsock *bus, struct sk_buff_head *queue,
                        struct sk_buff *frame, struct workqueue_struct *worker,
                        struct work_struct *work)
{
    unsigned long flags;
    int error;
    spin_lock_irqsave(&bus->lock, flags);
    error = bus->error;
    if (!error && queue->qlen >= VMSG_QUEUE_LIMIT)
        error = -ENOBUFS;
    if (!error) {
        __skb_queue_tail(queue, frame);
        queue_work(worker, work);
    }
    spin_unlock_irqrestore(&bus->lock, flags);
    if (error)
        kfree_skb(frame);
    return error;
}

static void vmsg_send_work(struct work_struct *work)
{
    struct vmsg_vsock *bus = container_of(work, struct vmsg_vsock, send_work);
    struct sk_buff *frame;
    unsigned long flags;
    for (;;) {
        int result;
        spin_lock_irqsave(&bus->lock, flags);
        frame = __skb_dequeue(&bus->send_frames);
        spin_unlock_irqrestore(&bus->lock, flags);
        if (!frame)
            return;
        result = vmsg_full_io(bus, frame->data, frame->len, true);
        kfree_skb(frame);
        if (result)
            vmsg_fail(bus, result);
    }
}

static void vmsg_event_work(struct work_struct *work)
{
    struct vmsg_vsock *bus = container_of(work, struct vmsg_vsock, event_work);
    struct sk_buff *frame;
    unsigned long flags;
    for (;;) {
        int result;
        spin_lock_irqsave(&bus->lock, flags);
        frame = !bus->error && bus->events_enabled ? __skb_dequeue(&bus->event_frames) : NULL;
        spin_unlock_irqrestore(&bus->lock, flags);
        if (!frame)
            return;
        /* Never execute callbacks in the receiver: they may await a response. */
        result = virtio_msg_event(bus->device, (void *)frame->data);
        kfree_skb(frame);
        if (result)
            vmsg_fail(bus, result);
    }
}

static bool vmsg_response_matches(struct virtio_msg *response, struct virtio_msg *expected)
{
    return (response->type & (VIRTIO_MSG_TYPE_BUS | VIRTIO_MSG_TYPE_RESPONSE)) ==
            VIRTIO_MSG_TYPE_RESPONSE && !(response->msg_id & 0x40) &&
        response->dev_id == expected->dev_id && response->msg_id == expected->msg_id &&
        response->token == expected->token;
}

static int vmsg_deliver(struct vmsg_vsock *bus, struct sk_buff *frame)
{
    struct virtio_msg *message = (void *)frame->data;
    unsigned long flags;
    int error = 0;
    if (message->msg_id & 0x40) {
        if ((message->type & 3) || le16_to_cpu(message->dev_id) != bus->device->dev_id) {
            kfree_skb(frame);
            return -EPROTO;
        }
        return vmsg_enqueue(bus, &bus->event_frames, frame, bus->event_queue, &bus->event_work);
    }
    spin_lock_irqsave(&bus->lock, flags);
    if (bus->error)
        error = bus->error;
    else if (!bus->response || !vmsg_response_matches(message, (void *)bus->expected))
        error = -EPROTO;
    else {
        memcpy(bus->response, message, frame->len);
        /* Correlation tokens belong to the bus; restore the core's opaque token. */
        bus->response->token = bus->original_token;
        bus->response = NULL;
        complete(&bus->response_ready);
    }
    spin_unlock_irqrestore(&bus->lock, flags);
    kfree_skb(frame);
    return error;
}

static int vmsg_receive_thread(void *opaque)
{
    struct vmsg_vsock *bus = opaque;
    int error = 0;
    while (!READ_ONCE(bus->error) && !kthread_should_stop()) {
        u8 header[sizeof(struct virtio_msg)];
        struct virtio_msg *message = (void *)header;
        struct sk_buff *frame;
        u16 length;
        error = vmsg_full_io(bus, header, sizeof(header), false);
        if (error)
            break;
        length = le16_to_cpu(message->msg_size);
        if (length < sizeof(header) || length > bus->maximum) {
            error = -EPROTO;
            break;
        }
        frame = alloc_skb(length, GFP_KERNEL);
        if (!frame) {
            error = -ENOMEM;
            break;
        }
        memcpy(skb_put(frame, sizeof(header)), header, sizeof(header));
        error = vmsg_full_io(bus, skb_put(frame, length - sizeof(header)),
                             length - sizeof(header), false);
        if (error) {
            kfree_skb(frame);
            break;
        }
        error = vmsg_deliver(bus, frame);
        if (error)
            break;
    }
    if (error)
        vmsg_fail(bus, error);
    return 0;
}

static int vmsg_validate_shm(struct vmsg_vsock *bus, struct virtio_msg *request,
                            struct virtio_msg *response)
{
    struct get_shm *query;
    struct get_shm_resp *reply;
    if (request->msg_id != VIRTIO_MSG_GET_SHM)
        return 0;
    if (le16_to_cpu(request->msg_size) < sizeof(*request) + sizeof(*query) ||
        le16_to_cpu(response->msg_size) < sizeof(*response) + sizeof(*reply))
        return -EPROTO;
    query = (void *)(request + 1);
    reply = (void *)(response + 1);
    if (reply->index != query->index || reply->reserved)
        return -EPROTO;
    /* The draft's absent-region response claims no physical memory. */
    if (!le64_to_cpu(reply->length))
        return 0;
    return vmsg_pci_shm_validate(&bus->shm, le32_to_cpu(reply->index),
        le64_to_cpu(reply->addr), le64_to_cpu(reply->length));
}

static int vmsg_transfer(struct virtio_msg_device *device, struct virtio_msg *request,
                         struct virtio_msg *response)
{
    struct vmsg_vsock *bus = device->bus_data;
    struct sk_buff *frame;
    struct virtio_msg *wire;
    unsigned long flags;
    u16 length = le16_to_cpu(request->msg_size);
    int error;
    if (length < sizeof(*request) || length > bus->maximum || (request->type & 3) ||
        le16_to_cpu(request->dev_id) != device->dev_id ||
        (!!(request->msg_id & 0x40) != !response))
        return -EINVAL;
    spin_lock_irqsave(&bus->lock, flags);
    bus->started = true;
    spin_unlock_irqrestore(&bus->lock, flags);
    frame = alloc_skb(length, response ? GFP_KERNEL : GFP_ATOMIC);
    if (!frame) {
        vmsg_fail(bus, -ENOMEM);
        return -ENOMEM;
    }
    wire = (void *)skb_put(frame, length);
    memcpy(wire, request, length);
    if (response) {
        spin_lock_irqsave(&bus->lock, flags);
        error = bus->error ? bus->error : bus->response ? -EBUSY : 0;
        if (!error) {
            reinit_completion(&bus->response_ready);
            bus->response = response;
            bus->original_token = request->token;
            wire->token = cpu_to_le16(++bus->next_token);
            memcpy(bus->expected, wire, sizeof(bus->expected));
        }
        spin_unlock_irqrestore(&bus->lock, flags);
        if (error) {
            kfree_skb(frame);
            return error;
        }
    }
    error = vmsg_enqueue(bus, &bus->send_frames, frame, bus->send_queue, &bus->send_work);
    if (error)
        vmsg_fail(bus, error);
    if (!response)
        return error;
    if (!wait_for_completion_timeout(&bus->response_ready, bus->timeout))
        vmsg_fail(bus, -ETIMEDOUT);
    spin_lock_irqsave(&bus->lock, flags);
    error = bus->error;
    bus->response = NULL;
    spin_unlock_irqrestore(&bus->lock, flags);
    if (!error) {
        error = vmsg_validate_shm(bus, request, response);
        if (error)
            vmsg_fail(bus, error);
    }
    return error;
}

static int vmsg_bind_shm(struct vmsg_vsock *bus, struct pci_dev *device,
                         struct device *pci_parent, u8 id, unsigned int bar)
{
    struct vmsg_pci_shm candidate = {0};
    unsigned long flags;
    int error;
    if (!bus)
        return -EINVAL;
    error = vmsg_pci_shm_bind(&candidate, device, pci_parent, id, bar);
    if (error)
        return error;
    spin_lock_irqsave(&bus->lock, flags);
    error = bus->error ? bus->error : (bus->started || bus->events_enabled || bus->shm.device) ? -EBUSY : 0;
    if (!error) {
        bus->shm = candidate;
        candidate.device = NULL;
    }
    spin_unlock_irqrestore(&bus->lock, flags);
    vmsg_pci_shm_put(&candidate);
    return error;
}

int vmsg_vsock_bind_shm(struct vmsg_vsock *bus, struct pci_dev *device, u8 id, unsigned int bar)
{
    if (!bus)
        return -EINVAL;
    return vmsg_bind_shm(bus, device, bus->device->vdev.dev.parent, id, bar);
}
EXPORT_SYMBOL_GPL(vmsg_vsock_bind_shm);

int vmsg_vsock_bind_shm_parent(struct vmsg_vsock *bus, struct pci_dev *device,
                               struct device *parent, u8 id, unsigned int bar)
{
    if (!bus || !device || !parent || parent == &device->dev ||
        bus->device->vdev.dev.parent != parent || parent->parent != &device->dev ||
        !device_is_registered(parent))
        return -EINVAL;
    return vmsg_bind_shm(bus, device, &device->dev, id, bar);
}
EXPORT_SYMBOL_GPL(vmsg_vsock_bind_shm_parent);

static bool vmsg_get_shm_region(struct virtio_msg_device *device,
                                struct virtio_shm_region *region, u8 id)
{
    struct vmsg_vsock *bus = device->bus_data;
    unsigned long flags;
    int error = 0;
    bool found = false;

    *region = (struct virtio_shm_region){0};
    if (!bus)
        return false;
    spin_lock_irqsave(&bus->lock, flags);
    bus->started = true;
    if (!bus->error && bus->shm.device && id == bus->shm.id) {
        error = vmsg_pci_shm_validate(&bus->shm, id, bus->shm.base, bus->shm.length);
        if (!error) {
            region->addr = bus->shm.base;
            region->len = bus->shm.length;
            found = true;
        }
    }
    spin_unlock_irqrestore(&bus->lock, flags);
    if (error)
        vmsg_fail(bus, error);
    return found;
}

static const char *vmsg_bus_info(struct virtio_msg_device *device, u16 *size, u32 *revision)
{
    struct vmsg_vsock *bus = device->bus_data;
    *size = bus->maximum;
    *revision = VIRTIO_MSG_REVISION_1;
    return "virtio-msg-vsock";
}

static void vmsg_synchronize(struct virtio_msg_device *device)
{
    struct vmsg_vsock *bus = device->bus_data;
    flush_work(&bus->event_work);
}

static const struct virtio_msg_ops vmsg_ops = {
    .bus_info = vmsg_bus_info, .transfer = vmsg_transfer, .synchronize_cbs = vmsg_synchronize,
    .get_shm_region = vmsg_get_shm_region,
};

const struct virtio_msg_ops *vmsg_vsock_get_ops(void) { return &vmsg_ops; }
EXPORT_SYMBOL_GPL(vmsg_vsock_get_ops);

int vmsg_vsock_enable_events(struct vmsg_vsock *bus)
{
    unsigned long flags;
    int error;
    spin_lock_irqsave(&bus->lock, flags);
    error = bus->error;
    if (!error) {
        bus->events_enabled = true;
        queue_work(bus->event_queue, &bus->event_work);
    }
    spin_unlock_irqrestore(&bus->lock, flags);
    return error;
}
EXPORT_SYMBOL_GPL(vmsg_vsock_enable_events);

void vmsg_vsock_shutdown(struct vmsg_vsock *bus)
{
    vmsg_fail(bus, -ESHUTDOWN);
    flush_work(&bus->shutdown_work);
    if (bus->receiver) {
        kthread_stop_put(bus->receiver);
        bus->receiver = NULL;
    }
    cancel_work_sync(&bus->send_work);
    cancel_work_sync(&bus->event_work);
    skb_queue_purge(&bus->send_frames);
    skb_queue_purge(&bus->event_frames);
}
EXPORT_SYMBOL_GPL(vmsg_vsock_shutdown);

void vmsg_vsock_destroy(struct vmsg_vsock *bus)
{
    vmsg_vsock_shutdown(bus);
    if (bus->send_queue)
        destroy_workqueue(bus->send_queue);
    if (bus->event_queue)
        destroy_workqueue(bus->event_queue);
    if (bus->socket)
        sock_release(bus->socket);
    if (bus->device->bus_data == bus)
        bus->device->bus_data = NULL;
    vmsg_pci_shm_put(&bus->shm);
    kfree(bus);
}
EXPORT_SYMBOL_GPL(vmsg_vsock_destroy);

struct vmsg_vsock *vmsg_vsock_connect(struct virtio_msg_device *device, u32 cid,
                                     u32 port, u16 maximum, unsigned long timeout)
{
    struct sockaddr_vm address = {.svm_family = AF_VSOCK, .svm_cid = cid, .svm_port = port};
    struct vmsg_vsock *bus;
    int error;
    if (!device || device->bus_data || maximum < VIRTIO_MSG_MIN_SIZE || !timeout ||
        timeout > MAX_SCHEDULE_TIMEOUT || cid != VMADDR_CID_HOST || port == VMADDR_PORT_ANY)
        return ERR_PTR(-EINVAL);
    bus = kzalloc(sizeof(*bus), GFP_KERNEL);
    if (!bus)
        return ERR_PTR(-ENOMEM);
    bus->device = device;
    bus->maximum = maximum;
    bus->timeout = timeout;
    spin_lock_init(&bus->lock);
    init_completion(&bus->response_ready);
    skb_queue_head_init(&bus->send_frames);
    skb_queue_head_init(&bus->event_frames);
    INIT_WORK(&bus->send_work, vmsg_send_work);
    INIT_WORK(&bus->event_work, vmsg_event_work);
    INIT_WORK(&bus->shutdown_work, vmsg_shutdown_work);
    bus->send_queue = alloc_ordered_workqueue("vmsg-vsock-tx", WQ_MEM_RECLAIM);
    bus->event_queue = alloc_ordered_workqueue("vmsg-vsock-event", WQ_MEM_RECLAIM);
    error = -ENOMEM;
    if (!bus->send_queue || !bus->event_queue)
        goto fail;
    error = sock_create_kern(&init_net, AF_VSOCK, SOCK_STREAM, 0, &bus->socket);
    if (error)
        goto fail;
    bus->socket->sk->sk_sndtimeo = timeout;
    error = kernel_connect(bus->socket, (void *)&address, sizeof(address), 0);
    if (error)
        goto fail;
    bus->receiver = kthread_create(vmsg_receive_thread, bus, "vmsg-vsock-rx");
    if (IS_ERR(bus->receiver)) {
        error = PTR_ERR(bus->receiver);
        bus->receiver = NULL;
        goto fail;
    }
    /* The receiver may exit on peer failure before shutdown joins it. */
    get_task_struct(bus->receiver);
    device->bus_data = bus;
    wake_up_process(bus->receiver);
    return bus;
fail:
    vmsg_vsock_destroy(bus);
    return ERR_PTR(error);
}
EXPORT_SYMBOL_GPL(vmsg_vsock_connect);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Offline virtio-msg AF_VSOCK bus integration library");
