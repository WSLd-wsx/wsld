// SPDX-License-Identifier: GPL-2.0+
/*
 * Virtio message transport.
 *
 * Copyright (C) 2026 Google LLC and Linaro.
 * Viresh Kumar <viresh.kumar@linaro.org>
 *
 * The virtio-msg transport encapsulates virtio operations as discrete message
 * exchanges rather than relying on PCI or memory-mapped I/O regions. It
 * separates bus-level functionality (e.g., device enumeration, hotplug events)
 * from device-specific operations (e.g., feature negotiation, virtqueue setup),
 * ensuring that a single, generic transport layer can be reused across multiple
 * bus implementations (like ARM Firmware Framework (FF-A), IPC, etc.).
 *
 * This file implements the generic Virtio message transport layer.
 */

#define pr_fmt(fmt) "virtio-msg: " fmt

#include <linux/bits.h>
#include <linux/err.h>
#include <linux/list.h>
#include <linux/rcupdate.h>
#include <linux/slab.h>
#include <linux/virtio.h>
#include <linux/virtio_config.h>
#include <linux/virtio_ring.h>
#include <uapi/linux/virtio_msg.h>

#include "virtio_msg_internal.h"

#define to_virtio_msg_device(_dev)	\
	container_of(_dev, struct virtio_msg_device, vdev)

static void msg_prepare(struct virtio_msg *vmsg, bool bus, u8 msg_id,
			u16 dev_id, u16 token, u16 payload_size)
{
	u16 size = sizeof(*vmsg) + payload_size;

	memset(vmsg, 0, size);

	if (bus) {
		vmsg->type = VIRTIO_MSG_TYPE_BUS;
	} else {
		vmsg->type = VIRTIO_MSG_TYPE_TRANSPORT;
		vmsg->dev_id = cpu_to_le16(dev_id);
	}

	vmsg->msg_id = msg_id;
	vmsg->token = cpu_to_le16(token);
	vmsg->msg_size = cpu_to_le16(size);
}

static void transport_msg_prepare(struct virtio_msg_device *vmdev, u8 msg_id,
				  u16 token, u16 payload_size)
{
	msg_prepare(vmdev->request, false, msg_id, vmdev->dev_id, token,
		    payload_size);
}

/**
 * virtio_msg_prepare - Initialize a virtio message for bus transfer
 * @vmsg: Pointer to the virtio message structure to initialize
 * @msg_id: Message ID to set in the message header
 * @token: Token for the transaction.
 * @payload_size: Size of the payload (in bytes)
 *
 * Prepares a virtio_msg structure for transmission over the bus transport
 * (VIRTIO_MSG_TYPE_BUS). The payload buffer follows the header.
 */
void virtio_msg_prepare(struct virtio_msg *vmsg, u8 msg_id, u16 token,
			u16 payload_size)
{
	msg_prepare(vmsg, true, msg_id, 0, token, payload_size);
}
EXPORT_SYMBOL_GPL(virtio_msg_prepare);

