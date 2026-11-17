/* Windows timing/error-preservation fixture, not a graphics hang reproduction. */
#include "native-call-trace.h"
#include <stdlib.h>
#define CHECK(x) do {if(!(x)){fprintf(stderr,"FAIL line %d: %s\n",__LINE__,#x);exit(1);}} while(0)
int main(void)
{
    CHECK(SetEnvironmentVariableW(L"WSX_TRACE_NATIVE_CALLS",NULL));
    SetLastError(1234);
    struct wsx_call_trace disabled=wsx_call_begin("disabled",0);
    CHECK(GetLastError()==1234 && !disabled.enabled);
    wsx_call_end(disabled,E_FAIL);CHECK(GetLastError()==1234);
    CHECK(SetEnvironmentVariableW(L"WSX_TRACE_NATIVE_CALLS",L"1"));
    SetLastError(5678);
    struct wsx_call_trace slow=wsx_call_begin("fixture-slow",0x1234000);
    CHECK(GetLastError()==5678 && slow.enabled);
    Sleep(25);
    CHECK(GetTickCount64()-slow.started>=20);
    wsx_call_end(slow,HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA));CHECK(GetLastError()==5678);
    struct wsx_call_trace next=wsx_call_begin("fixture-success",0);
    CHECK(next.sequence==slow.sequence+1);
    wsx_call_end(next,S_OK);
    CHECK(SetEnvironmentVariableW(L"WSX_TRACE_NATIVE_CALLS",L"invalid"));
    CHECK(!wsx_call_begin("disabled-invalid",0).enabled);
    puts("PASS: opt-in native timing, stable sequence, elapsed time and LastError preservation; no VM/GPU executed");
    return 0;
}
