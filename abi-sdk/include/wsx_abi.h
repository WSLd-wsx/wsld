#ifndef WSX_ABI_H
#define WSX_ABI_H
#include <stdint.h>
#include <stddef.h>
#ifdef _WIN32
#define WSX_CALL __cdecl
#else
#define WSX_CALL
#endif
#define WSX_ABI_VERSION 2u
#define WSX_CAP_DIAGNOSTICS 1ull
#define WSX_CAP_INVENTORY 2ull
typedef struct WsxSnapshot { uint32_t size, version, progress, state; } WsxSnapshot;
typedef struct WsxHostV2 {
    uint32_t size, version;
    uint64_t capabilities, reserved[2];
    int32_t (WSX_CALL *diagnostic_start)(uint32_t, uint32_t, uint64_t *);
    int32_t (WSX_CALL *task_query)(uint64_t, WsxSnapshot *);
    int32_t (WSX_CALL *task_cancel)(uint64_t);
    int32_t (WSX_CALL *task_release)(uint64_t);
    int32_t (WSX_CALL *error_text)(int32_t, uint8_t *, uint32_t, uint32_t *);
    /* On inventory error -11, a bounded UTF-8 explanation may be returned. */
    int32_t (WSX_CALL *inventory)(uint8_t *, uint32_t, uint32_t *);
} WsxHostV2;
/* Optional separately negotiated launch extension; caller owns all buffers. */
typedef struct WsxLaunchV1 {
    uint32_t size, version;
    uint64_t reserved[2];
    int32_t (WSX_CALL *launch)(const uint8_t *, uint32_t, const uint8_t *, uint32_t, uint32_t, uint8_t *, uint32_t, uint32_t *);
} WsxLaunchV1;
typedef int32_t (WSX_CALL *WsxProAttachLaunchV1)(const WsxLaunchV1 *);
/* Synchronous bounded calls; import is not account initialization. */
typedef struct WsxInstallV1 {
    uint32_t size, version;
    uint64_t reserved[2];
    int32_t (WSX_CALL *online)(uint8_t *, uint32_t, uint32_t *);
    int32_t (WSX_CALL *install)(const uint8_t *, uint32_t, uint8_t *, uint32_t, uint32_t *);
} WsxInstallV1;
typedef int32_t (WSX_CALL *WsxProAttachInstallV1)(const WsxInstallV1 *);
/* Process-local snapshot; buffers are owned by the caller. */
typedef struct WsxInstallProgressV1 {
    uint32_t size, version;
    uint64_t reserved[2];
    int32_t (WSX_CALL *query)(uint8_t *, uint32_t, uint32_t *);
} WsxInstallProgressV1;
typedef int32_t (WSX_CALL *WsxProAttachInstallProgressV1)(const WsxInstallProgressV1 *);
/* Resolve these exports dynamically. No host static import library dependency. */
typedef uint32_t (WSX_CALL *WsxProVersion)(void);
typedef int32_t (WSX_CALL *WsxProInitV2)(const WsxHostV2 *);
typedef int32_t (WSX_CALL *WsxProRunV2)(void);
typedef int32_t (WSX_CALL *WsxProShutdownV2)(void);
#endif