static int virtio_msg_xfer(struct virtio_msg_device *vmdev)
{
	struct virtio_msg *response = vmdev->response;
	u32 minimum = sizeof(*response);
	int ret;

	switch (vmdev->request->msg_id) {
	case VIRTIO_MSG_DEVICE_INFO:
		minimum += sizeof(struct get_device_info_resp);
		break;
	case VIRTIO_MSG_GET_DEV_FEATURES:
		minimum += sizeof(struct get_features_resp) + 2 * sizeof(__le32);
		break;
	case VIRTIO_MSG_GET_CONFIG:
		minimum += sizeof(struct get_config_resp) +
			le32_to_cpu(((struct get_config *)
			virtio_msg_payload(vmdev->request))->size);
		break;
	case VIRTIO_MSG_SET_CONFIG:
		minimum += sizeof(struct set_config_resp);
		break;
	case VIRTIO_MSG_GET_DEVICE_STATUS:
		minimum += sizeof(struct get_device_status_resp);
		break;
	case VIRTIO_MSG_SET_DEVICE_STATUS:
		minimum += sizeof(struct set_device_status_resp);
		break;
	case VIRTIO_MSG_GET_VQUEUE:
		minimum += sizeof(struct get_vqueue_resp);
		break;
	case VIRTIO_MSG_GET_SHM:
		minimum += sizeof(struct get_shm_resp);
		break;
	case VIRTIO_MSG_SET_DRV_FEATURES:
	case VIRTIO_MSG_SET_VQUEUE:
	case VIRTIO_MSG_RESET_VQUEUE:
		break;
	default:
		return -EINVAL;
	}

	memset(vmdev->response, 0, vmdev->msg_size);

	ret = vmdev->ops->transfer(vmdev, vmdev->request, vmdev->response);
	if (ret) {
		dev_err(&vmdev->vdev.dev, "Transfer request failed (%d)\n", ret);
		return ret;
	}

	/* The bus must supply one complete frame within the advertised bound. */
	if (le16_to_cpu(response->msg_size) < minimum ||
	    le16_to_cpu(response->msg_size) > vmdev->msg_size ||
	    response->token != vmdev->request->token ||
	    response->dev_id != vmdev->request->dev_id ||
	    response->msg_id != vmdev->request->msg_id ||
	    (response->type & (VIRTIO_MSG_TYPE_RESPONSE | VIRTIO_MSG_TYPE_BUS)) !=
		VIRTIO_MSG_TYPE_RESPONSE)
		return -EPROTO;

	/* Repeated fields validate semantics, not bus-owned token correlation. */
	if (response->msg_id == VIRTIO_MSG_GET_CONFIG) {
		struct get_config *req = virtio_msg_payload(vmdev->request);
		struct get_config_resp *res = virtio_msg_payload(response);

		if (res->offset != req->offset || res->size != req->size)
			return -EPROTO;
	} else if (response->msg_id == VIRTIO_MSG_SET_CONFIG) {
		struct set_config *req = virtio_msg_payload(vmdev->request);
		struct set_config_resp *res = virtio_msg_payload(response);
		u32 size = le32_to_cpu(res->size);

		if (res->offset != req->offset)
			return -EPROTO;
		/* A zero-length response to a nonzero write means not applied. */
		if (!size && le32_to_cpu(req->size))
			return -EIO;
		if (res->size != req->size ||
		    size > le16_to_cpu(response->msg_size) - sizeof(*response) -
			   sizeof(*res) || memcmp(res->config, req->config, size))
			return -EPROTO;
	}

	return 0;
}

static int virtio_msg_get_device_info(struct virtio_msg_device *vmdev)
{
	struct get_device_info_resp *payload = virtio_msg_payload(vmdev->response);
	struct virtio_device *vdev = &vmdev->vdev;
	u32 num_feature_blocks;
	int ret;

	guard(mutex)(&vmdev->request_lock);

	static_assert(sizeof(*vmdev->response) + sizeof(*payload) <=
		      VIRTIO_MSG_MIN_SIZE);

	transport_msg_prepare(vmdev, VIRTIO_MSG_DEVICE_INFO, TOKEN_FIXED, 0);

	ret = virtio_msg_xfer(vmdev);
	if (ret)
		return ret;

	vdev->id.device = le32_to_cpu(payload->device_id);
	if (vdev->id.device == 0) {
		/*
		 * A virtio device with ID 0 is a placeholder and indicates no
		 * valid device function is present.
		 */
		return -ENODEV;
	}

	vdev->id.vendor = le32_to_cpu(payload->vendor_id);
	vmdev->config_size = le32_to_cpu(payload->config_size);
	num_feature_blocks = le32_to_cpu(payload->num_feature_blocks);

	/* Linux supports 64 feature bits */
	if (num_feature_blocks < 2) {
		dev_err(&vdev->dev, "Incompatible num_feature_blocks (%u)\n",
			num_feature_blocks);
		return -EINVAL;
	}

	return 0;
}

