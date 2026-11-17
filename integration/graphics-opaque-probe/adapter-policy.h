/* SPDX-License-Identifier: MIT */
#ifndef WSX_ADAPTER_POLICY_H
#define WSX_ADAPTER_POLICY_H
#include <string.h>
#include "venus-protocol/vulkan.h"

/* Collapse duplicate enumeration views only with complete matching identities. */
static inline int wsx_same_adapter(const VkPhysicalDeviceProperties *a,
                                  const VkPhysicalDeviceIDProperties *ai,
                                  const VkPhysicalDeviceProperties *b,
                                  const VkPhysicalDeviceIDProperties *bi)
{
   const uint8_t zero[VK_UUID_SIZE] = {0};
   return a->vendorID == b->vendorID && a->deviceID == b->deviceID &&
      a->apiVersion == b->apiVersion && a->driverVersion == b->driverVersion &&
      a->deviceType == b->deviceType && ai->deviceLUIDValid && bi->deviceLUIDValid &&
      ai->deviceNodeMask && !(ai->deviceNodeMask & (ai->deviceNodeMask - 1)) &&
      ai->deviceNodeMask == bi->deviceNodeMask &&
      memcmp(ai->deviceUUID,zero,VK_UUID_SIZE) && memcmp(ai->driverUUID,zero,VK_UUID_SIZE) &&
      !memcmp(ai->deviceUUID,bi->deviceUUID,VK_UUID_SIZE) &&
      !memcmp(ai->driverUUID,bi->driverUUID,VK_UUID_SIZE) &&
      !memcmp(ai->deviceLUID,bi->deviceLUID,VK_LUID_SIZE);
}
#endif
