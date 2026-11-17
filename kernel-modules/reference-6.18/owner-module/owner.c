/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Exact-device ownership around the existing virtio-msg and kernel vsock bus. */
#include <linux/completion.h>
#include <linux/hyperv.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/pci.h>
#include <linux/slab.h>
#include <linux/uuid.h>
#include <linux/vm_sockets.h>
#include <linux/virtio_gpu.h>
#include "vsock.h"
#include "parent.h"

/* ABI 2: bounded configuration admission is separate from binding. */
#define OWNER_LIMIT 16
struct configuration {
    guid_t instance;
    unsigned int domain, bus, slot, function, port, bar, number;
    unsigned long long size;
    bool used, active;
};
static struct configuration configurations[OWNER_LIMIT];
static DEFINE_MUTEX(configuration_lock);

struct owner {
    struct configuration config;
    struct wsx_msg_parent parent;
    struct virtio_msg_device device;
    struct virtio_msg_ops ops;
    struct vmsg_vsock *bus;
    struct mutex retirement;
    struct completion released;
    bool registered, retired;
};
static bool address_matches(struct pci_dev *device,const struct configuration *config)
{
    return pci_domain_nr(device->bus)==config->domain && device->bus->number==config->bus &&
        PCI_SLOT(device->devfn)==config->slot && PCI_FUNC(device->devfn)==config->function;
}
static bool identity(struct pci_dev *device,const struct configuration *config)
{
    struct device *parent;
    if (!address_matches(device,config)) return false;
    for (parent=device->dev.parent;parent;parent=parent->parent)
        if (parent->bus && !strcmp(parent->bus->name,"vmbus"))
            return guid_equal(&device_to_hv_device(parent)->dev_instance,&config->instance);
    return false;
}
static void forget_configuration(const struct configuration *config)
{
    int i;
    mutex_lock(&configuration_lock);
    for (i=0;i<OWNER_LIMIT;i++)
        if (configurations[i].used && guid_equal(&configurations[i].instance,&config->instance)) {
            memset(&configurations[i],0,sizeof(configurations[i])); break;
        }
    mutex_unlock(&configuration_lock);
}
static ssize_t configuration_abi_show(struct device_driver *driver,char *buffer)
{
    (void)driver;
    return sysfs_emit(buffer,"2\n");
}
static DRIVER_ATTR_RO(configuration_abi);
static ssize_t configure_store(struct device_driver *driver,const char *buffer,size_t count)
{
    struct configuration config={0};
    char instance[37],extra;
    int i,free_slot=-1,result=0;
    (void)driver;
    if (sscanf(buffer,"%x:%x:%x.%x %36s %u %u %llu %u %c",
        &config.domain,&config.bus,&config.slot,&config.function,instance,
        &config.port,&config.bar,&config.size,&config.number,&extra)!=9 ||
        config.domain>U16_MAX || config.bus>U8_MAX || config.slot>31 || config.function>7 ||
        !config.port || config.port>=U32_MAX-1 || !config.number || config.number>U16_MAX ||
        (config.bar!=0 && config.bar!=2 && config.bar!=4) ||
        config.size<PAGE_SIZE || !is_power_of_2(config.size) ||
        guid_parse(instance,&config.instance) || guid_is_null(&config.instance)) return -EINVAL;
    mutex_lock(&configuration_lock);
    for (i=0;i<OWNER_LIMIT;i++) {
        const struct configuration *c=&configurations[i];
        if (!c->used) { if(free_slot<0) free_slot=i; continue; }
        if (guid_equal(&c->instance,&config.instance) ||
            (c->domain==config.domain && c->bus==config.bus && c->slot==config.slot && c->function==config.function) ||
            (c->port<=config.port+1 && config.port<=c->port+1)) { result=-EEXIST; break; }
    }
    if (!result) {
        if(free_slot<0) result=-ENOSPC;
        else {config.used=true;configurations[free_slot]=config;}
    }
    mutex_unlock(&configuration_lock);
    return result ? result : (ssize_t)count;
}
static DRIVER_ATTR_WO(configure);

