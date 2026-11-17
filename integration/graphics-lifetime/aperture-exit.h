/* VM-loss reconciliation for an owned Guest RAM aperture. No guessed unmap. */
#ifndef WSX_APERTURE_EXIT_H
#define WSX_APERTURE_EXIT_H
#include <windows.h>
#include <stdint.h>
#include <string.h>
#include <stdio.h>

typedef HRESULT (WINAPI *wsx_wait_vm_exit_fn)(HANDLE,DWORD,PWSTR *);

/* The original HCS handle is borrowed from the retained direct bootstrap.
 * Never reopen by VM name: that could inspect a replacement generation. */
static HRESULT wsx_aperture_vm_exit(HANDLE original_system)
{
    if(!original_system || original_system==INVALID_HANDLE_VALUE)return E_INVALIDARG;
    HMODULE module=GetModuleHandleW(L"computecore.dll");
    if(!module)return E_UNEXPECTED;
    FARPROC raw=GetProcAddress(module,"HcsWaitForComputeSystemExit");
    if(!raw)return E_NOTIMPL;
    wsx_wait_vm_exit_fn wait;
    _Static_assert(sizeof(wait)==sizeof(raw),"HCS exit ABI");
    memcpy(&wait,&raw,sizeof(wait));
    PWSTR result=NULL;HRESULT hr=wait(original_system,0,&result);
    if(result)LocalFree(result);
    return hr;
}

/* The old aperture may already have been revoked by VM teardown. A failed HDV
 * call alone never proves that. Require successful exact-VM exit AND every byte
 * of the original CPU range to be MEM_FREE. Reserved, committed, reused, unknown
 * or partially free ranges stay retained. No VirtualFree/UnmapViewOfFile guesses.
 * The caller has already exclusively retired Rust views and drained GPU users.
 * This discharges only this CPU aperture, not GPU/Section/HDV/callback barriers.
 */
static HRESULT wsx_aperture_reconcile_exit(HANDLE original_system,void *address,
                                         UINT32 bytes,HRESULT destroy_status)
{
    if(destroy_status!=HRESULT_FROM_WIN32(ERROR_INVALID_HANDLE))return destroy_status;
    if(!address || !bytes || (uintptr_t)address>UINTPTR_MAX-bytes)return destroy_status;
    HRESULT exited=wsx_aperture_vm_exit(original_system);
    if(exited!=S_OK)return destroy_status;
    uintptr_t cursor=(uintptr_t)address,end=cursor+bytes;
    while(cursor<end){
        MEMORY_BASIC_INFORMATION info={0};
        SIZE_T n=VirtualQuery((void *)cursor,&info,sizeof(info));
        uintptr_t base=(uintptr_t)info.BaseAddress;
        if(n!=sizeof(info) || info.State!=MEM_FREE || base>cursor ||
           !info.RegionSize || base>UINTPTR_MAX-info.RegionSize ||
           base+info.RegionSize<=cursor){
            fprintf(stderr,"WSX_APERTURE_EXIT: retained address=%p bytes=%lu query_bytes=%llu state=0x%lx destroy_hr=0x%08lx\n",
                address,(unsigned long)bytes,(unsigned long long)n,(unsigned long)info.State,(unsigned long)(uint32_t)destroy_status);
            return destroy_status;
        }
        cursor=base+info.RegionSize;
    }
    fprintf(stderr,"WSX_APERTURE_EXIT: revoked address=%p bytes=%lu source=owned-hcs-handle+mem-free destroy_hr=0x%08lx\n",
        address,(unsigned long)bytes,(unsigned long)(uint32_t)destroy_status);
    return S_OK;
}
#endif
