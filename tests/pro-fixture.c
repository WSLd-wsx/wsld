/* Test-only DLL. Never package this file as Pro. */
#include "../abi-sdk/include/wsx_abi.h"
static WsxHostV2 host;
__declspec(dllexport) uint32_t WSX_CALL wsx_pro_version(void) {
#ifdef BAD_ABI
    return 99;
#else
    return 2;
#endif
}
#ifndef MISSING_EXPORT
__declspec(dllexport) int32_t WSX_CALL wsx_pro_init_v2(const WsxHostV2 *h) {
#ifdef FAIL_INIT
    (void)h; return -4;
#else
    if (!h || h->size != sizeof(*h) || h->version != 2) return -2;
    host=*h; return 0;
#endif
}
#endif
__declspec(dllexport) int32_t WSX_CALL wsx_pro_run_v2(void) {
    uint64_t id=0; uint32_t required=0; uint8_t short_buffer=0xab;
    WsxSnapshot snapshot={sizeof(snapshot),2,0,0};
    if (host.task_query(0,&snapshot)!=-6) return -10;
    if (host.error_text(-6,&short_buffer,1,&required)!=-8 || short_buffer!=0xab) return -10;
    if (host.diagnostic_start(100,0,&id)!=0 || !id) return -10;
    if (host.task_cancel(id)!=0 || host.task_release(id)!=0) return -10;
    if (host.task_query(id,&snapshot)!=-6) return -10;
    return 0;
}
__declspec(dllexport) int32_t WSX_CALL wsx_pro_shutdown_v2(void) { return 0; }
