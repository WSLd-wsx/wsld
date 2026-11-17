/* SPDX-License-Identifier: GPL-2.0-or-later */
#include <linux/errno.h>
#include <linux/mm.h>
#include "pci-shm.h"

static int vmsg_pci_shm_range(struct pci_dev *dev, unsigned int bar, u64 *base, u64 *length)
{
    unsigned long flags;
    u64 start, end;
    if (!dev || bar >= PCI_STD_NUM_BARS || (bar & 1))
        return -EINVAL;
    if (!pci_is_enabled(dev) || pci_dev_is_disconnected(dev))
        return -ENODEV;
    flags = pci_resource_flags(dev, bar);
    if ((flags & (IORESOURCE_MEM | IORESOURCE_MEM_64 | IORESOURCE_PREFETCH)) !=
        (IORESOURCE_MEM | IORESOURCE_MEM_64 | IORESOURCE_PREFETCH) ||
        (flags & (IORESOURCE_IO | IORESOURCE_UNSET | IORESOURCE_DISABLED)))
        return -EINVAL;
    start = pci_resource_start(dev, bar);
    end = pci_resource_end(dev, bar);
    if (!start || end < start || end == U64_MAX ||
        !IS_ALIGNED(start, PAGE_SIZE) || !IS_ALIGNED(end + 1, PAGE_SIZE))
        return -ERANGE;
    *base = start;
    *length = end - start + 1;
    return 0;
}

int vmsg_pci_shm_bind(struct vmsg_pci_shm *region, struct pci_dev *dev,
                      struct device *parent, u8 id, unsigned int bar)
{
    u64 base, length;
    int error;
    if (!region || !dev || parent != &dev->dev)
        return -EINVAL;
    if (region->device)
        return -EBUSY;
    error = vmsg_pci_shm_range(dev, bar, &base, &length);
    if (error)
        return error;
    *region = (struct vmsg_pci_shm){ .device = pci_dev_get(dev), .base = base,
        .length = length, .bar = bar, .id = id };
    return 0;
}

int vmsg_pci_shm_validate(const struct vmsg_pci_shm *region, u32 id, u64 base, u64 length)
{
    u64 live_base, live_length;
    int error;
    if (!region || !region->device)
        return -ENXIO;
    error = vmsg_pci_shm_range(region->device, region->bar, &live_base, &live_length);
    if (error)
        return error;
    if (live_base != region->base || live_length != region->length)
        return -ESTALE;
    if (id != region->id || base != region->base || length != region->length)
        return -ERANGE;
    return 0;
}

void vmsg_pci_shm_put(struct vmsg_pci_shm *region)
{
    if (region->device)
        pci_dev_put(region->device);
    *region = (struct vmsg_pci_shm){0};
}