static u64 virtio_msg_get_features(struct virtio_device *vdev)
{
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vdev);
	struct get_features *req_payload = virtio_msg_payload(vmdev->request);
	struct get_features_resp *res_payload = virtio_msg_payload(vmdev->response);
	__le32 *features;
	int ret;

	guard(mutex)(&vmdev->request_lock);

	static_assert(sizeof(*vmdev->request) + sizeof(*req_payload) <
		      VIRTIO_MSG_MIN_SIZE);
	static_assert(sizeof(*vmdev->response) + sizeof(*res_payload)
		      + 2 * sizeof(*features) < VIRTIO_MSG_MIN_SIZE);

	transport_msg_prepare(vmdev, VIRTIO_MSG_GET_DEV_FEATURES, TOKEN_FIXED,
			      sizeof(*req_payload));

	/* Linux supports 64 feature bits */
	req_payload->num = cpu_to_le32(2);
	req_payload->index = 0;

	ret = virtio_msg_xfer(vmdev);
	if (ret)
		return 0;

	features = (__le32 *)res_payload->features;
	return ((u64)(le32_to_cpu(features[1])) << 32) | le32_to_cpu(features[0]);
}

static int virtio_msg_finalize_features(struct virtio_device *vdev)
{
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vdev);
	struct set_features *payload = virtio_msg_payload(vmdev->request);
	__le32 *features = (__le32 *)payload->features;

	guard(mutex)(&vmdev->request_lock);

	static_assert(sizeof(*vmdev->request) + sizeof(*payload)
		      + 2 * sizeof(*features) < VIRTIO_MSG_MIN_SIZE);

	/* Give virtio_ring a chance to accept features */
	vring_transport_features(vdev);

	transport_msg_prepare(vmdev, VIRTIO_MSG_SET_DRV_FEATURES, TOKEN_FIXED,
			      sizeof(*payload) + 2 * sizeof(*features));

	/* Linux supports 64 feature bits */
	payload->num = cpu_to_le32(2);
	payload->index = 0;

	features[0] = cpu_to_le32((u32)vmdev->vdev.features);
	features[1] = cpu_to_le32(vmdev->vdev.features >> 32);

	return virtio_msg_xfer(vmdev);
}

static void virtio_msg_get(struct virtio_device *vdev, unsigned int offset,
			   void *buf, unsigned int len)
{
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vdev);
	struct get_config *req_payload = virtio_msg_payload(vmdev->request);
	struct get_config_resp *res_payload = virtio_msg_payload(vmdev->response);
	u32 i = 0, max;

	guard(mutex)(&vmdev->request_lock);

	static_assert(sizeof(*vmdev->request) + sizeof(*req_payload) <
		      VIRTIO_MSG_MIN_SIZE);
	static_assert(sizeof(*vmdev->response) + sizeof(*res_payload) <
		      VIRTIO_MSG_MIN_SIZE);

	if (offset > vmdev->config_size || len > vmdev->config_size - offset) {
		dev_err(&vmdev->vdev.dev,
			"Invalid config read operation: %u: %u: %u\n", offset,
			len, vmdev->config_size);
		return;
	}

	if (!len)
		return;

	/* Maximum payload size available in the response message buffer */
	max = vmdev->msg_size - sizeof(*vmdev->response) - sizeof(*res_payload);

	transport_msg_prepare(vmdev, VIRTIO_MSG_GET_CONFIG, TOKEN_FIXED,
			      sizeof(*req_payload));

	while (i != len) {
		u32 size = min(max, len - i);

		req_payload->offset = cpu_to_le32(offset + i);
		req_payload->size = cpu_to_le32(size);

		if (virtio_msg_xfer(vmdev))
			return;

		/* Buffer holds the data in little endian */
		memcpy(buf + i, res_payload->config, size);
		i += size;
	}

	WRITE_ONCE(vmdev->generation_count, le32_to_cpu(res_payload->generation));
}

