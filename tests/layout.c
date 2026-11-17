#include "../abi-sdk/include/wsx_abi.h"
_Static_assert(sizeof(WsxHostV2)==80, "HostV2 size");
_Static_assert(_Alignof(WsxHostV2)==8, "HostV2 alignment");
_Static_assert(offsetof(WsxHostV2, diagnostic_start)==32, "Diagnostic callback offset");
_Static_assert(offsetof(WsxHostV2, inventory)==72, "Inventory callback offset");
_Static_assert(sizeof(WsxLaunchV1)==32, "LaunchV1 size");
_Static_assert(offsetof(WsxLaunchV1, launch)==24, "LaunchV1 callback offset");
_Static_assert(sizeof(WsxSnapshot)==16, "Snapshot size");
int main(void) { return WSX_ABI_VERSION == 2 ? 0 : 1; }
