/* SPDX-License-Identifier: MIT */
/* Included after the pinned control-queue helpers; not a second transport. */
#include <linux/completion.h>
#include <linux/kref.h>
#if __BYTE_ORDER__ != __ORDER_LITTLE_ENDIAN__
#error The paired WSX opaque wire adapter currently requires a little-endian Guest
#endif
struct wsx_opaque_wait {
    struct kref refs;
    struct completion done;
    struct wsx_opaque_reply reply;
    int result;
    struct virtio_gpu_object_array *objects;
    struct virtio_gpu_device *vgdev;
};
static void wsx_opaque_wait_free(struct kref *ref)
{
    struct wsx_opaque_wait *wait = container_of(ref,struct wsx_opaque_wait,refs);
    /* Final GEM release may enqueue UNREF. Use the existing deferred worker
     * so a response callback never blocks its own control queue. */
    if (wait->objects) virtio_gpu_array_put_free_delayed(wait->vgdev,wait->objects);
    kfree(wait);
}
static void wsx_opaque_response(struct virtio_gpu_device *vgdev,
                                struct virtio_gpu_vbuffer *vbuf)
{
    struct wsx_opaque_wait *wait=vbuf->resp_cb_data;
    struct virtio_gpu_ctrl_hdr *hdr=(void *)vbuf->resp_buf;
    (void)vgdev;
    if (vbuf->wsx_response_length==sizeof(*hdr)+sizeof(wait->reply) &&
        le32_to_cpu(hdr->type)==WSX_OPAQUE_RESPONSE && !hdr->flags &&
        !hdr->fence_id && !hdr->ctx_id && !hdr->ring_idx && !memchr_inv(hdr->padding,0,sizeof(hdr->padding))) {
        memcpy(&wait->reply,hdr+1,sizeof(wait->reply));
        wait->result=0;
    }
    complete(&wait->done);
    kref_put(&wait->refs,wsx_opaque_wait_free);
}
int wsx_opaque_exchange(struct virtio_gpu_device *vgdev,
                        const struct wsx_opaque_request *request,
                        struct wsx_opaque_reply *reply, struct drm_gem_object *object)
{
    struct wsx_opaque_wait *wait;
    struct virtio_gpu_vbuffer *vbuf;
    struct virtio_gpu_ctrl_hdr *hdr;
    void *response,*data;
    int ret;
    if (!virtio_has_feature(vgdev->vdev,WSX_OPAQUE_FEATURE)) return -EOPNOTSUPP;
    wait=kzalloc(sizeof(*wait),GFP_KERNEL);
    response=kzalloc(sizeof(*hdr)+sizeof(*reply),GFP_KERNEL);
    data=kmemdup(request,sizeof(*request),GFP_KERNEL);
    if (!wait || !response || !data) { kfree(wait);kfree(response);kfree(data);return -ENOMEM; }
    if (object) {
        wait->objects=virtio_gpu_array_alloc(1);
        if (!wait->objects) { kfree(wait);kfree(response);kfree(data);return -ENOMEM; }
        virtio_gpu_array_add_obj(wait->objects,object);
        wait->vgdev=vgdev;
    }
    kref_init(&wait->refs);init_completion(&wait->done);wait->result=-EPROTO;
    hdr=virtio_gpu_alloc_cmd_resp(vgdev,wsx_opaque_response,&vbuf,sizeof(*hdr),
                                 sizeof(*hdr)+sizeof(*reply),response);
    hdr->type=cpu_to_le32(WSX_OPAQUE_COMMAND);
    vbuf->data_buf=data;vbuf->data_size=sizeof(*request);vbuf->resp_cb_data=wait;
    kref_get(&wait->refs);
    ret=virtio_gpu_queue_ctrl_buffer(vgdev,vbuf);
    if (ret) {
        /* The queue helper frees vbuf/data/response on enqueue failure. */
        kref_put(&wait->refs,wsx_opaque_wait_free);
        goto out;
    }
    virtio_gpu_notify(vgdev);
    if (!wait_for_completion_timeout(&wait->done,5*HZ)) { ret=-ETIMEDOUT;goto out; }
    ret=wait->result;
    if (ret) goto out;
    *reply=wait->reply;
    if (reply->magic!=WSX_OPAQUE_MAGIC || reply->version!=1 || reply->op!=request->op ||
        reply->reserved || (reply->capabilities&7)!=7 || (reply->capabilities&~7)) ret=-EPROTO;
    else if (reply->status) ret=-EREMOTEIO;
out:
    /* A timed-out response still owns its wait storage until the real callback.
     * No response or GPU completion is synthesized and no queue is reset. */
    kref_put(&wait->refs,wsx_opaque_wait_free);
    return ret;
}
int wsx_opaque_scanout(struct virtio_gpu_device *vgdev, unsigned scanout,
                       struct virtio_gpu_object *bo, struct drm_framebuffer *fb,
                       unsigned w,unsigned h,unsigned x,unsigned y)
{
    struct wsx_opaque_request req={.magic=WSX_OPAQUE_MAGIC,.version=1,.op=WSX_OPAQUE_SCANOUT};
    struct wsx_opaque_reply reply;
    const struct wsx_image_contract *c=&bo->wsx_contract;
    int ret;
    if (scanout || !smp_load_acquire(&bo->wsx_opaque_valid)) return -EINVAL;
    if (fb->width!=c->width || fb->height!=c->height || fb->offsets[0] ||
        fb->format->num_planes!=1 || !w || !h || x>=c->width || y>=c->height ||
        w>c->width-x || h>c->height-y) return -EINVAL;
    if ((c->format==44 && fb->format->format!=DRM_FORMAT_ARGB8888 && fb->format->format!=DRM_FORMAT_XRGB8888) ||
        (c->format==37 && fb->format->format!=DRM_FORMAT_ABGR8888 && fb->format->format!=DRM_FORMAT_XBGR8888) ||
        (c->format!=37 && c->format!=44)) return -EINVAL;
    req.resource=bo->hw_res_handle;req.width=c->width;req.height=c->height;req.format=c->format;
    req.crop[0]=x;req.crop[1]=y;req.crop[2]=w;req.crop[3]=h;req.contract=*c;
    memcpy(req.instance,bo->wsx_instance,16);
    ret=wsx_opaque_exchange(vgdev,&req,&reply,&bo->base.base);
    if (!ret && (memcmp(reply.instance,req.instance,16) ||
        memcmp(&reply.contract,c,sizeof(*c)))) ret=-ESTALE;
    drm_dbg(vgdev->ddev,"WSX opaque scanout resource=%u epoch=%llu generation=%llu result=%d\n",
            bo->hw_res_handle,c->context_epoch,c->image_generation,ret);
    return ret;
}