static void virtio_msg_set(struct virtio_device *vdev, unsigned int offset,
			   const void *buf, unsigned int len)
{
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vdev);
	struct set_config *payload = virtio_msg_payload(vmdev->request);
	struct set_config_resp *response = virtio_msg_payload(vmdev->response);
	u32 i = 0, max, fixed_size = sizeof(*vmdev->request) + sizeof(*payload);

	guard(mutex)(&vmdev->request_lock);

	static_assert(sizeof(*vmdev->request) + sizeof(*payload) <
		      VIRTIO_MSG_MIN_SIZE);

	if (offset > vmdev->config_size || len > vmdev->config_size - offset) {
		dev_err(&vmdev->vdev.dev,
			"Invalid config write operation: %u: %u: %u\n", offset,
			len, vmdev->config_size);
		return;
	}

	/* Maximum payload size available in the request message buffer */
	max = vmdev->msg_size - fixed_size;

	/* Message size is set before sending the message */
	transport_msg_prepare(vmdev, VIRTIO_MSG_SET_CONFIG, TOKEN_FIXED, 0);
	/* The bus adapter must select the baseline configuration profile. */
	payload->generation = 0;

	while (i != len) {
		u32 size = min(max, len - i);

		payload->offset = cpu_to_le32(offset + i);
		payload->size = cpu_to_le32(size);

		/* Buffer holds the data in little endian */
		memcpy(payload->config, buf + i, size);
		i += size;

		/* Update the message size */
		vmdev->request->msg_size = cpu_to_le16(fixed_size + size);

		if (virtio_msg_xfer(vmdev))
			return;

		WRITE_ONCE(vmdev->generation_count, le32_to_cpu(response->generation));
	}
}

static u32 virtio_msg_generation(struct virtio_device *vdev)
{
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vdev);
	struct get_config *req_payload = virtio_msg_payload(vmdev->request);
	struct get_config_resp *res_payload = virtio_msg_payload(vmdev->response);

	guard(mutex)(&vmdev->request_lock);

	transport_msg_prepare(vmdev, VIRTIO_MSG_GET_CONFIG, TOKEN_FIXED,
			      sizeof(*req_payload));
	req_payload->offset = cpu_to_le32(0);
	req_payload->size = cpu_to_le32(0);

	if (virtio_msg_xfer(vmdev))
		return 0;

	WRITE_ONCE(vmdev->generation_count, le32_to_cpu(res_payload->generation));
	return READ_ONCE(vmdev->generation_count);
}

static u8 virtio_msg_get_status(struct virtio_device *vdev)
{
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vdev);
	struct get_device_status_resp *payload = virtio_msg_payload(vmdev->response);

	guard(mutex)(&vmdev->request_lock);

	static_assert(sizeof(*vmdev->response) + sizeof(*payload) <
		      VIRTIO_MSG_MIN_SIZE);

	transport_msg_prepare(vmdev, VIRTIO_MSG_GET_DEVICE_STATUS, TOKEN_FIXED, 0);

	if (virtio_msg_xfer(vmdev))
		return VIRTIO_CONFIG_S_FAILED;

	return (u8)le32_to_cpu(payload->status);
}

static void virtio_msg_set_status(struct virtio_device *vdev, u8 status)
{
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vdev);
	struct set_device_status *payload = virtio_msg_payload(vmdev->request);

	guard(mutex)(&vmdev->request_lock);

	static_assert(sizeof(*vmdev->request) + sizeof(*payload) <
		      VIRTIO_MSG_MIN_SIZE);

	transport_msg_prepare(vmdev, VIRTIO_MSG_SET_DEVICE_STATUS, TOKEN_FIXED,
			      sizeof(*payload));
	payload->status = cpu_to_le32(status);

	if (virtio_msg_xfer(vmdev))
		dev_warn(&vmdev->vdev.dev, "Set status %u failed\n", status);
}

static void virtio_msg_vq_reset(struct virtqueue *vq)
{
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vq->vdev);
	struct reset_vqueue *payload = virtio_msg_payload(vmdev->request);

	guard(mutex)(&vmdev->request_lock);

	static_assert(sizeof(*vmdev->request) + sizeof(*payload) <
		      VIRTIO_MSG_MIN_SIZE);

	transport_msg_prepare(vmdev, VIRTIO_MSG_RESET_VQUEUE, TOKEN_FIXED,
			      sizeof(*payload));
	payload->index = cpu_to_le32(vq->index);

	if (virtio_msg_xfer(vmdev))
		dev_warn(&vmdev->vdev.dev, "Virtqueue reset failed\n");
}

