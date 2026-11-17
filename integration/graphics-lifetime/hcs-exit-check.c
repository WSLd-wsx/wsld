/* Isolated HCS return-policy fixture; no VM or GPU is created or stopped. */
#include <windows.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
static HRESULT exit_status, operation_status;
static unsigned exit_queries, operation_closes;
static HRESULT WINAPI fake_exit(HANDLE system,DWORD timeout,PWSTR *result)
{ (void)system;(void)timeout;*result=NULL;++exit_queries;return exit_status; }
static HRESULT WINAPI fake_operation(HANDLE operation,DWORD timeout,PWSTR *result)
{ (void)operation;(void)timeout;*result=NULL;return operation_status; }
static void WINAPI fake_close(HANDLE operation) { (void)operation;++operation_closes; }
static FARPROC fake_resolve(HMODULE module,LPCSTR name)
{
 (void)module;FARPROC result=NULL;
 if(!strcmp(name,"HcsWaitForComputeSystemExit")){void *p=(void *)fake_exit;memcpy(&result,&p,sizeof(result));}
 if(!strcmp(name,"HcsWaitForOperationResult")){void *p=(void *)fake_operation;memcpy(&result,&p,sizeof(result));}
 if(!strcmp(name,"HcsCloseOperation")){void *p=(void *)fake_close;memcpy(&result,&p,sizeof(result));}
 return result;
}
#define GetProcAddress fake_resolve
#include "direct-owner.c"
#undef GetProcAddress
#define CHECK(x) do {if(!(x)){fprintf(stderr,"FAIL line %d: %s\n",__LINE__,#x);exit(1);}} while(0)
int main(void)
{
 struct wsx_direct_owner owner={0};
 owner.module=(HMODULE)1;owner.system=(HANDLE)2;owner.thread=GetCurrentThreadId();
 owner.phase=WSX_DIRECT_ATTACHED;
 CHECK(DuplicateHandle(GetCurrentProcess(),GetCurrentThread(),GetCurrentProcess(),&owner.thread_reference,SYNCHRONIZE,FALSE,0));
 const HRESULT pending[]={HRESULT_FROM_WIN32(WAIT_TIMEOUT),HRESULT_FROM_WIN32(ERROR_TIMEOUT),(HRESULT)0x80370117L,(HRESULT)0x80370118L};
 for(unsigned i=0;i<sizeof(pending)/sizeof(pending[0]);++i){exit_status=pending[i];CHECK(wsx_direct_vm_exited(&owner)==S_FALSE);CHECK(!owner.vm_exited);CHECK(wsx_direct_revoke_exited_vm(&owner)==S_FALSE);CHECK(owner.phase==WSX_DIRECT_ATTACHED);}
 exit_status=E_ACCESSDENIED;CHECK(wsx_direct_vm_exited(&owner)==E_ACCESSDENIED);CHECK(!owner.vm_exited);
 exit_status=S_OK;CHECK(wsx_direct_vm_exited(&owner)==S_OK);CHECK(owner.vm_exited);
 unsigned queries=exit_queries;exit_status=E_FAIL;CHECK(wsx_direct_vm_exited(&owner)==S_OK);CHECK(exit_queries==queries);
 owner.operation=(HANDLE)3;operation_status=pending[0];CHECK(wsx_direct_revoke_exited_vm(&owner)==S_FALSE);CHECK(owner.operation);CHECK(!operation_closes);
 operation_status=E_FAIL;CHECK(wsx_direct_revoke_exited_vm(&owner)==S_OK);CHECK(!owner.operation);CHECK(operation_closes==1);CHECK(owner.phase==WSX_DIRECT_REMOVED);
 owner.thread=GetCurrentThreadId()+1;CHECK(wsx_direct_vm_exited(&owner)==RPC_E_WRONG_THREAD);
 CloseHandle(owner.thread_reference);
 puts("PASS: HCS timeout/error retain authority; confirmed exact VM exit is latched; pending operation retained; wrong thread rejected; no actual VM/GPU executed");
 return 0;
}
