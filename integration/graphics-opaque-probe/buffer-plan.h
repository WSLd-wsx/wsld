/* SPDX-License-Identifier: MIT */
/* Diagnostic allocation plan only; no graphics resource owner or global state. */
#ifndef WSX_BUFFER_PLAN_H
#define WSX_BUFFER_PLAN_H
#include "memory-type-policy.h"
#include <string.h>
struct wsx_buffer_plan { uint32_t eligible, index; VkDeviceSize size; VkBool32 dedicated; };
static inline VkResult
wsx_plan_buffer(const VkPhysicalDeviceMemoryProperties *memory,
                const VkExternalMemoryProperties *external,
                const VkMemoryRequirements2 req[2],
                const VkMemoryDedicatedRequirements dedicated[2], struct wsx_buffer_plan *plan)
{
   *plan = (struct wsx_buffer_plan){ .index = UINT32_MAX };
   const VkExternalMemoryFeatureFlags required = VK_EXTERNAL_MEMORY_FEATURE_EXPORTABLE_BIT |
                                                 VK_EXTERNAL_MEMORY_FEATURE_IMPORTABLE_BIT;
   if ((external->externalMemoryFeatures & required) != required ||
       !(external->compatibleHandleTypes & VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_WIN32_BIT))
      return VK_ERROR_INVALID_EXTERNAL_HANDLE;
   if (!req[0].memoryRequirements.size || !req[0].memoryRequirements.alignment ||
       req[0].memoryRequirements.size != req[1].memoryRequirements.size ||
       req[0].memoryRequirements.alignment != req[1].memoryRequirements.alignment)
      return VK_ERROR_INITIALIZATION_FAILED;
   plan->eligible = wsx_memory_types(memory, req[0].memoryRequirements.memoryTypeBits,
      req[1].memoryRequirements.memoryTypeBits, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT);
   plan->index = wsx_choose_host_type(memory, plan->eligible);
   if (plan->index == UINT32_MAX) return VK_ERROR_FEATURE_NOT_PRESENT;
   plan->size = req[0].memoryRequirements.size;
   plan->dedicated = !!(external->externalMemoryFeatures & VK_EXTERNAL_MEMORY_FEATURE_DEDICATED_ONLY_BIT) ||
      dedicated[0].requiresDedicatedAllocation || dedicated[1].requiresDedicatedAllocation ||
      dedicated[0].prefersDedicatedAllocation || dedicated[1].prefersDedicatedAllocation;
   return VK_SUCCESS;
}
/* Opaque Vulkan handles must reuse the exporting allocation's exact size/type
 * and device/driver identity. vkGetMemoryWin32HandlePropertiesKHR is prohibited
 * for opaque handle types; it is not a substitute for these original facts. */
static inline VkResult
wsx_check_opaque_import(const struct wsx_buffer_plan *source, VkDeviceSize size,
                        uint32_t index, uint32_t resource_bits,
                        const VkPhysicalDeviceIDProperties *exporter,
                        const VkPhysicalDeviceIDProperties *importer)
{
   if (source->index >= 32 || index != source->index || size != source->size ||
       !(resource_bits & (UINT32_C(1) << index)) ||
       memcmp(exporter->deviceUUID, importer->deviceUUID, VK_UUID_SIZE) ||
       memcmp(exporter->driverUUID, importer->driverUUID, VK_UUID_SIZE))
      return VK_ERROR_INVALID_EXTERNAL_HANDLE;
   return VK_SUCCESS;
}
#endif