static void virtio_msg_reset(struct virtio_device *vdev)
{
	/* Setting status to 0 initiates a device reset */
	virtio_msg_set_status(vdev, 0);
}

static bool _vmsg_notify(struct virtqueue *vq, u32 index, u32 offset, bool wrap)
{
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vq->vdev);
	u8 buffer[sizeof(struct virtio_msg) + sizeof(struct event_avail)];
	struct virtio_msg *event = (void *)buffer;
	struct event_avail *payload = virtio_msg_payload(event);
	u32 val;

	static_assert(sizeof(*event) + sizeof(*payload) <
		      VIRTIO_MSG_MIN_SIZE);
	msg_prepare(event, false, VIRTIO_MSG_EVENT_AVAIL, vmdev->dev_id,
		    TOKEN_EVENT, sizeof(*payload));
	payload->index = cpu_to_le32(index);

	val = offset & GENMASK(VIRTIO_MSG_EVENT_AVAIL_WRAP_SHIFT - 1, 0);
	if (wrap)
		val |= BIT(VIRTIO_MSG_EVENT_AVAIL_WRAP_SHIFT);
	payload->next_offset_wrap = cpu_to_le32(val);

	/* The bus must consume/copy this event without sleeping before return. */
	return !vmdev->ops->transfer(vmdev, event, NULL);
}

static bool virtio_msg_notify(struct virtqueue *vq)
{
	return _vmsg_notify(vq, vq->index, 0, 0);
}

static bool virtio_msg_notify_with_data(struct virtqueue *vq)
{
	u32 index, offset, wrap, data = vring_notification_data(vq);

	index = data & GENMASK(15, 0);
	data >>= 16;
	offset = data & GENMASK(VRING_PACKED_EVENT_F_WRAP_CTR - 1, 0);
	wrap = data >> VRING_PACKED_EVENT_F_WRAP_CTR;

	return _vmsg_notify(vq, index, offset, wrap);
}

/**
 * virtio_msg_event - Handle an incoming event message for a virtio message device.
 * @vmdev: Pointer to the virtio message device.
 * @vmsg: Pointer to the incoming virtio message containing the event.
 *
 * Processes an event message received from the bus layer for the given
 * virtio message device. Depending on the message type, this function either:
 *
 *  - Triggers the device configuration change handler if the message is of type
 *    VIRTIO_MSG_EVENT_CONFIG.
 *
 *  - Delivers a used-buffer notification to the corresponding virtqueue if the
 *    message is of type VIRTIO_MSG_EVENT_USED.
 *
 * Return: 0 for a valid notification, -EPROTO for malformed frames, or
 * -EINVAL for unknown message types or invalid virtqueue indices.
 */
int virtio_msg_event(struct virtio_msg_device *vmdev, struct virtio_msg *vmsg)
{
	struct event_used *payload = virtio_msg_payload(vmsg);
	struct device *dev = &vmdev->vdev.dev;
	struct virtqueue *vq;
	unsigned int index;
	u32 size = le16_to_cpu(vmsg->msg_size);

	/* The bus supplies a complete frame; validate before reading its payload. */
	if (size < sizeof(*vmsg) || size > vmdev->msg_size ||
	    le16_to_cpu(vmsg->dev_id) != vmdev->dev_id ||
	    (vmsg->type & (VIRTIO_MSG_TYPE_RESPONSE | VIRTIO_MSG_TYPE_BUS)))
		return -EPROTO;

	if (vmsg->msg_id == VIRTIO_MSG_EVENT_CONFIG) {
		struct event_config *config = virtio_msg_payload(vmsg);
		u32 offset, length, data_size;

		if (size < sizeof(*vmsg) + sizeof(*config))
			return -EPROTO;
		offset = le32_to_cpu(config->offset);
		length = le32_to_cpu(config->size);
		data_size = size - sizeof(*vmsg) - sizeof(*config);
		if (offset > vmdev->config_size ||
		    length > vmdev->config_size - offset ||
		    (data_size && data_size < length))
			return -EPROTO;

		WRITE_ONCE(vmdev->generation_count, le32_to_cpu(config->generation));
		/* The standard driver callback refreshes required configuration. */
		virtio_config_changed(&vmdev->vdev);
		return 0;
	}

	if (vmsg->msg_id == VIRTIO_MSG_EVENT_USED) {
		if (size < sizeof(*vmsg) + sizeof(*payload))
			return -EPROTO;
		index = le32_to_cpu(payload->index);

		virtio_device_for_each_vq(&vmdev->vdev, vq) {
			if (index == vq->index) {
				/* A coalesced notification may find an already-drained ring. */
				vring_interrupt(0, vq);

				return 0;
			}
		}

		dev_err(dev, "Failed to find virtqueue (%u)", index);
	} else {
		dev_err(dev, "Unexpected message id: (%u)\n", vmsg->msg_id);
	}

	return -EINVAL;
}
EXPORT_SYMBOL_GPL(virtio_msg_event);

