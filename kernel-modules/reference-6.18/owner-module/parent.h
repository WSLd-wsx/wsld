/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef WSX_MSG_PARENT_H
#define WSX_MSG_PARENT_H
#include <linux/completion.h>
#include <linux/device.h>
#include <linux/dma-mapping.h>
#include <linux/platform_device.h>

/* Standard platform GPA transport parent, not another DRM device or PCI function.
 * The caller retains this storage/code and the carrier through close, including
 * failed initialization. Zero-initialize once before open. Serialize calls and
 * never republish after close.
 * Its DMA profile is coherent, unencrypted x86-64 direct GPA only. No PCI DMA
 * translation/ops are copied. Device publication alone does not admit graphics.
 */
struct wsx_msg_parent {
    struct platform_device platform;
    struct device_dma_parameters dma;
    struct completion released;
    u64 mask;
    bool initialized, added, retired;
};
int wsx_msg_parent_open(struct wsx_msg_parent *, struct device *carrier);
/* Unpublish/drop the creator reference once; retain storage until wait returns.
 * The virtio child must already be unregistered. Stock DRM may still hold this
 * exact parent. Waiting proves only reference retirement, not remote GPU drain.
 */
void wsx_msg_parent_retire(struct wsx_msg_parent *);
void wsx_msg_parent_wait(struct wsx_msg_parent *);
bool wsx_msg_parent_released(struct wsx_msg_parent *);
#endif