static int queues_ready(struct virtio_msg_device *device)
{
    struct owner *owner=container_of(device,struct owner,device);
    return vmsg_vsock_enable_events(owner->bus);
}
static void release(struct virtio_msg_device *device)
{
    struct owner *owner=container_of(device,struct owner,device);
    /* No bus teardown on an arbitrary reference-dropping thread. */
    complete_all(&owner->released);
}
static void retire(struct owner *owner)
{
    bool held=false;
    if (owner->retired)
        return;
    if (owner->registered) {
        /* Preserve the core request buffers and virtio storage while late DRM
         * references retire. device_del drops the child's hierarchy reference,
         * so this storage reference does not keep the parent alive in a cycle. */
        get_device(&owner->device.vdev.dev);
        held=true;
        virtio_msg_unregister(&owner->device);
        owner->registered=false;
    }
    wsx_msg_parent_retire(&owner->parent);
    wsx_msg_parent_wait(&owner->parent);
    if (held)
        put_device(&owner->device.vdev.dev);
    /* The locked virtio-msg core installs release before device-info discovery
     * or registration, and calls it even on failure from those stages. Earlier
     * validation/allocation failures have no initialized device or callback.
     * A callback not observed yet is never interpreted as absent ownership. */
    if (owner->device.vdev.dev.release)
        wait_for_completion(&owner->released);
    if (owner->bus) {
        vmsg_vsock_destroy(owner->bus);
        owner->bus=NULL;
    }
    /* This is local reference/I/O retirement, not remote GPU/HDV drain or an
     * independent proof of all client callbacks. Production admission remains
     * closed pending the full trusted Guest/provider shutdown contract. */
    owner->retired=true;
}
static ssize_t retire_store(struct device *device,struct device_attribute *attribute,
    const char *buffer,size_t count)
{
    struct owner *owner=dev_get_drvdata(device);
    bool requested;
    int result=kstrtobool(buffer,&requested);
    (void)attribute;
    if (result || !requested)
        return -EINVAL;
    if (mutex_lock_interruptible(&owner->retirement))
        return -ERESTARTSYS;
    retire(owner);
    mutex_unlock(&owner->retirement);
    return count;
}
static DEVICE_ATTR_WO(retire);

static ssize_t configuration_show(struct device *device,struct device_attribute *attribute,char *buffer)
{
    struct owner *owner=dev_get_drvdata(device);
    const struct configuration *c=&owner->config;
    (void)attribute;
    return sysfs_emit(buffer,"%pUl %u %u %llu %u\n",&c->instance,c->port,c->bar,c->size,c->number);
}
static DEVICE_ATTR_RO(configuration);