static void virtio_msg_del_vqs(struct virtio_device *vdev)
{
	struct virtqueue *vq, *n;

	list_for_each_entry_safe(vq, n, &vdev->vqs, list) {
		virtio_msg_vq_reset(vq);
		vring_del_virtqueue(vq);
	}
}

static int virtio_msg_vq_get_locked(struct virtio_msg_device *vmdev, unsigned int *num,
			     unsigned int index)
{
	struct get_vqueue *req_payload = virtio_msg_payload(vmdev->request);
	struct get_vqueue_resp *res_payload = virtio_msg_payload(vmdev->response);
	int ret;

	static_assert(sizeof(*vmdev->request) + sizeof(*req_payload) <
		      VIRTIO_MSG_MIN_SIZE);
	static_assert(sizeof(*vmdev->response) + sizeof(*res_payload) <
		      VIRTIO_MSG_MIN_SIZE);

	transport_msg_prepare(vmdev, VIRTIO_MSG_GET_VQUEUE, TOKEN_FIXED,
			      sizeof(*req_payload));
	req_payload->index = cpu_to_le32(index);

	ret = virtio_msg_xfer(vmdev);
	if (ret)
		return ret;

	if (le32_to_cpu(res_payload->index) != index)
		return -EPROTO;
	*num = le32_to_cpu(res_payload->max_size);
	if (!*num)
		return -ENOENT;

	return 0;
}

static int virtio_msg_vq_get(struct virtio_msg_device *vmdev, unsigned int *num,
			     unsigned int index)
{
	guard(mutex)(&vmdev->request_lock);

	return virtio_msg_vq_get_locked(vmdev, num, index);
}

static int virtio_msg_vq_set(struct virtio_msg_device *vmdev,
			     struct virtqueue *vq, unsigned int index)
{
	struct set_vqueue *payload = virtio_msg_payload(vmdev->request);
	struct get_vqueue_resp *response = virtio_msg_payload(vmdev->response);
	unsigned int maximum;
	int ret;

	guard(mutex)(&vmdev->request_lock);

	static_assert(sizeof(*vmdev->request) + sizeof(*payload) <
		      VIRTIO_MSG_MIN_SIZE);

	transport_msg_prepare(vmdev, VIRTIO_MSG_SET_VQUEUE, TOKEN_FIXED,
			      sizeof(*payload));
	payload->index = cpu_to_le32(index);
	/* Draft SET_VQUEUE state operation 1 enables the configured queue. */
	payload->flags = cpu_to_le32(1);
	payload->size = cpu_to_le32(virtqueue_get_vring_size(vq));
	payload->descriptor_addr = cpu_to_le64(virtqueue_get_desc_addr(vq));
	payload->driver_addr = cpu_to_le64(virtqueue_get_avail_addr(vq));
	payload->device_addr = cpu_to_le64(virtqueue_get_used_addr(vq));

	ret = virtio_msg_xfer(vmdev);
	if (ret)
		return ret;

