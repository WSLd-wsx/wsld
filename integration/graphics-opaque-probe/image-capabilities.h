/* SPDX-License-Identifier: MIT */
/* Query exact external-image profiles; no allocation or presentation claim. */
static int wsx_image_capabilities(PFN_vkGetInstanceProcAddr get,
   VkInstance instance, VkPhysicalDevice physical)
{
   PFN_vkGetPhysicalDeviceImageFormatProperties2 query =
      (PFN_vkGetPhysicalDeviceImageFormatProperties2)get(instance,"vkGetPhysicalDeviceImageFormatProperties2");
   PFN_vkGetPhysicalDeviceExternalSemaphoreProperties semaphore =
      (PFN_vkGetPhysicalDeviceExternalSemaphoreProperties)get(instance,"vkGetPhysicalDeviceExternalSemaphoreProperties");
   if (!query || !semaphore) return 0;
   const VkFormat formats[] = {VK_FORMAT_B8G8R8A8_UNORM,VK_FORMAT_R8G8B8A8_UNORM,
      VK_FORMAT_B8G8R8A8_SRGB,VK_FORMAT_R8G8B8A8_SRGB};
   const VkImageUsageFlags usages[] = {
      VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_SAMPLED_BIT,
      VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_SAMPLED_BIT |
         VK_IMAGE_USAGE_TRANSFER_SRC_BIT | VK_IMAGE_USAGE_TRANSFER_DST_BIT};
   int complete=1;
   printf("{\"image_capability_query\":true,\"handle_type\":%u,\"profiles\":[",
      (unsigned)VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_WIN32_BIT);
   unsigned count=0;
   for (unsigned f=0;f<sizeof(formats)/sizeof(formats[0]);f++)
      for (unsigned t=0;t<2;t++)
         for (unsigned u=0;u<sizeof(usages)/sizeof(usages[0]);u++) {
            VkPhysicalDeviceExternalImageFormatInfo external={
               .sType=VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_EXTERNAL_IMAGE_FORMAT_INFO,
               .handleType=VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_WIN32_BIT};
            VkPhysicalDeviceImageFormatInfo2 info={
               .sType=VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_IMAGE_FORMAT_INFO_2,.pNext=&external,
               .format=formats[f],.type=VK_IMAGE_TYPE_2D,
               .tiling=t ? VK_IMAGE_TILING_LINEAR : VK_IMAGE_TILING_OPTIMAL,.usage=usages[u]};
            VkExternalImageFormatProperties properties={
               .sType=VK_STRUCTURE_TYPE_EXTERNAL_IMAGE_FORMAT_PROPERTIES};
            VkImageFormatProperties2 result={
               .sType=VK_STRUCTURE_TYPE_IMAGE_FORMAT_PROPERTIES_2,.pNext=&properties};
            VkResult status=query(physical,&info,&result);
            if (status!=VK_SUCCESS && status!=VK_ERROR_FORMAT_NOT_SUPPORTED) complete=0;
            /* Outputs are not capability evidence after an unsuccessful call. */
            if (status!=VK_SUCCESS) {
               memset(&result.imageFormatProperties,0,sizeof(result.imageFormatProperties));
               memset(&properties.externalMemoryProperties,0,sizeof(properties.externalMemoryProperties));
            }
            printf("%s{\"format\":%u,\"tiling\":%u,\"usage\":%u,\"flags\":0,"
               "\"vk_result\":%d,\"external_features\":%u,\"compatible_handles\":%u,"
               "\"export_from_imported_handles\":%u,\"max_width\":%u,\"max_height\":%u,"
               "\"sample_counts\":%u}",count++ ? "," : "",(unsigned)info.format,
               (unsigned)info.tiling,info.usage,status,
               properties.externalMemoryProperties.externalMemoryFeatures,
               properties.externalMemoryProperties.compatibleHandleTypes,
               properties.externalMemoryProperties.exportFromImportedHandleTypes,
               result.imageFormatProperties.maxExtent.width,result.imageFormatProperties.maxExtent.height,
               result.imageFormatProperties.sampleCounts);
         }
   VkPhysicalDeviceExternalSemaphoreInfo si={
      .sType=VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_EXTERNAL_SEMAPHORE_INFO,
      .handleType=VK_EXTERNAL_SEMAPHORE_HANDLE_TYPE_OPAQUE_WIN32_BIT};
   VkExternalSemaphoreProperties sp={.sType=VK_STRUCTURE_TYPE_EXTERNAL_SEMAPHORE_PROPERTIES};
   semaphore(physical,&si,&sp);
   printf("],\"binary_semaphore\":{\"handle_type\":%u,\"features\":%u,"
      "\"compatible_handles\":%u,\"export_from_imported_handles\":%u},"
      "\"query_completed\":%s,\"images_created\":0,\"gpu_submissions\":0,"
      "\"zero_copy_presentation_verified\":false}\n",(unsigned)si.handleType,
      sp.externalSemaphoreFeatures,sp.compatibleHandleTypes,sp.exportFromImportedHandleTypes,
      complete ? "true" : "false");
   return complete;
}
