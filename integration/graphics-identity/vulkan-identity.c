/* Adapted from the WSX Vulkan identity prototype; see PROVENANCE.md. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#define VK_NO_PROTOTYPES
#include "vulkan.h"
#include <stdio.h>
#include <string.h>

/* Return a JSON Boolean when support is known, or null if the query failed. */
static const char *dynamic_rendering(VkPhysicalDevice device, uint32_t api,
                                     uint32_t instance_api,
                                     PFN_vkEnumerateDeviceExtensionProperties extensions,
                                     PFN_vkGetPhysicalDeviceFeatures2 features) {
    if (!features || VK_API_VERSION_VARIANT(api)) return "null";
    if (api < VK_API_VERSION_1_3 || instance_api < VK_API_VERSION_1_3) {
        VkExtensionProperties list[256];
        uint32_t count = sizeof(list) / sizeof(list[0]);
        if (!extensions || extensions(device, NULL, &count, list) != VK_SUCCESS) return "null";
        uint32_t i = 0;
        for (; i < count; ++i)
            if (!strcmp(list[i].extensionName, VK_KHR_DYNAMIC_RENDERING_EXTENSION_NAME)) break;
        if (i == count) return "false";
    }
    VkPhysicalDeviceDynamicRenderingFeatures dr = {.sType=VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_DYNAMIC_RENDERING_FEATURES};
    VkPhysicalDeviceFeatures2 f = {.sType=VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2, .pNext=&dr};
    features(device, &f);
    return dr.dynamicRendering ? "true" : "false";
}

static void bytes(const unsigned char *p, unsigned n) {
    putchar('[');
    for (unsigned i = 0; i < n; ++i) printf("%s%u", i ? "," : "", p[i]);
    putchar(']');
}
static void json_string(const char *s) {
    putchar('"');
    for (const unsigned char *p = (const unsigned char *)s; *p; ++p) {
        if (*p == '"' || *p == '\\') putchar('\\');
        if (*p < 32) printf("\\u%04x", (unsigned)*p); else putchar(*p);
    }
    putchar('"');
}
int wsx_vulkan_identity(void) {
    HMODULE module = LoadLibraryExW(L"vulkan-1.dll", NULL, LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!module) return 1;
    PFN_vkGetInstanceProcAddr get = (PFN_vkGetInstanceProcAddr)GetProcAddress(module, "vkGetInstanceProcAddr");
    PFN_vkCreateInstance create = get ? (PFN_vkCreateInstance)get(NULL, "vkCreateInstance") : NULL;
    if (!create) { FreeLibrary(module); return 1; }
    PFN_vkEnumerateInstanceVersion version = (PFN_vkEnumerateInstanceVersion)get(NULL, "vkEnumerateInstanceVersion");
    uint32_t loader_api = VK_API_VERSION_1_1;
    if (version && version(&loader_api) != VK_SUCCESS) loader_api = VK_API_VERSION_1_1;
    const uint32_t instance_api = loader_api >= VK_API_VERSION_1_3 ? VK_API_VERSION_1_3 : VK_API_VERSION_1_1;
    VkApplicationInfo app = {.sType=VK_STRUCTURE_TYPE_APPLICATION_INFO, .pApplicationName="WSX adapter identity", .apiVersion=instance_api};
    VkInstanceCreateInfo ci = {.sType=VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO, .pApplicationInfo=&app};
    VkInstance instance = VK_NULL_HANDLE;
    if (create(&ci, NULL, &instance) != VK_SUCCESS) { FreeLibrary(module); return 2; }
    PFN_vkDestroyInstance destroy = (PFN_vkDestroyInstance)get(instance, "vkDestroyInstance");
    PFN_vkEnumeratePhysicalDevices enumerate = (PFN_vkEnumeratePhysicalDevices)get(instance, "vkEnumeratePhysicalDevices");
    PFN_vkGetPhysicalDeviceProperties2 props = (PFN_vkGetPhysicalDeviceProperties2)get(instance, "vkGetPhysicalDeviceProperties2");
    PFN_vkGetPhysicalDeviceFeatures2 features = (PFN_vkGetPhysicalDeviceFeatures2)get(instance, "vkGetPhysicalDeviceFeatures2");
    PFN_vkEnumerateDeviceExtensionProperties extensions =
        (PFN_vkEnumerateDeviceExtensionProperties)get(instance, "vkEnumerateDeviceExtensionProperties");
    if (!destroy || !enumerate || !props) { if (destroy) destroy(instance, NULL); FreeLibrary(module); return 3; }
    VkPhysicalDevice devices[16]; uint32_t count = 16;
    if (enumerate(instance, &count, devices) != VK_SUCCESS) { destroy(instance, NULL); FreeLibrary(module); return 3; }
    puts("[");
    for (uint32_t i = 0; i < count; ++i) {
        VkPhysicalDeviceIDProperties id = {.sType=VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_ID_PROPERTIES};
        VkPhysicalDeviceProperties2 p = {.sType=VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2, .pNext=&id};
        props(devices[i], &p);
        printf("%s{\"name\":", i ? "," : ""); json_string(p.properties.deviceName);
        printf(",\"vendor_id\":%u,\"device_id\":%u,\"device_uuid\":", p.properties.vendorID, p.properties.deviceID);
        bytes(id.deviceUUID, VK_UUID_SIZE); printf(",\"driver_uuid\":"); bytes(id.driverUUID, VK_UUID_SIZE);
        printf(",\"luid\":"); bytes(id.deviceLUID, VK_LUID_SIZE);
        printf(",\"node_mask\":%u,\"luid_valid\":%u,\"api_version\":%u,\"dynamic_rendering\":%s}",
               id.deviceNodeMask, id.deviceLUIDValid, p.properties.apiVersion,
               dynamic_rendering(devices[i], p.properties.apiVersion, instance_api, extensions, features));
    }
    puts("]"); destroy(instance, NULL); FreeLibrary(module); return 0;
}