	/* An empty SET_VQUEUE response may acknowledge a rejected update. */
	ret = virtio_msg_vq_get_locked(vmdev, &maximum, index);
	if (ret)
		return ret;
	if (maximum < virtqueue_get_vring_size(vq) ||
	    le32_to_cpu(response->size) != virtqueue_get_vring_size(vq) ||
	    !(le32_to_cpu(response->reserved) & 1) ||
	    le64_to_cpu(response->descriptor_addr) != virtqueue_get_desc_addr(vq) ||
	    le64_to_cpu(response->driver_addr) != virtqueue_get_avail_addr(vq) ||
	    le64_to_cpu(response->device_addr) != virtqueue_get_used_addr(vq))
		return -EIO;

	return 0;
}

static struct virtqueue *
virtio_msg_setup_vq(struct virtio_msg_device *vmdev, unsigned int index,
		    void (*callback)(struct virtqueue *vq), const char *name,
		    bool ctx)
{
	bool (*notify)(struct virtqueue *vq);
	struct virtqueue *vq;
	unsigned int num;
	int ret;

	if (__virtio_test_bit(&vmdev->vdev, VIRTIO_F_NOTIFICATION_DATA))
		notify = virtio_msg_notify_with_data;
	else
		notify = virtio_msg_notify;

	ret = virtio_msg_vq_get(vmdev, &num, index);
	if (ret)
		return ERR_PTR(ret);

	vq = vring_create_virtqueue(index, num, PAGE_SIZE, &vmdev->vdev, true,
				    true, ctx, notify, callback, name);
	if (!vq)
		return ERR_PTR(-ENOMEM);

	vq->num_max = num;

	ret = virtio_msg_vq_set(vmdev, vq, index);
	if (ret) {
		vring_del_virtqueue(vq);
		return ERR_PTR(ret);
	}

	return vq;
}

static int virtio_msg_find_vqs(struct virtio_device *vdev, unsigned int nvqs,
			       struct virtqueue *vqs[],
			       struct virtqueue_info vqs_info[],
			       struct irq_affinity *desc)
{
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vdev);
	int i, queue_idx = 0;

	for (i = 0; i < nvqs; ++i) {
		struct virtqueue_info *vqi = &vqs_info[i];

		if (!vqi->name) {
			vqs[i] = NULL;
			continue;
		}

		vqs[i] = virtio_msg_setup_vq(vmdev, queue_idx++, vqi->callback,
					     vqi->name, vqi->ctx);
		if (IS_ERR(vqs[i])) {
			virtio_msg_del_vqs(vdev);
			return PTR_ERR(vqs[i]);
		}
	}

	if (vmdev->ops->queues_ready) {
		int ret = vmdev->ops->queues_ready(vmdev);
		if (ret) {
			virtio_msg_del_vqs(vdev);
			return ret;
		}
	}
	return 0;
}

static const char *virtio_msg_bus_name(struct virtio_device *vdev)
{
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vdev);

	return vmdev->bus_name;
}

static void virtio_msg_synchronize_cbs(struct virtio_device *vdev)
{
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vdev);

	if (vmdev->ops->synchronize_cbs)
		vmdev->ops->synchronize_cbs(vmdev);
	else
		synchronize_rcu();
}

static void virtio_msg_release_dev(struct device *_d)
{
	struct virtio_device *vdev =
			container_of(_d, struct virtio_device, dev);
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vdev);

	/* The bus release callback may free the containing device. */
	kfree(vmdev->request);
	vmdev->request = NULL;
	vmdev->response = NULL;
	if (vmdev->ops->release)
		vmdev->ops->release(vmdev);
}

static bool virtio_msg_get_shm_region(struct virtio_device *vdev,
				      struct virtio_shm_region *region, u8 id)
{
	struct virtio_msg_device *vmdev = to_virtio_msg_device(vdev);
	struct get_shm *request = virtio_msg_payload(vmdev->request);
	struct get_shm_resp *response = virtio_msg_payload(vmdev->response);
	u64 addr, len;

	guard(mutex)(&vmdev->request_lock);

