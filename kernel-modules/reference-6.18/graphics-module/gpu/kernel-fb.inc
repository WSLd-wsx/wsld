/* SPDX-License-Identifier: MIT */
/* Explicit opaque framebuffer creation, separate from standard ADDFB2 pitch
 * semantics. It retains the original GEM reference and standard DRM lifetime. */
int wsx_opaque_fb_ioctl(struct drm_device *dev, void *data, struct drm_file *file)
{
    struct drm_wsx_opaque_fb *q = data;
    struct virtio_gpu_device *vgdev = dev->dev_private;
    struct drm_gem_object *obj;
    struct virtio_gpu_object *bo;
    struct virtio_gpu_framebuffer *fb;
    const struct wsx_image_contract *c;
    struct drm_mode_fb_cmd2 mode = {0};
    const struct drm_format_info *format;
    int ret;
    if (!virtio_has_feature(vgdev->vdev, WSX_OPAQUE_FEATURE)) return -EOPNOTSUPP;
    if (q->version != WSX_OPAQUE_CONSUMER_ABI || !q->handle || q->fb_id) return -EINVAL;
    obj = drm_gem_object_lookup(file, q->handle);
    if (!obj) return -ENOENT;
    bo = gem_to_virtio_gpu_obj(obj); c = &bo->wsx_contract;
    if (!smp_load_acquire(&bo->wsx_opaque_valid)) { ret = -ENODATA; goto put; }
    if (memcmp(q->instance, bo->wsx_instance, 16) || q->context_epoch != c->context_epoch ||
        q->image_generation != c->image_generation) { ret = -ESTALE; goto put; }
    if (!((c->format == 44 && (q->format == DRM_FORMAT_XRGB8888 || q->format == DRM_FORMAT_ARGB8888)) ||
          (c->format == 37 && (q->format == DRM_FORMAT_XBGR8888 || q->format == DRM_FORMAT_ABGR8888)))) {
        ret = -EINVAL; goto put;
    }
    if (!c->width || !c->height || c->width > dev->mode_config.max_width ||
        c->height > dev->mode_config.max_height || c->allocation_size > obj->size) {
        ret = -EINVAL; goto put;
    }
    format = drm_format_info(q->format);
    if (!format || format->num_planes != 1) { ret = -EINVAL; goto put; }
    fb = kzalloc(sizeof(*fb), GFP_KERNEL);
    if (!fb) { ret = -ENOMEM; goto put; }
    mode.width = c->width; mode.height = c->height; mode.pixel_format = q->format;
    mode.modifier[0] = DRM_FORMAT_MOD_INVALID;
    /* No invented pitch or DRM modifier: the typed scanout uses the descriptor.
     * Standard ADDFB2 remains unchanged and cannot create this framebuffer. */
    ret = virtio_gpu_framebuffer_init(dev, fb, format, &mode, obj);
    if (ret) { kfree(fb); goto put; }
    fb->wsx_opaque = true;
    q->fb_id = fb->base.base.id;
    mutex_lock(&file->fbs_lock);
    list_add(&fb->base.filp_head, &file->fbs);
    mutex_unlock(&file->fbs_lock);
    return 0;
put:
    drm_gem_object_put(obj);
    return ret;
}
