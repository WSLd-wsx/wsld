/* SPDX-License-Identifier: GPL-2.0-or-later */
#include <linux/cc_platform.h>
#include <linux/dma-map-ops.h>
#include <linux/swiotlb.h>
#include "parent.h"

static void released(struct device *device)
{
    struct wsx_msg_parent *parent=container_of(to_platform_device(device),struct wsx_msg_parent,platform);
    complete_all(&parent->released);
}

static int gpa_profile(struct device *device)
{
    /* This software transport publishes GPA, not a hardware DMA/IOMMU address.
     * Reject profiles requiring translation, bouncing or cache maintenance.
     * device_initialize supplies the standard DMA defaults; no firmware or
     * hardware driver may configure this software transport parent afterwards.
     * The caller keeps it unbound and its DMA identity immutable. */
    if (!IS_ENABLED(CONFIG_X86_64) || cc_platform_has(CC_ATTR_MEM_ENCRYPT) ||
        cc_platform_has(CC_ATTR_GUEST_MEM_ENCRYPT) || get_dma_ops(device) ||
        device->dma_range_map || device->bus_dma_limit ||
        !dev_is_dma_coherent(device) || is_swiotlb_force_bounce(device))
        return -EOPNOTSUPP;
#ifdef CONFIG_IOMMU_DMA
    if (device->dma_iommu)
        return -EOPNOTSUPP;
#endif
    return dma_set_mask_and_coherent(device,DMA_BIT_MASK(64));
}

int wsx_msg_parent_open(struct wsx_msg_parent *parent,struct device *carrier)
{
    int result;
    struct device *device;
    if (!parent || !carrier || parent->initialized || parent->retired)
        return -EINVAL;
    init_completion(&parent->released);
    device=&parent->platform.dev;
    device_initialize(device);
    parent->initialized=true;
    device->parent=carrier;
    device->release=released;
    device->dma_mask=&parent->mask;
    device->dma_parms=&parent->dma;
    parent->platform.name="wsx-virtio-msg";
    parent->platform.id=PLATFORM_DEVID_AUTO;
    result=gpa_profile(device);
    if (result)
        return result;
    /* A real platform subsystem/modalias lets unmodified libdrm and Venus
     * discover the eventual stock DRM child. No platform resources are added. */
    result=platform_device_add(&parent->platform);
    if (result)
        return result;
    parent->added=true;
    return 0;
}

void wsx_msg_parent_retire(struct wsx_msg_parent *parent)
{
    if (!parent || !parent->initialized || parent->retired)
        return;
    parent->retired=true;
    if (parent->added) {
        parent->added=false;
        platform_device_unregister(&parent->platform);
    } else {
        put_device(&parent->platform.dev);
    }
}

void wsx_msg_parent_wait(struct wsx_msg_parent *parent)
{
    if (parent && parent->initialized)
        wait_for_completion(&parent->released);
}

bool wsx_msg_parent_released(struct wsx_msg_parent *parent)
{
    return parent && parent->initialized && completion_done(&parent->released);
}