	static_assert(sizeof(*vmdev->response) + sizeof(*response) <=
		      VIRTIO_MSG_MIN_SIZE);
	region->addr = 0;
	region->len = 0;
	if (vmdev->ops->get_shm_region) {
		struct virtio_shm_region local = {};

		if (!vmdev->ops->get_shm_region(vmdev, &local, id) ||
		    !local.len || local.len > U64_MAX - local.addr)
			return false;
		*region = local;
		return true;
	}
	transport_msg_prepare(vmdev, VIRTIO_MSG_GET_SHM, TOKEN_FIXED,
			      sizeof(*request));
	request->index = cpu_to_le32(id);
	if (virtio_msg_xfer(vmdev))
		return false;
	if (le32_to_cpu(response->index) != id || response->reserved)
		return false;

	addr = le64_to_cpu(response->addr);
	len = le64_to_cpu(response->length);
	if (!len || len > U64_MAX - addr)
		return false;

	/* Discovery only: the bus/VM owner must already own this region. */
	region->addr = addr;
	region->len = len;
	return true;
}

static const struct virtio_config_ops virtio_msg_config_ops = {
	.get			= virtio_msg_get,
	.set			= virtio_msg_set,
	.generation		= virtio_msg_generation,
	.get_status		= virtio_msg_get_status,
	.set_status		= virtio_msg_set_status,
	.reset			= virtio_msg_reset,
	.find_vqs		= virtio_msg_find_vqs,
	.del_vqs		= virtio_msg_del_vqs,
	.get_features		= virtio_msg_get_features,
	.finalize_features	= virtio_msg_finalize_features,
	.bus_name		= virtio_msg_bus_name,
	.get_shm_region		= virtio_msg_get_shm_region,
	.synchronize_cbs	= virtio_msg_synchronize_cbs,
};

/**
 * virtio_msg_register - Register a virtio message device with the virtio core
 * @vmdev: Pointer to the virtio message device to register
 *
 * Initializes and registers a virtio message device.
 *
 * Return: 0 on success, or a negative error code on failure.
 */
int virtio_msg_register(struct virtio_msg_device *vmdev)
{
	u32 version;
	int ret;

	if (!vmdev || !vmdev->ops || !vmdev->ops->bus_info ||
	    !vmdev->ops->transfer)
		return -EINVAL;

	vmdev->bus_name = vmdev->ops->bus_info(vmdev, &vmdev->msg_size,
					       &version);
	if (version != VIRTIO_MSG_REVISION_1 ||
	    vmdev->msg_size < VIRTIO_MSG_MIN_SIZE)
		return -EINVAL;

	/*
	 * Allocate per-device request and response buffers, each of size
	 * `msg_size`.
	 *
	 * The request mutex serializes preparation through response consumption,
	 * so one pair of buffers
	 * suffices.
	 */
	vmdev->request = kzalloc(2 * vmdev->msg_size, GFP_KERNEL);
	if (!vmdev->request)
		return -ENOMEM;

	vmdev->response = (void *)vmdev->request + vmdev->msg_size;
	mutex_init(&vmdev->request_lock);

	vmdev->vdev.config = &virtio_msg_config_ops;
	vmdev->vdev.dev.release = virtio_msg_release_dev;

	ret = virtio_msg_get_device_info(vmdev);
	if (ret) {
		virtio_msg_release_dev(&vmdev->vdev.dev);
		return ret;
	}

	ret = register_virtio_device(&vmdev->vdev);
	if (ret)
		put_device(&vmdev->vdev.dev);
	return ret;
}
EXPORT_SYMBOL_GPL(virtio_msg_register);

/**
 * virtio_msg_unregister - Unregister a virtio message device
 * @vmdev: Pointer to the virtio message device to unregister
 *
 * Unregisters a previously registered virtio message device from the virtio
 * core and releases associated resources.
 */
void virtio_msg_unregister(struct virtio_msg_device *vmdev)
{
	unregister_virtio_device(&vmdev->vdev);
}
EXPORT_SYMBOL_GPL(virtio_msg_unregister);

MODULE_AUTHOR("Viresh Kumar <viresh.kumar@linaro.org>");
MODULE_DESCRIPTION("Virtio message transport");
MODULE_LICENSE("GPL");
