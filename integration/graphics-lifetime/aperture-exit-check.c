/* Actual production GuestLease close; HCS/HDV returns are simulated.
 * VirtualAlloc/VirtualQuery use real Windows memory. No VM or GPU is touched. */
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include "guest-memory.h"
static HRESULT vm_status,destroy_status;
static unsigned queries,destroys,releases,checks;
static int query_mode;
static unsigned range_queries;
static HRESULT WINAPI fake_wait(HANDLE system,DWORD timeout,PWSTR *result)
{ if(system!=(HANDLE)123 || timeout)abort();*result=NULL;++queries;return vm_status; }
static HMODULE WINAPI fake_module(LPCWSTR name) {(void)name;return (HMODULE)1;}
static FARPROC WINAPI fake_resolve(HMODULE module,LPCSTR name)
{ (void)module;if(strcmp(name,"HcsWaitForComputeSystemExit"))abort();FARPROC p;void *f=(void *)fake_wait;memcpy(&p,&f,sizeof(p));return p; }
static SIZE_T WINAPI fake_query(LPCVOID address,PMEMORY_BASIC_INFORMATION out,SIZE_T bytes)
{
 if(!query_mode)return VirtualQuery(address,out,bytes);
 memset(out,0,bytes);out->BaseAddress=(void *)address;out->State=MEM_FREE;out->RegionSize=4096;
 if(query_mode==1)return 0;
 if(query_mode==2)return sizeof(*out)-1;
 if(query_mode==3)out->RegionSize=0;
 if(query_mode==4)out->BaseAddress=(char *)address+4096;
 if(query_mode==5){out->BaseAddress=(void *)(UINTPTR_MAX-100);out->RegionSize=4096;}
 if(query_mode==6){out->State=MEM_RESERVE;}
 if(query_mode==7 && range_queries++){out->State=MEM_COMMIT;}
 return sizeof(*out);
}
#define GetModuleHandleW fake_module
#define GetProcAddress fake_resolve
#define VirtualQuery fake_query
#include "guest-memory.c"
#undef GetModuleHandleW
#undef GetProcAddress
#undef VirtualQuery
HRESULT wsx_hdv_host_check_device(const struct wsx_hdv_host *h,HDV_DEVICE d,BOOL mapping)
{(void)mapping;return h->device==d && h->device_references ? S_OK : E_INVALIDARG;}
HRESULT wsx_hdv_host_reference_device(struct wsx_hdv_host *h,HDV_DEVICE *d)
{(void)h;(void)d;return E_NOTIMPL;}
HRESULT wsx_hdv_host_release_device(struct wsx_hdv_host *h,HDV_DEVICE d)
{if(h->device!=d || !h->device_references)abort();--h->device_references;++releases;return S_OK;}
HRESULT WINAPI HdvCreateGuestMemoryAperture(HDV_DEVICE d,UINT64 g,UINT32 n,BOOL r,void **a)
{(void)d;(void)g;(void)n;(void)r;(void)a;return E_NOTIMPL;}
HRESULT WINAPI HdvDestroyGuestMemoryAperture(HDV_DEVICE d,void *p)
{(void)d;(void)p;++destroys;return destroy_status;}
#define CHECK(x) do {++checks;if(!(x)){fprintf(stderr,"FAIL line %d: %s\n",__LINE__,#x);exit(1);}}while(0)
static void attempt(void *address,UINT32 size,HANDLE system,HRESULT expected)
{
 struct wsx_hdv_host h={0};h.device=(HDV_DEVICE)2;h.device_references=1;h.original_system=system;
 struct wsx_hdv_guest_memory m={&h,h.device,address,0x200000,size,TRUE,WSX_HDV_GUEST_MAPPED};
 unsigned before=releases;
 CHECK(wsx_hdv_guest_memory_close(&m)==expected);
 if(expected==S_OK){CHECK(m.phase==WSX_HDV_GUEST_EMPTY && !m.host && !m.address);CHECK(h.device_references==0 && releases==before+1);}
 else{CHECK(m.address==address && m.host==&h && m.phase==WSX_HDV_GUEST_UNCERTAIN);CHECK(h.device_references==1 && releases==before);}
}
int main(void)
{
 HRESULT invalid=HRESULT_FROM_WIN32(ERROR_INVALID_HANDLE);
 void *p=VirtualAlloc(NULL,65536,MEM_COMMIT|MEM_RESERVE,PAGE_READWRITE);CHECK(p);
 vm_status=S_OK;destroy_status=invalid;attempt(p,4096,(HANDLE)123,invalid);
 CHECK(VirtualFree(p,0,MEM_RELEASE));
 attempt(p,4096,(HANDLE)123,S_OK);
 const HRESULT uncertain[]={E_ACCESSDENIED,E_FAIL,S_FALSE,HRESULT_FROM_WIN32(WAIT_TIMEOUT)};
 for(unsigned i=0;i<sizeof(uncertain)/sizeof(*uncertain);++i){vm_status=uncertain[i];attempt(p,4096,(HANDLE)123,invalid);}
 vm_status=S_OK;unsigned before=queries;attempt(p,4096,NULL,invalid);CHECK(queries==before);
 destroy_status=E_ACCESSDENIED;attempt(p,4096,(HANDLE)123,E_ACCESSDENIED);CHECK(queries==before);
 destroy_status=invalid;
 void *reused=VirtualAlloc(p,65536,MEM_RESERVE,PAGE_READWRITE);CHECK(reused==p);
 attempt(p,4096,(HANDLE)123,invalid);CHECK(VirtualFree(reused,0,MEM_RELEASE));
 for(query_mode=1;query_mode<=7;++query_mode){range_queries=0;attempt(p,8192,(HANDLE)123,invalid);}
 query_mode=0;
 destroy_status=S_OK;before=queries;attempt(p,4096,(HANDLE)123,S_OK);CHECK(queries==before);
 printf("aperture exit checks=%u; real Windows address space, simulated HCS/HDV returns\n",checks);
 return 0;
}
