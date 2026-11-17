/* Opt-in diagnostics only. A slow call never authorizes cancellation or release. */
#ifndef WSX_NATIVE_CALL_TRACE_H
#define WSX_NATIVE_CALL_TRACE_H
#include <windows.h>
#include <stdint.h>
#include <stdio.h>

struct wsx_call_trace {
    const char *operation;
    ULONGLONG started;
    LONG sequence;
    BOOL enabled;
};

static struct wsx_call_trace wsx_call_begin(const char *operation, UINT64 subject)
{
    DWORD saved=GetLastError();
    WCHAR setting[4]={0};
    static volatile LONG sequence;
    struct wsx_call_trace trace={operation,0,0,FALSE};
    if(GetEnvironmentVariableW(L"WSX_TRACE_NATIVE_CALLS",setting,4)==1 && setting[0]==L'1') {
        FILETIME utc; ULARGE_INTEGER stamp;
        GetSystemTimePreciseAsFileTime(&utc);stamp.LowPart=utc.dwLowDateTime;stamp.HighPart=utc.dwHighDateTime;
        trace.enabled=TRUE;trace.started=GetTickCount64();trace.sequence=InterlockedIncrement(&sequence);
        fprintf(stderr,"WSX_NATIVE_CALL: phase=begin api=%s sequence=%ld pid=%lu tid=%lu utc_filetime=%llu tick_ms=%llu subject=0x%llx\n",
            operation,(long)trace.sequence,(unsigned long)GetCurrentProcessId(),(unsigned long)GetCurrentThreadId(),
            (unsigned long long)stamp.QuadPart,(unsigned long long)trace.started,(unsigned long long)subject);
        fflush(stderr);
    }
    SetLastError(saved);
    return trace;
}

static void wsx_call_end(struct wsx_call_trace trace, HRESULT status)
{
    DWORD saved=GetLastError();
    if(trace.enabled) {
        FILETIME utc; ULARGE_INTEGER stamp;
        GetSystemTimePreciseAsFileTime(&utc);stamp.LowPart=utc.dwLowDateTime;stamp.HighPart=utc.dwHighDateTime;
        fprintf(stderr,"WSX_NATIVE_CALL: phase=end api=%s sequence=%ld pid=%lu tid=%lu utc_filetime=%llu elapsed_ms=%llu hr=0x%08lx\n",
            trace.operation,(long)trace.sequence,(unsigned long)GetCurrentProcessId(),(unsigned long)GetCurrentThreadId(),
            (unsigned long long)stamp.QuadPart,(unsigned long long)(GetTickCount64()-trace.started),(unsigned long)(uint32_t)status);
        fflush(stderr);
    }
    SetLastError(saved);
}
#endif
