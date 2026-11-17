/* SPDX-License-Identifier: MIT */
/* Native allocation/export/re-import diagnostic. Never submits GPU commands. */
#define VK_NO_PROTOTYPES
#define VK_USE_PLATFORM_WIN32_KHR
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "venus-protocol/vulkan.h"
#include "buffer-plan.h"
#include "adapter-policy.h"
#include "object-type.h"
#include "image-capabilities.h"

struct dispatch {
   PFN_vkDestroyDevice DestroyDevice;
   PFN_vkCreateBuffer CreateBuffer;
   PFN_vkDestroyBuffer DestroyBuffer;
   PFN_vkGetBufferMemoryRequirements2 GetBufferMemoryRequirements2;
   PFN_vkAllocateMemory AllocateMemory;
   PFN_vkFreeMemory FreeMemory;
   PFN_vkBindBufferMemory BindBufferMemory;
   PFN_vkMapMemory MapMemory;
   PFN_vkUnmapMemory UnmapMemory;
   PFN_vkFlushMappedMemoryRanges FlushMappedMemoryRanges;
   PFN_vkInvalidateMappedMemoryRanges InvalidateMappedMemoryRanges;
   PFN_vkGetMemoryWin32HandleKHR GetMemoryWin32HandleKHR;
};
static int read_elevation(int *elevated)
{
   HANDLE token = NULL;
   TOKEN_ELEVATION elevation = {0}; DWORD bytes = 0;
   if (!OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&token)) return 0;
   int ok = GetTokenInformation(token,TokenElevation,&elevation,sizeof(elevation),&bytes) &&
            bytes == sizeof(elevation);
   if (ok) *elevated = elevation.TokenIsElevated != 0;
   CloseHandle(token); return ok;
}
static void hex(const uint8_t *bytes, size_t count, char *out)
{
   for (size_t i = 0; i < count; i++) sprintf(out+i*2,"%02x",bytes[i]);
}
static int check(uint32_t vendor, uint32_t device_id, uint32_t expected_driver_id, const char *expected_luid, int allow_elevated, int query_images)
{
   const char *phase = "token";
   VkResult result = VK_ERROR_INITIALIZATION_FAILED;
   int passed = 0, instance_created = 0, device_created = 0, allocations = 0;
   int exports = 0, imports = 0, binds = 0, maps = 0, rounds = 0, flushes = 0, invalidates = 0;
   int cleaned = 0, handle_closed = 1;
   int elevated = -1;
   int object_type_queried = 0;
   NTSTATUS object_type_status = (NTSTATUS)0xc0000001;
   char exported_object_type[128] = {0};
   void *section_view = NULL;
   DWORD section_view_error = 0;
   int section_view_attempted = 0, section_view_rounds = 0, section_view_matches = 0;
   int section_view_cleanup = 1;
   uint32_t enumerated = 0, pci_matches = 0, driver_matches = 0, selected_driver_id = 0, duplicate_views = 0;
   char candidates[32768] = {0}; size_t candidate_used = 0;
   DWORD handles_before = 0, handles_after = 0;
   HMODULE library = NULL;
   VkInstance instance = VK_NULL_HANDLE;
   VkDevice device = VK_NULL_HANDLE;
   VkBuffer buffers[2] = {0}; VkDeviceMemory memories[2] = {0}; void *mapped[2] = {0};
   HANDLE exported = NULL, duplicate = NULL;
   PFN_vkDestroyInstance DestroyInstance = NULL;
   struct dispatch vk = {0};
   VkPhysicalDeviceMemoryProperties memory = {0};
   VkPhysicalDeviceProperties properties = {0};
   VkPhysicalDeviceIDProperties identity = {.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_ID_PROPERTIES};
   VkExternalBufferProperties external = {.sType = VK_STRUCTURE_TYPE_EXTERNAL_BUFFER_PROPERTIES};
   VkMemoryRequirements2 requirements[2] = {0};
   VkMemoryDedicatedRequirements dedicated[2] = {0};
   struct wsx_buffer_plan plan = {.index = UINT32_MAX};
   GetProcessHandleCount(GetCurrentProcess(),&handles_before);
   if (!read_elevation(&elevated) || (elevated && !allow_elevated)) goto done;
   phase = "system_loader";
   wchar_t path[MAX_PATH]; UINT length = GetSystemDirectoryW(path,MAX_PATH);
   if (!length || length >= MAX_PATH-14) goto done;
   memcpy(path+length,L"\\vulkan-1.dll",sizeof(L"\\vulkan-1.dll"));
   library = LoadLibraryExW(path,NULL,LOAD_LIBRARY_SEARCH_SYSTEM32);
   if (!library) goto done;
   FARPROC address = GetProcAddress(library,"vkGetInstanceProcAddr");
   PFN_vkGetInstanceProcAddr get;
   _Static_assert(sizeof(get)==sizeof(address),"function pointer size");
   memcpy(&get,&address,sizeof(get)); if (!get) goto done;
   PFN_vkCreateInstance CreateInstance = (PFN_vkCreateInstance)get(NULL,"vkCreateInstance");
   if (!CreateInstance) goto done;
   VkApplicationInfo app = {.sType=VK_STRUCTURE_TYPE_APPLICATION_INFO,
      .pApplicationName="WSX native external buffer check",.apiVersion=VK_API_VERSION_1_1};
   VkInstanceCreateInfo ci = {.sType=VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,.pApplicationInfo=&app};
   phase = "create_instance";
   VkInstance instance_candidate = VK_NULL_HANDLE;
   result = CreateInstance(&ci,NULL,&instance_candidate);
   if (result != VK_SUCCESS) goto done;
   instance = instance_candidate; instance_created = 1;
   DestroyInstance = (PFN_vkDestroyInstance)get(instance,"vkDestroyInstance");
   if (!DestroyInstance) goto done;
#define IP(name) PFN_vk##name name=(PFN_vk##name)get(instance,"vk" #name); if (!name) goto done
   IP(EnumeratePhysicalDevices); IP(GetPhysicalDeviceProperties);
   IP(GetPhysicalDeviceProperties2); IP(GetPhysicalDeviceMemoryProperties);
   IP(GetPhysicalDeviceQueueFamilyProperties); IP(EnumerateDeviceExtensionProperties);
   IP(GetPhysicalDeviceExternalBufferProperties); IP(CreateDevice); IP(GetDeviceProcAddr);
#undef IP
   phase = "adapter_identity";
   VkPhysicalDevice devices[64], physical = VK_NULL_HANDLE;
   uint32_t count = 64;
   result = EnumeratePhysicalDevices(instance,&count,devices);
   if (result != VK_SUCCESS || count > 64) goto done;
   enumerated = count;
   for (uint32_t i = 0; i < count; i++) {
      VkPhysicalDeviceProperties candidate;
      GetPhysicalDeviceProperties(devices[i],&candidate);
      if (candidate.vendorID == vendor && candidate.deviceID == device_id) {
         pci_matches++;
         VkExtensionProperties candidate_extensions[512]; uint32_t extension_count = 512;
         result = EnumerateDeviceExtensionProperties(devices[i],NULL,&extension_count,candidate_extensions);
         if (result != VK_SUCCESS || extension_count > 512) goto done;
         int has_driver_properties = candidate.apiVersion >= VK_API_VERSION_1_2;
         for (uint32_t j = 0; j < extension_count; j++)
            if (!strcmp(candidate_extensions[j].extensionName,VK_KHR_DRIVER_PROPERTIES_EXTENSION_NAME))
               has_driver_properties = 1;
         VkPhysicalDeviceDriverProperties driver = {.sType=VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_DRIVER_PROPERTIES};
         VkPhysicalDeviceIDProperties candidate_identity = {.sType=VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_ID_PROPERTIES,
            .pNext=has_driver_properties ? &driver : NULL};
         if (has_driver_properties && candidate.apiVersion >= VK_API_VERSION_1_1) {
            VkPhysicalDeviceProperties2 detail = {.sType=VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2,.pNext=&candidate_identity};
            GetPhysicalDeviceProperties2(devices[i],&detail);
         }
         char device_uuid[33]={0}, driver_uuid[33]={0}, luid[17]={0};
         hex(candidate_identity.deviceUUID,16,device_uuid); hex(candidate_identity.driverUUID,16,driver_uuid);
         hex(candidate_identity.deviceLUID,8,luid);
         int written = snprintf(candidates+candidate_used,sizeof(candidates)-candidate_used,
            "%s{\"api_version\":%u,\"driver_id\":%u,\"driver_version_raw\":%u,"
            "\"device_uuid\":\"%s\",\"driver_uuid\":\"%s\",\"luid\":\"%s\",\"luid_valid\":%u,\"node_mask\":%u}",
            candidate_used ? "," : "",candidate.apiVersion,(unsigned)driver.driverID,candidate.driverVersion,
            device_uuid,driver_uuid,luid,candidate_identity.deviceLUIDValid,candidate_identity.deviceNodeMask);
         if (written < 0 || (size_t)written >= sizeof(candidates)-candidate_used) goto done;
         candidate_used += (size_t)written;
         if ((!expected_driver_id || (uint32_t)driver.driverID == expected_driver_id) && candidate_identity.deviceLUIDValid &&
             !strcmp(luid,expected_luid)) {
            if (physical && wsx_same_adapter(&properties,&identity,&candidate,&candidate_identity)) {
               duplicate_views++;
            } else {
               driver_matches++;
               if (!physical) {
                  physical = devices[i]; properties = candidate; identity = candidate_identity;
                  identity.pNext = NULL; selected_driver_id = (uint32_t)driver.driverID;
               }
            }
         }
      }
   }
   result = VK_ERROR_INCOMPATIBLE_DRIVER;
   if (driver_matches != 1 || properties.apiVersion < VK_API_VERSION_1_1) goto done;
   VkPhysicalDeviceProperties2 props = {.sType=VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2,.pNext=&identity};
   GetPhysicalDeviceProperties2(physical,&props);
   GetPhysicalDeviceMemoryProperties(physical,&memory);
   if (!memory.memoryTypeCount || memory.memoryTypeCount > VK_MAX_MEMORY_TYPES) goto done;
   phase = "external_memory_win32_extension";
   VkExtensionProperties extensions[512]; count = 512;
   result = EnumerateDeviceExtensionProperties(physical,NULL,&count,extensions);
   if (result != VK_SUCCESS || count > 512) goto done;
   int supported = 0;
   for (uint32_t i = 0; i < count; i++)
      if (!strcmp(extensions[i].extensionName,VK_KHR_EXTERNAL_MEMORY_WIN32_EXTENSION_NAME)) supported = 1;
   result = VK_ERROR_EXTENSION_NOT_PRESENT;
   if (!supported) goto done;
   if (query_images) {
      phase = "external_image_capabilities";
      passed = wsx_image_capabilities(get,instance,physical);
      result = passed ? VK_SUCCESS : VK_ERROR_INITIALIZATION_FAILED;
      goto done;
   }
   const VkBufferUsageFlags usage = VK_BUFFER_USAGE_TRANSFER_SRC_BIT | VK_BUFFER_USAGE_TRANSFER_DST_BIT;
   const VkExternalMemoryHandleTypeFlagBits handle_type = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_WIN32_BIT;
   VkPhysicalDeviceExternalBufferInfo query = {.sType=VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_EXTERNAL_BUFFER_INFO,
      .usage=usage,.handleType=handle_type};
   phase = "exact_buffer_capability";
   GetPhysicalDeviceExternalBufferProperties(physical,&query,&external);
   if ((external.externalMemoryProperties.externalMemoryFeatures & 6) != 6 ||
       !(external.externalMemoryProperties.compatibleHandleTypes & handle_type)) {
      result = VK_ERROR_INVALID_EXTERNAL_HANDLE; goto done;
   }
   VkQueueFamilyProperties families[64]; count = 64;
   GetPhysicalDeviceQueueFamilyProperties(physical,&count,families);
   uint32_t family = UINT32_MAX;
   for (uint32_t i = 0; i < count && i < 64; i++) if (families[i].queueCount) { family=i; break; }
   if (family == UINT32_MAX) goto done;
   float priority = 1;
   VkDeviceQueueCreateInfo queue = {.sType=VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
      .queueFamilyIndex=family,.queueCount=1,.pQueuePriorities=&priority};
   const char *extension = VK_KHR_EXTERNAL_MEMORY_WIN32_EXTENSION_NAME;
   VkDeviceCreateInfo dc = {.sType=VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,.queueCreateInfoCount=1,
      .pQueueCreateInfos=&queue,.enabledExtensionCount=1,.ppEnabledExtensionNames=&extension};
   phase = "create_device";
   VkDevice device_candidate = VK_NULL_HANDLE;
   result = CreateDevice(physical,&dc,NULL,&device_candidate);
   if (result != VK_SUCCESS) goto done;
   device = device_candidate; device_created = 1;
#define DP(name) vk.name=(PFN_vk##name)GetDeviceProcAddr(device,"vk" #name); if (!vk.name) goto done
   phase = "device_entry_points";
   DP(DestroyDevice); DP(CreateBuffer); DP(DestroyBuffer); DP(GetBufferMemoryRequirements2);
   DP(AllocateMemory); DP(FreeMemory); DP(BindBufferMemory); DP(MapMemory); DP(UnmapMemory);
   DP(FlushMappedMemoryRanges); DP(InvalidateMappedMemoryRanges); DP(GetMemoryWin32HandleKHR);
#undef DP
   /* Opaque handles are never passed to vkGetMemoryWin32HandlePropertiesKHR. */
   VkExternalMemoryBufferCreateInfo external_create = {
      .sType=VK_STRUCTURE_TYPE_EXTERNAL_MEMORY_BUFFER_CREATE_INFO,.handleTypes=handle_type};
   VkBufferCreateInfo bc = {.sType=VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,.pNext=&external_create,
      .size=65536,.usage=usage,.sharingMode=VK_SHARING_MODE_EXCLUSIVE};
   for (unsigned i = 0; i < 2; i++) {
      phase = "create_external_buffer";
      VkBuffer candidate = VK_NULL_HANDLE;
      result = vk.CreateBuffer(device,&bc,NULL,&candidate);
      if (result != VK_SUCCESS) goto done;
      buffers[i] = candidate;
      dedicated[i].sType = VK_STRUCTURE_TYPE_MEMORY_DEDICATED_REQUIREMENTS;
      requirements[i].sType = VK_STRUCTURE_TYPE_MEMORY_REQUIREMENTS_2;
      requirements[i].pNext = &dedicated[i];
      VkBufferMemoryRequirementsInfo2 info = {
         .sType=VK_STRUCTURE_TYPE_BUFFER_MEMORY_REQUIREMENTS_INFO_2,.buffer=buffers[i]};
      vk.GetBufferMemoryRequirements2(device,&info,&requirements[i]);
   }
   phase = "host_visible_memory_type_intersection";
   result = wsx_plan_buffer(&memory,&external.externalMemoryProperties,requirements,dedicated,&plan);
   if (result != VK_SUCCESS) goto done;
   VkMemoryDedicatedAllocateInfo allocation_object = {
      .sType=VK_STRUCTURE_TYPE_MEMORY_DEDICATED_ALLOCATE_INFO,.buffer=buffers[0]};
   VkExportMemoryAllocateInfo export_info = {.sType=VK_STRUCTURE_TYPE_EXPORT_MEMORY_ALLOCATE_INFO,
      .pNext=plan.dedicated ? &allocation_object : NULL,.handleTypes=handle_type};
   /* Omit optional Win32 export attributes: specification guarantees read/write
    * access, default security and non-inheritance for OPAQUE_WIN32. */
   VkMemoryAllocateInfo allocation = {.sType=VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
      .pNext=&export_info,.allocationSize=plan.size,.memoryTypeIndex=plan.index};
   VkDeviceMemory memory_candidate = VK_NULL_HANDLE;
   phase = "allocate_exportable_memory";
   result = vk.AllocateMemory(device,&allocation,NULL,&memory_candidate);
   if (result != VK_SUCCESS) goto done;
   memories[0] = memory_candidate; allocations++;
   phase = "bind_export_buffer";
   result = vk.BindBufferMemory(device,buffers[0],memories[0],0);
   if (result != VK_SUCCESS) goto done;
   binds++;
   VkMemoryGetWin32HandleInfoKHR export_handle = {.sType=VK_STRUCTURE_TYPE_MEMORY_GET_WIN32_HANDLE_INFO_KHR,
      .memory=memories[0],.handleType=handle_type};
   HANDLE candidate_handle = NULL;
   phase = "export_opaque_win32";
   result = vk.GetMemoryWin32HandleKHR(device,&export_handle,&candidate_handle);
   if (result != VK_SUCCESS) goto done;
   exported = candidate_handle;
   if (!exported || exported == INVALID_HANDLE_VALUE) { exported=NULL; result=VK_ERROR_INVALID_EXTERNAL_HANDLE; goto done; }
   exports++;
   phase = "exported_object_type";
   object_type_queried = wsx_object_type(wsx_query_object_entry(),exported,
      exported_object_type,sizeof(exported_object_type),&object_type_status);
   if (!object_type_queried) goto done;
   /* A Section type alone does not identify its contents. Inspect only a
    * read-only view at offset zero; never write driver metadata or seek offsets. */
   if (!strcmp(exported_object_type,"Section")) {
      section_view_attempted = 1;
      section_view = MapViewOfFile(exported,FILE_MAP_READ,0,0,65536);
      if (section_view) section_view_matches = 1;
      else section_view_error = GetLastError();
   }
   phase = "duplicate_owned_handle";
   if (!DuplicateHandle(GetCurrentProcess(),exported,GetCurrentProcess(),&duplicate,0,FALSE,DUPLICATE_SAME_ACCESS)) {
      result=VK_ERROR_INVALID_EXTERNAL_HANDLE; goto done;
   }
   phase = "original_import_metadata";
   result = wsx_check_opaque_import(&plan,allocation.allocationSize,allocation.memoryTypeIndex,
      requirements[1].memoryRequirements.memoryTypeBits,&identity,&identity);
   if (result != VK_SUCCESS) goto done;
   VkMemoryDedicatedAllocateInfo import_object = {
      .sType=VK_STRUCTURE_TYPE_MEMORY_DEDICATED_ALLOCATE_INFO,.buffer=buffers[1]};
   VkImportMemoryWin32HandleInfoKHR import_info = {.sType=VK_STRUCTURE_TYPE_IMPORT_MEMORY_WIN32_HANDLE_INFO_KHR,
      .pNext=plan.dedicated ? &import_object : NULL,.handleType=handle_type,.handle=duplicate};
   allocation.pNext = &import_info; /* Exact original size/type; no re-export. */
   phase = "import_opaque_win32";
   memory_candidate = VK_NULL_HANDLE;
   result = vk.AllocateMemory(device,&allocation,NULL,&memory_candidate);
   if (result != VK_SUCCESS) goto done;
   memories[1] = memory_candidate; allocations++; imports++;
   /* Win32 imports reference the payload but never consume caller HANDLEs. */
   if (!CloseHandle(duplicate)) { handle_closed=0; goto done; }
   duplicate = NULL;
   phase = "bind_import_buffer";
   result = vk.BindBufferMemory(device,buffers[1],memories[1],0);
   if (result != VK_SUCCESS) goto done;
   binds++;
   for (unsigned i = 0; i < 2; i++) {
      phase = i ? "map_import" : "map_export";
      void *candidate = NULL;
      result = vk.MapMemory(device,memories[i],0,VK_WHOLE_SIZE,0,&candidate);
      if (result != VK_SUCCESS) goto done;
      mapped[i] = candidate; maps++;
      if (!mapped[i]) { result=VK_ERROR_MEMORY_MAP_FAILED; goto done; }
   }
   phase = "bidirectional_cpu_visibility";
   for (unsigned round = 0; round < 16; round++) {
      unsigned from = round & 1, to = from ^ 1;
      volatile uint8_t *writer = mapped[from], *reader = mapped[to];
      for (unsigned n = 0; n < 65536; n++) writer[n] = (uint8_t)(n*17+round);
      MemoryBarrier();
      if (!(memory.memoryTypes[plan.index].propertyFlags & VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)) {
         VkMappedMemoryRange range = {.sType=VK_STRUCTURE_TYPE_MAPPED_MEMORY_RANGE,
            .memory=memories[from],.offset=0,.size=VK_WHOLE_SIZE};
         result = vk.FlushMappedMemoryRanges(device,1,&range); flushes++;
         if (result != VK_SUCCESS) goto done;
         range.memory = memories[to];
         result = vk.InvalidateMappedMemoryRanges(device,1,&range); invalidates++;
         if (result != VK_SUCCESS) goto done;
      }
      for (unsigned n = 0; n < 65536; n++)
         if (reader[n] != (uint8_t)(n*17+round)) { result=VK_ERROR_MEMORY_MAP_FAILED; goto done; }
      if (section_view_matches) {
         const volatile uint8_t *section_reader = section_view;
         for (unsigned n = 0; n < 65536; n++) {
            if (section_reader[n] != (uint8_t)(n*17+round)) {
               section_view_matches = 0;
               break;
            }
         }
         if (section_view_matches) section_view_rounds++;
      }
      rounds++;
   }
   passed = 1; phase = "complete";
done:
   if (section_view && !UnmapViewOfFile(section_view)) section_view_cleanup = 0;
   if (device) {
      for (unsigned i = 0; i < 2; i++) if (mapped[i] && vk.UnmapMemory) vk.UnmapMemory(device,memories[i]);
      for (unsigned i = 0; i < 2; i++) if (buffers[i] && vk.DestroyBuffer) vk.DestroyBuffer(device,buffers[i],NULL);
      for (unsigned i = 0; i < 2; i++) if (memories[i] && vk.FreeMemory) vk.FreeMemory(device,memories[i],NULL);
   }
   if (duplicate && !CloseHandle(duplicate)) handle_closed=0;
   if (exported && !CloseHandle(exported)) handle_closed=0;
   if (device && vk.DestroyDevice) vk.DestroyDevice(device,NULL);
   if (instance && DestroyInstance) DestroyInstance(instance,NULL);
   cleaned = (!device || vk.DestroyDevice) && (!instance || DestroyInstance) && handle_closed && section_view_cleanup;
   if (library && cleaned && !FreeLibrary(library)) cleaned=0;
   GetProcessHandleCount(GetCurrentProcess(),&handles_after);
   char uuid[33]={0}, driver_uuid[33]={0}, selected_luid[17]={0}, flags[512]={0};
   hex(identity.deviceUUID,16,uuid); hex(identity.driverUUID,16,driver_uuid);
   hex(identity.deviceLUID,8,selected_luid);
   size_t used=0;
   for (unsigned i=0; i<memory.memoryTypeCount && i<VK_MAX_MEMORY_TYPES; i++)
      used+=(size_t)snprintf(flags+used,sizeof(flags)-used,"%s%u",i ? "," : "",memory.memoryTypes[i].propertyFlags);
   printf("{\"pass\":%s,\"phase\":\"%s\",\"vk_result\":%d,\"vendor_id\":%u,\"device_id\":%u,"
      "\"api_version\":%u,\"driver_version_raw\":%u,\"device_uuid\":\"%s\",\"driver_uuid\":\"%s\","
      "\"memory_type_flags\":[%s],\"external_features\":%u,\"compatible_handle_types\":%u,"
      "\"export_buffer_type_bits\":%u,\"import_buffer_type_bits\":%u,\"eligible_host_visible_bits\":%u,"
      "\"memory_type_index\":%u,\"allocation_size\":%llu,\"dedicated\":%s,"
      "\"instance_created\":%s,\"device_created\":%s,\"allocations\":%d,\"exports\":%d,\"imports\":%d,"
      "\"bindings\":%d,\"mappings\":%d,\"cpu_rounds\":%d,\"flushes\":%d,\"invalidates\":%d,"
      "\"cleanup_completed\":%s,\"handles_before\":%lu,\"handles_after\":%lu,"
      "\"allow_elevated\":%s,\"token_elevated\":%d,"
      "\"adapter_luid\":\"%s\","
      "\"object_type_queried\":%s,\"object_type_status\":%u,\"exported_object_type\":\"%s\",\"export_is_section\":%s,"
      "\"section_view_attempted\":%s,\"section_view_mapped\":%s,\"section_view_error\":%lu,"
      "\"section_view_offset\":0,\"section_view_length\":65536,\"section_view_read_only\":true,"
      "\"section_view_matching_rounds\":%d,\"section_view_matches_buffer\":%s,\"section_view_cleanup_completed\":%s,"
      "\"enumerated_devices\":%u,\"pci_matches\":%u,\"driver_matches\":%u,\"driver_id\":%u,\"duplicate_views\":%u,\"pci_candidates\":[%s],"
      "\"opaque_handle_property_queries\":0,\"gpu_submissions\":0,\"guest_mapping_verified\":false}\n",
      passed && cleaned ? "true":"false",phase,result,properties.vendorID,properties.deviceID,
      properties.apiVersion,properties.driverVersion,uuid,driver_uuid,flags,
      external.externalMemoryProperties.externalMemoryFeatures,external.externalMemoryProperties.compatibleHandleTypes,
      requirements[0].memoryRequirements.memoryTypeBits,requirements[1].memoryRequirements.memoryTypeBits,
      plan.eligible,plan.index,(unsigned long long)plan.size,plan.dedicated ? "true":"false",
      instance_created ? "true":"false",device_created ? "true":"false",allocations,exports,imports,binds,maps,rounds,
      flushes,invalidates,cleaned ? "true":"false",(unsigned long)handles_before,(unsigned long)handles_after,
      allow_elevated ? "true":"false",elevated,selected_luid,
      object_type_queried ? "true":"false",(unsigned)object_type_status,exported_object_type,
      object_type_queried && !strcmp(exported_object_type,"Section") ? "true":"false",
      section_view_attempted ? "true":"false",section_view ? "true":"false",(unsigned long)section_view_error,
      section_view_rounds,section_view_matches && section_view_rounds == 16 ? "true":"false",section_view_cleanup ? "true":"false",
      enumerated,pci_matches,driver_matches,selected_driver_id,duplicate_views,candidates);
   return passed && cleaned ? 0 : 1;
}
int main(int argc, char **argv)
{
   if (argc == 2 && !strcmp(argv[1],"--check")) {
      puts("{\"invocation_check\":true,\"vulkan_loaded\":false}"); return 0;
   }
   unsigned vendor, device, driver_id; char extra;
   int query_images = argc > 1 && !strcmp(argv[1],"--query-images");
   if ((argc != 6 && argc != 7) || (argc == 7 && strcmp(argv[6],"--allow-elevated")) ||
       (!query_images && strcmp(argv[1],"--allocate")) ||
       sscanf(argv[2],"%x%c",&vendor,&extra) != 1 || sscanf(argv[3],"%x%c",&device,&extra) != 1 ||
       sscanf(argv[4],"%u%c",&driver_id,&extra) != 1 || !vendor || !device ||
       strlen(argv[5]) != 16 || strspn(argv[5],"0123456789abcdef") != 16) {
      fputs("Explicit --allocate <vendor hex> <device hex> <Vulkan driver ID> <LUID bytes hex> [--allow-elevated] required\n",stderr); return 2;
   }
   return check(vendor,device,driver_id,argv[5],argc == 7,query_images);
}
