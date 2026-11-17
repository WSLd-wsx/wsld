/* SPDX-License-Identifier: MIT */
/* Query the real Windows loader, without a logical device or GPU submissions. */
#define WIN32_LEAN_AND_MEAN
#define VK_NO_PROTOTYPES
#include <windows.h>
#include "venus-protocol/vulkan.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void json_string(const char *s)
{
   putchar('"');
   for (const unsigned char *p = (const unsigned char *)s; *p; p++) {
      if (*p == '"' || *p == '\\') putchar('\\');
      if (*p < 32) printf("\\u%04x", *p); else putchar(*p);
   }
   putchar('"');
}

static void hex(const unsigned char *p, size_t n)
{
   putchar('"');
   for (size_t i = 0; i < n; i++) printf("%02x", p[i]);
   putchar('"');
}

int main(void)
{
   wchar_t path[MAX_PATH];
   UINT len = GetSystemDirectoryW(path, MAX_PATH);
   if (!len || len + 14 >= MAX_PATH) return 1;
   wcscat(path, L"\\vulkan-1.dll");
   HMODULE module = LoadLibraryExW(path, NULL, LOAD_LIBRARY_SEARCH_SYSTEM32);
   if (!module) return 2;
   FARPROC address = GetProcAddress(module, "vkGetInstanceProcAddr");
   PFN_vkGetInstanceProcAddr gipa = NULL;
   _Static_assert(sizeof(gipa) == sizeof(address), "Windows function pointer size");
   memcpy(&gipa, &address, sizeof(gipa));
   if (!gipa) return 3;
   PFN_vkCreateInstance create = (PFN_vkCreateInstance)gipa(VK_NULL_HANDLE, "vkCreateInstance");
   if (!create) return 4;
   VkApplicationInfo app = {.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO,
      .pApplicationName = "WSX native maintenance5 audit", .apiVersion = VK_MAKE_API_VERSION(0, 1, 3, 301)};
   VkInstanceCreateInfo ci = {.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO, .pApplicationInfo = &app};
   VkInstance instance = VK_NULL_HANDLE;
   VkResult result = create(&ci, NULL, &instance);
   if (result != VK_SUCCESS) { fprintf(stderr, "vkCreateInstance result=%d\n", result); return 5; }
#define LOAD(name) PFN_vk##name name = (PFN_vk##name)gipa(instance, "vk" #name); if (!name) return 6
   LOAD(DestroyInstance);
   LOAD(EnumeratePhysicalDevices);
   LOAD(EnumerateDeviceExtensionProperties);
   LOAD(GetPhysicalDeviceProperties2);
   LOAD(GetPhysicalDeviceFeatures2);
   LOAD(GetPhysicalDeviceMemoryProperties);
   LOAD(GetPhysicalDeviceFormatProperties);
#undef LOAD
   uint32_t count = 0;
   result = EnumeratePhysicalDevices(instance, &count, NULL);
   if (result != VK_SUCCESS || count > 256) return 7;
   VkPhysicalDevice *devices = calloc(count ? count : 1, sizeof(*devices));
   if (!devices) return 8;
   result = EnumeratePhysicalDevices(instance, &count, devices);
   if (result != VK_SUCCESS) return 9;
   printf("{\"enumeration_result\":%d,\"count\":%u,\"devices\":[", result, count);
   for (uint32_t i = 0; i < count; i++) {
      VkPhysicalDeviceDriverProperties driver = {.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_DRIVER_PROPERTIES};
      VkPhysicalDeviceIDProperties id = {.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_ID_PROPERTIES, .pNext = &driver};
      VkPhysicalDeviceProperties2 props = {.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2, .pNext = &id};
      GetPhysicalDeviceProperties2(devices[i], &props);
      uint32_t n = 0;
      result = EnumerateDeviceExtensionProperties(devices[i], NULL, &n, NULL);
      if (result != VK_SUCCESS || n > 4096) return 10;
      VkExtensionProperties *ext = calloc(n ? n : 1, sizeof(*ext));
      if (!ext) return 11;
      result = EnumerateDeviceExtensionProperties(devices[i], NULL, &n, ext);
      if (result != VK_SUCCESS) return 12;
      int maintenance5 = 0, external_win32 = 0, swapchain = 0;
      for (uint32_t j = 0; j < n; j++) {
         if (!strcmp(ext[j].extensionName, "VK_KHR_maintenance5")) maintenance5 = 1;
         if (!strcmp(ext[j].extensionName, "VK_KHR_external_memory_win32")) external_win32 = 1;
         if (!strcmp(ext[j].extensionName, VK_KHR_SWAPCHAIN_EXTENSION_NAME)) swapchain = 1;
      }
      VkPhysicalDeviceMemoryProperties memory;
      GetPhysicalDeviceMemoryProperties(devices[i], &memory);
      if (memory.memoryTypeCount > VK_MAX_MEMORY_TYPES) return 13;
      uint32_t host_visible = 0;
      for (uint32_t j = 0; j < memory.memoryTypeCount; j++)
         if (memory.memoryTypes[j].propertyFlags & VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT)
            host_visible |= (uint32_t)1u << j;
      VkFormatProperties bgra, rgba;
      GetPhysicalDeviceFormatProperties(devices[i], VK_FORMAT_B8G8R8A8_UNORM, &bgra);
      GetPhysicalDeviceFormatProperties(devices[i], VK_FORMAT_R8G8B8A8_UNORM, &rgba);
      printf("%s{\"name\":", i ? "," : ""); json_string(props.properties.deviceName);
      printf(",\"vendor_id\":%u,\"device_id\":%u,\"api_version\":%u,\"driver_version\":%u,\"driver_name\":",
             props.properties.vendorID, props.properties.deviceID, props.properties.apiVersion, props.properties.driverVersion);
      json_string(driver.driverName); printf(",\"driver_info\":"); json_string(driver.driverInfo);
      printf(",\"luid_valid\":%s,\"luid\":", id.deviceLUIDValid ? "true" : "false"); hex(id.deviceLUID, VK_LUID_SIZE);
      printf(",\"node_mask\":%u,\"device_uuid\":", id.deviceNodeMask); hex(id.deviceUUID, VK_UUID_SIZE);
      printf(",\"driver_uuid\":"); hex(id.driverUUID, VK_UUID_SIZE);
      printf(",\"maintenance5_extension\":%s,\"maintenance5_feature\":", maintenance5 ? "true" : "false");
      if (maintenance5 || props.properties.apiVersion >= VK_API_VERSION_1_4) {
         VkPhysicalDeviceMaintenance5FeaturesKHR m5 = {.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_MAINTENANCE_5_FEATURES_KHR};
         VkPhysicalDeviceFeatures2 features = {.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2, .pNext = &m5};
         GetPhysicalDeviceFeatures2(devices[i], &features);
         printf("%s", m5.maintenance5 ? "true" : "false");
      } else printf("null");
      printf(",\"host_visible_memory_type_bits\":%u,\"bgra_linear_features\":%u,"
             "\"rgba_linear_features\":%u,\"external_memory_win32_extension\":%s,"
             "\"swapchain_extension\":%s", host_visible, bgra.linearTilingFeatures,
             rgba.linearTilingFeatures, external_win32 ? "true" : "false", swapchain ? "true" : "false");
      printf(",\"extension_count\":%u,\"extensions\":[", n);
      for (uint32_t j = 0; j < n; j++) { if (j) putchar(','); json_string(ext[j].extensionName); }
      printf("]}"); free(ext);
   }
   puts("],\"logical_device_created\":false,\"gpu_submissions\":0}");
   free(devices);
   DestroyInstance(instance, NULL);
   FreeLibrary(module);
   return 0;
}
