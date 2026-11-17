/* SPDX-License-Identifier: MIT */
#ifndef WSX_KERNEL_LAYOUT_ASSERTS_H
#define WSX_KERNEL_LAYOUT_ASSERTS_H
#include <linux/build_bug.h>
#include <linux/stddef.h>
/* drm_gem_shmem_free() and drm_gem_fb_destroy() free their base argument.
 * container_of() correctness elsewhere does not make interior-pointer frees
 * safe. These assertions deliberately compile against the real driver types. */
static_assert(offsetof(struct virtio_gpu_object, base) == 0,
              "WSX: SHMEM base must equal the allocation start");
static_assert(offsetof(struct virtio_gpu_object_shmem, base) == 0,
              "WSX: SHMEM container must preserve the allocation start");
static_assert(offsetof(struct drm_gem_shmem_object, base) == 0,
              "WSX: GEM base must preserve SHMEM helper ownership");
static_assert(offsetof(struct virtio_gpu_framebuffer, base) == 0,
              "WSX: framebuffer base must equal the allocation start");
#endif
