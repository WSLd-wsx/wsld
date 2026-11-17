/* SPDX-License-Identifier: MIT */
/* Stateless Vulkan memory-type filtering. Bits name indices, not properties. */
#ifndef WSX_MEMORY_TYPE_POLICY_H
#define WSX_MEMORY_TYPE_POLICY_H
#include <stdint.h>
#include "venus-protocol/vulkan_core.h"
static inline uint32_t
wsx_memory_types(const VkPhysicalDeviceMemoryProperties *memory, uint32_t resource_bits,
                 uint32_t import_bits, VkMemoryPropertyFlags required)
{
   uint32_t result = 0;
   if (!memory || !memory->memoryTypeCount || memory->memoryTypeCount > VK_MAX_MEMORY_TYPES)
      return 0;
   for (uint32_t i = 0; i < memory->memoryTypeCount; i++) {
      const uint32_t bit = UINT32_C(1) << i;
      if ((resource_bits & import_bits & bit) &&
          (memory->memoryTypes[i].propertyFlags & required) == required)
         result |= bit;
   }
   return result;
}
static inline uint32_t
wsx_choose_host_type(const VkPhysicalDeviceMemoryProperties *memory, uint32_t bits)
{
   uint32_t chosen = UINT32_MAX, score = 0;
   if (!memory || !memory->memoryTypeCount || memory->memoryTypeCount > VK_MAX_MEMORY_TYPES)
      return UINT32_MAX;
   bits = wsx_memory_types(memory, bits, UINT32_MAX, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT);
   for (uint32_t i = 0; i < memory->memoryTypeCount && i < VK_MAX_MEMORY_TYPES; i++) {
      if (!(bits & (UINT32_C(1) << i))) continue;
      const VkMemoryPropertyFlags flags = memory->memoryTypes[i].propertyFlags;
      const uint32_t current = !!(flags & VK_MEMORY_PROPERTY_HOST_COHERENT_BIT) * 2u +
                               !!(flags & VK_MEMORY_PROPERTY_HOST_CACHED_BIT);
      if (chosen == UINT32_MAX || current > score) { chosen = i; score = current; }
   }
   return chosen;
}
#endif
