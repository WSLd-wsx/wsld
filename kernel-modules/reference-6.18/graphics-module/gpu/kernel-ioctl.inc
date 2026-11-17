/* SPDX-License-Identifier: MIT */
/* GEM ownership and standard PRIME fds stay in the original DRM driver. */
static int wsx_opaque_query_ioctl(struct drm_device *dev,void *data,struct drm_file *file)
{
    struct drm_wsx_opaque_query *q=data;
    struct virtio_gpu_device *vgdev=dev->dev_private;
    struct drm_gem_object *obj;
    struct virtio_gpu_object *bo;
    struct wsx_opaque_request req={.magic=WSX_OPAQUE_MAGIC,.version=1,.op=WSX_OPAQUE_HELLO};
    struct wsx_opaque_reply reply;
    const struct wsx_image_contract *c=&reply.contract;
    int ret;
    if (!virtio_has_feature(vgdev->vdev,WSX_OPAQUE_FEATURE)) return -EOPNOTSUPP;
    if (q->version!=1 || q->reserved || !q->handle || !q->width || !q->height ||
        (q->format!=37 && q->format!=44)) return -EINVAL;
    obj=drm_gem_object_lookup(file,q->handle);
    if (!obj) return -ENOENT;
    bo=gem_to_virtio_gpu_obj(obj);
    if (!bo->host3d_blob || !(bo->blob_flags&VIRTGPU_BLOB_FLAG_USE_SHAREABLE)) { ret=-EINVAL;goto put; }
    ret=wsx_opaque_exchange(vgdev,&req,&reply,obj);
    if (ret) goto put;
    if (!memchr_inv(reply.instance,0,16)) { ret=-EPROTO;goto put; }
    req.op=WSX_OPAQUE_DESCRIBE;memcpy(req.instance,reply.instance,16);
    req.resource=bo->hw_res_handle;req.width=q->width;req.height=q->height;req.format=q->format;
    ret=wsx_opaque_exchange(vgdev,&req,&reply,obj);
    if (ret) goto put;
    if (memcmp(reply.instance,req.instance,16) || c->version!=1 || c->struct_size!=sizeof(*c) ||
        c->reserved || c->resource_id!=bo->hw_res_handle || c->width!=q->width ||
        c->height!=q->height || c->format!=q->format || c->tiling || c->view_count>16 ||
        !c->context_epoch || !c->image_generation || !c->image_id || !c->memory_id ||
        c->depth!=1 || c->samples!=1 || c->mip_levels!=1 || c->array_layers!=1 ||
        c->sharing_mode || c->requires_dedicated>1 || c->prefers_dedicated>1 ||
        (c->external_features&6)!=6 || !c->device_node_mask ||
        !memchr_inv(c->device_uuid,0,16) || !memchr_inv(c->driver_uuid,0,16) ||
        memchr_inv(c->view_formats+c->view_count,0,(16-c->view_count)*sizeof(__u32)) ||
        c->handle_type!=2 || c->dedicated!=1 || c->binding_offset ||
        !c->allocation_size || obj->size<c->allocation_size || !c->requirements_size ||
        c->requirements_size>c->allocation_size || !is_power_of_2(c->alignment) ||
        c->memory_type_index>=32 || !(c->memory_type_bits&BIT(c->memory_type_index))) {
        ret=-EPROTO;goto put;
    }
    ret=dma_resv_lock(obj->resv,NULL);
    if (ret) goto put;
    if (smp_load_acquire(&bo->wsx_opaque_valid)) {
        if (memcmp(&bo->wsx_contract,c,sizeof(*c)) || memcmp(bo->wsx_instance,reply.instance,16))
            ret=-ESTALE;
    } else {
        bo->wsx_contract=*c;memcpy(bo->wsx_instance,reply.instance,16);
        smp_store_release(&bo->wsx_opaque_valid,true);
    }
    /* Never rewrite published metadata while KMS is reading it. */
    if (!ret) { q->contract=*c;memcpy(q->instance,reply.instance,16); }
    dma_resv_unlock(obj->resv);
put:
    drm_gem_object_put(obj);
    return ret;
}

/* Read-only lookup distinguishes an ordinary untagged dma-buf (ENODATA) from
 * malformed or stale opaque input. Never reconstruct metadata from stride. */
static int wsx_opaque_lookup_ioctl(struct drm_device *dev, void *data, struct drm_file *file)
{
    struct drm_wsx_opaque_query *q = data;
    struct virtio_gpu_device *vgdev = dev->dev_private;
    struct drm_gem_object *obj;
    struct virtio_gpu_object *bo;
    int ret = 0;
    if (!virtio_has_feature(vgdev->vdev, WSX_OPAQUE_FEATURE)) return -EOPNOTSUPP;
    if (q->version != 1 || q->reserved || !q->handle) return -EINVAL;
    obj = drm_gem_object_lookup(file, q->handle);
    if (!obj) return -ENOENT;
    bo = gem_to_virtio_gpu_obj(obj);
    if (!smp_load_acquire(&bo->wsx_opaque_valid)) ret = -ENODATA;
    else if ((q->width && q->width != bo->wsx_contract.width) ||
             (q->height && q->height != bo->wsx_contract.height) ||
             (q->format && q->format != bo->wsx_contract.format)) ret = -ESTALE;
    else {
        q->contract = bo->wsx_contract;
        q->width = q->contract.width; q->height = q->contract.height; q->format = q->contract.format;
        memcpy(q->instance, bo->wsx_instance, 16);
    }
    drm_gem_object_put(obj);
    return ret;
}