static int probe(struct pci_dev *device,const struct pci_device_id *id)
{
    struct owner *owner;
    struct configuration config={0};
    int i;
    unsigned long flags;
    int result;
    (void)id;
    mutex_lock(&configuration_lock);
    for(i=0;i<OWNER_LIMIT;i++)
        if(configurations[i].used && !configurations[i].active && identity(device,&configurations[i])) {
            configurations[i].active=true;config=configurations[i];break;
        }
    mutex_unlock(&configuration_lock);
    if(!config.used) return -ENODEV;
    flags=pci_resource_flags(device,config.bar);
    if ((flags&(IORESOURCE_MEM|IORESOURCE_MEM_64|IORESOURCE_PREFETCH))!=
        (IORESOURCE_MEM|IORESOURCE_MEM_64|IORESOURCE_PREFETCH) ||
        pci_resource_len(device,config.bar)!=config.size)
        {result=-EINVAL;goto forget;}
    result=pci_enable_device_mem(device);
    if (result)
        goto forget;
    result=dma_set_mask_and_coherent(&device->dev,DMA_BIT_MASK(64));
    if (result)
        goto disable;
    pci_set_master(device);
    owner=kzalloc(sizeof(*owner),GFP_KERNEL);
    if (!owner) { result=-ENOMEM;goto clear; }
    owner->config=config;
    mutex_init(&owner->retirement);
    init_completion(&owner->released);
    if (!try_module_get(THIS_MODULE)) { result=-ENODEV;goto free; }
    result=wsx_msg_parent_open(&owner->parent,&device->dev);
    if (result)
        goto retire_owner;
    owner->ops=*vmsg_vsock_get_ops();
    owner->ops.release=release;
    owner->ops.queues_ready=queues_ready;
    owner->device.ops=&owner->ops;
    owner->device.dev_id=config.number;
    owner->device.vdev.dev.parent=&owner->parent.platform.dev;
    owner->bus=vmsg_vsock_connect(&owner->device,VMADDR_CID_HOST,config.port,52,msecs_to_jiffies(15000));
    if (IS_ERR(owner->bus)) {
        result=PTR_ERR(owner->bus);owner->bus=NULL;goto retire_owner;
    }
    result=vmsg_vsock_bind_shm_parent(owner->bus,device,&owner->parent.platform.dev,
        VIRTIO_GPU_SHM_ID_HOST_VISIBLE,config.bar);
    if (result)
        goto retire_owner;
    pci_set_drvdata(device,owner);
    result=virtio_msg_register(&owner->device);
    if (result) {
        pci_set_drvdata(device,NULL);
        goto retire_owner;
    }
    owner->registered=true;
    result=device_create_file(&device->dev,&dev_attr_retire);
    if (result) {
        pci_set_drvdata(device,NULL);
        goto retire_owner;
    }
    result=device_create_file(&device->dev,&dev_attr_configuration);
    if(result) {
        device_remove_file(&device->dev,&dev_attr_retire);
        pci_set_drvdata(device,NULL);goto retire_owner;
    }
    /* Do not claim the memory region here: stock virtio-gpu owns its mmap
     * aperture claim. PCI decode and local range identity remain with PCI. */
    return 0;
retire_owner:
    retire(owner);
    module_put(THIS_MODULE);
free:
    kfree(owner);
clear:
    pci_clear_master(device);
disable:
    pci_disable_device(device);
forget:
    forget_configuration(&config);
    return result;
}
static void remove_device(struct pci_dev *device)
{
    struct owner *owner=pci_get_drvdata(device);
    device_remove_file(&device->dev,&dev_attr_configuration);
    device_remove_file(&device->dev,&dev_attr_retire);
    mutex_lock(&owner->retirement);
    retire(owner);
    mutex_unlock(&owner->retirement);
    pci_set_drvdata(device,NULL);
    pci_clear_master(device);
    pci_disable_device(device);
    forget_configuration(&owner->config);
    kfree(owner);
    module_put(THIS_MODULE);
}
static struct pci_driver driver={
    .name="wsx-virtio-msg-pci",
    .probe=probe,
    .remove=remove_device,
    .driver={.suppress_bind_attrs=true},
};
static int __init initialize(void)
{
    int result=pci_register_driver(&driver);
    if(result) return result;
    result=driver_create_file(&driver.driver,&driver_attr_configuration_abi);
    if(result) goto unregister;
    result=driver_create_file(&driver.driver,&driver_attr_configure);
    if(!result) return 0;
    driver_remove_file(&driver.driver,&driver_attr_configuration_abi);
unregister:
    pci_unregister_driver(&driver);return result;
}
static void __exit finish(void)
{
    driver_remove_file(&driver.driver,&driver_attr_configure);
    driver_remove_file(&driver.driver,&driver_attr_configuration_abi);
    pci_unregister_driver(&driver);
}
module_init(initialize);
module_exit(finish);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Exact-device PCI owner for the isolated virtio-msg Hyper-V route");

MODULE_INFO(wsx_owner_configuration_abi,"2");
