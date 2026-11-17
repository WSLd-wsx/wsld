// Isolated HCS/HDV experiment. No disks, network, WSL commands or GPU work.
// Only the newly created, retained VM handle can be terminated by this program.
#include <windows.h>
#include <computecore.h>
#include <hypervdevicevirtualization.h>
#include <objbase.h>
#include <cstdio>
#include <cstdlib>
extern "C" HRESULT wsx_probe_reconcile(HANDLE,void*,UINT32,HRESULT);

static void check(const char *stage, HRESULT hr) {
    printf("%s hr=0x%08lx\n",stage,(unsigned long)hr); fflush(stdout);
    if (hr != S_OK) exit(1);
}
static HRESULT CALLBACK initialize(void*) { return S_OK; }
static void CALLBACK teardown(void*) {}
static HRESULT CALLBACK configure(void*,UINT32,const PCWSTR*) { return S_OK; }
static HRESULT CALLBACK details(void*,HDV_PCI_PNP_ID *id,UINT32 count,UINT32 *bars) {
    *id={}; id->VendorID=0x1af4;id->DeviceID=0x1050;
    for(UINT32 i=0;i<count;++i)bars[i]=0; return S_OK;
}
static void CALLBACK stop(void*) {}
static HRESULT CALLBACK read_config(void*,UINT32,UINT32 *value) {*value=0;return S_OK;}
static HRESULT CALLBACK write_config(void*,UINT32,UINT32) {return S_OK;}
static HRESULT CALLBACK read_memory(void*,HDV_PCI_BAR_SELECTOR,UINT64,UINT64,BYTE*) {return E_NOTIMPL;}
static HRESULT CALLBACK write_memory(void*,HDV_PCI_BAR_SELECTOR,UINT64,UINT64,const BYTE*) {return E_NOTIMPL;}
static void query(const char *stage,void *address) {
    MEMORY_BASIC_INFORMATION mbi={};SIZE_T n=VirtualQuery(address,&mbi,sizeof(mbi));
    printf("%s address=%p query_bytes=%zu base=%p size=%zu state=0x%lx type=0x%lx\n",
        stage,address,n,mbi.BaseAddress,mbi.RegionSize,mbi.State,mbi.Type);fflush(stdout);
}
int main() {
    setvbuf(stdout,nullptr,_IONBF,0);
    check("CoInitializeEx",CoInitializeEx(nullptr,COINIT_MULTITHREADED));
    check("CoInitializeSecurity",CoInitializeSecurity(nullptr,-1,nullptr,nullptr,
        RPC_C_AUTHN_LEVEL_DEFAULT,RPC_C_IMP_LEVEL_IMPERSONATE,nullptr,EOAC_NONE,nullptr));
    GUID id;check("CoCreateGuid",CoCreateGuid(&id));WCHAR name[40];StringFromGUID2(id,name,40);
    printf("owned_vm=%ls\n",name);
    const wchar_t *config=LR"({"SchemaVersion":{"Major":2,"Minor":1},"Owner":"WSX isolated aperture test","ShouldTerminateOnLastHandleClosed":true,"VirtualMachine":{"Chipset":{"Uefi":{}},"ComputeTopology":{"Memory":{"Backing":"Physical","SizeInMB":256},"Processor":{"Count":1}}}})";
    HCS_OPERATION operation=HcsCreateOperation(nullptr,nullptr);if(!operation)return 1;
    HCS_SYSTEM system=nullptr;check("HcsCreateComputeSystem",HcsCreateComputeSystem(name,config,operation,nullptr,&system));
    PWSTR result=nullptr;HRESULT hr=HcsWaitForOperationResult(operation,30000,&result);
    if(result){printf("create_result=%ls\n",result);LocalFree(result);}check("create_wait",hr);HcsCloseOperation(operation);
    HMODULE module=LoadLibraryExW(L"vmdevicehost.dll",nullptr,LOAD_LIBRARY_SEARCH_SYSTEM32);if(!module)return 1;
#define RESOLVE(name) auto call_##name=reinterpret_cast<decltype(&name)>(GetProcAddress(module,#name));if(!call_##name)return 1
    RESOLVE(HdvInitializeDeviceHost);RESOLVE(HdvCreateDeviceInstance);
    RESOLVE(HdvCreateGuestMemoryAperture);RESOLVE(HdvDestroyGuestMemoryAperture);RESOLVE(HdvTeardownDeviceHost);
    HDV_HOST host=nullptr;check("initialize_host",call_HdvInitializeDeviceHost(system,&host));
    HDV_PCI_DEVICE_INTERFACE callbacks={HdvPciDeviceInterfaceVersion1,initialize,teardown,configure,details,initialize,stop,read_config,write_config,read_memory,write_memory};
    GUID cls,instance;CoCreateGuid(&cls);CoCreateGuid(&instance);HDV_DEVICE device=nullptr;int context=1;
    check("create_device",call_HdvCreateDeviceInstance(host,HdvDeviceTypePCI,&cls,&instance,&callbacks,&context,&device));
    operation=HcsCreateOperation(nullptr,nullptr);if(!operation)return 1;
    check("start_owned_vm",HcsStartComputeSystem(system,operation,nullptr));
    result=nullptr;hr=HcsWaitForOperationResult(operation,30000,&result);if(result){printf("start_result=%ls\n",result);LocalFree(result);}check("start_wait",hr);HcsCloseOperation(operation);
    WCHAR cls_text[40],instance_text[40],modify[512];StringFromGUID2(cls,cls_text,40);StringFromGUID2(instance,instance_text,40);
    cls_text[37]=0;instance_text[37]=0;
    swprintf_s(modify,L"{\"ResourcePath\":\"VirtualMachine/Devices/FlexibleIov/%ls\",\"RequestType\":\"Add\",\"Settings\":{\"EmulatorId\":\"%ls\",\"HostingModel\":\"External\"}}",instance_text+1,cls_text+1);
    operation=HcsCreateOperation(nullptr,nullptr);if(!operation)return 1;
    check("attach_owned_device",HcsModifyComputeSystem(system,operation,modify,nullptr));
    result=nullptr;hr=HcsWaitForOperationResult(operation,30000,&result);if(result){printf("attach_result=%ls\n",result);LocalFree(result);}check("attach_wait",hr);HcsCloseOperation(operation);
    void *address=nullptr;check("create_aperture",call_HdvCreateGuestMemoryAperture(device,0x200000,4096,FALSE,&address));
    query("before_exit",address);
    HRESULT refused=wsx_probe_reconcile(system,address,4096,HRESULT_FROM_WIN32(ERROR_INVALID_HANDLE));
    printf("live_vm_reconcile hr=0x%08lx\n",(unsigned long)refused);
    if(refused!=HRESULT_FROM_WIN32(ERROR_INVALID_HANDLE))return 1;
    void *normal=nullptr;check("create_normal_aperture",call_HdvCreateGuestMemoryAperture(device,0x201000,4096,FALSE,&normal));
    check("destroy_normal_aperture",call_HdvDestroyGuestMemoryAperture(device,normal));
    operation=HcsCreateOperation(nullptr,nullptr);if(!operation)return 1;
    check("terminate_owned_vm",HcsTerminateComputeSystem(system,operation,nullptr));
    result=nullptr;hr=HcsWaitForOperationResult(operation,30000,&result);if(result)LocalFree(result);check("terminate_wait",hr);HcsCloseOperation(operation);
    result=nullptr;hr=HcsWaitForComputeSystemExit(system,30000,&result);if(result)LocalFree(result);check("exact_vm_exit",hr);
    query("after_exit",address);
    hr=call_HdvDestroyGuestMemoryAperture(device,address);printf("destroy_aperture hr=0x%08lx\n",(unsigned long)hr);
    query("after_destroy",address);
    check("production_reconcile",wsx_probe_reconcile(system,address,4096,hr));
    hr=call_HdvTeardownDeviceHost(host);printf("teardown_host hr=0x%08lx\n",(unsigned long)hr);
    query("after_teardown",address);
    HcsCloseComputeSystem(system);
    // No product retirement claim: this is a diskless VM API observation only.
    return hr==S_OK?0:1;
}
