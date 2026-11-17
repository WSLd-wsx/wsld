/* SPDX-License-Identifier: MIT */
/* INVALID is an internal no-modifier sentinel for the private framebuffer
 * ioctl, never a newly advertised modifier. Atomic admission also verifies
 * the framebuffer's private provenance and immutable native descriptor. */
static bool wsx_plane_format_supported(struct drm_plane *plane, u32 format, u64 modifier)
{
    struct virtio_gpu_device *vgdev = plane->dev->dev_private;
    if (modifier == DRM_FORMAT_MOD_LINEAR) return true;
    return modifier == DRM_FORMAT_MOD_INVALID && plane->type == DRM_PLANE_TYPE_PRIMARY &&
        virtio_has_feature(vgdev->vdev, WSX_OPAQUE_FEATURE) &&
        (format == DRM_FORMAT_XRGB8888 || format == DRM_FORMAT_ARGB8888 ||
         format == DRM_FORMAT_XBGR8888 || format == DRM_FORMAT_ABGR8888);
}
static int wsx_plane_check_opaque(struct drm_plane *plane, struct drm_framebuffer *fb)
{
    struct virtio_gpu_framebuffer *vgfb = to_virtio_gpu_framebuffer(fb);
    struct virtio_gpu_object *bo;
    const struct wsx_image_contract *c;
    if (!vgfb->wsx_opaque) return fb->modifier == DRM_FORMAT_MOD_INVALID ? -EINVAL : 0;
    if (plane->type != DRM_PLANE_TYPE_PRIMARY || !fb->obj[0] ||
        fb->modifier != DRM_FORMAT_MOD_INVALID || fb->pitches[0] || fb->offsets[0]) return -EINVAL;
    bo = gem_to_virtio_gpu_obj(fb->obj[0]);
    if (!smp_load_acquire(&bo->wsx_opaque_valid)) return -ESTALE;
    c = &bo->wsx_contract;
    if (fb->width != c->width || fb->height != c->height ||
        !(c->usage & 0x10) || c->allocation_size > fb->obj[0]->size) return -EINVAL;
    return 0;
}
