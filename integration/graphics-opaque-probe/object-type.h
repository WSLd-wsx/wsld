/* SPDX-License-Identifier: MIT */
/* Diagnostic type information only; never map, rename or reinterpret a handle. */
#ifndef WSX_OBJECT_TYPE_H
#define WSX_OBJECT_TYPE_H
#include <windows.h>
#include <winternl.h>
#include <stdint.h>
#include <string.h>

typedef NTSTATUS (NTAPI *wsx_query_object_fn)(HANDLE, OBJECT_INFORMATION_CLASS,
                                           PVOID, ULONG, PULONG);
static inline wsx_query_object_fn wsx_query_object_entry(void)
{
   HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
   FARPROC address = ntdll ? GetProcAddress(ntdll,"NtQueryObject") : NULL;
   wsx_query_object_fn query = NULL;
   _Static_assert(sizeof(query) == sizeof(address), "function pointer size");
   memcpy(&query,&address,sizeof(query));
   return query;
}

static inline int wsx_object_type(wsx_query_object_fn query, HANDLE handle,
                                 char *name, size_t capacity, NTSTATUS *status)
{
   union {
      PUBLIC_OBJECT_TYPE_INFORMATION header;
      unsigned char bytes[4096];
   } output = {0};
   ULONG returned = 0;
   if (!name || !capacity || !status) return 0;
   name[0] = 0;
   *status = (NTSTATUS)0xc000000d; /* STATUS_INVALID_PARAMETER */
   if (!query || !handle || handle == INVALID_HANDLE_VALUE) return 0;
   *status = query(handle,ObjectTypeInformation,&output,sizeof(output),&returned);
   if (*status != 0 || returned < sizeof(output.header) || returned > sizeof(output)) return 0;
   const UNICODE_STRING *type = &output.header.TypeName;
   uintptr_t start = (uintptr_t)type->Buffer, base = (uintptr_t)&output;
   if (!type->Length || type->Length % sizeof(WCHAR) || type->Length > type->MaximumLength ||
       start < base || start - base > returned || type->Length > returned - (start - base) ||
       start % _Alignof(WCHAR) || type->Length / sizeof(WCHAR) >= capacity) return 0;
   /* Kernel object types are identifiers. Reject unexpected characters instead
    * of copying unchecked bytes into the diagnostic's JSON. No object name is queried. */
   size_t count = type->Length / sizeof(WCHAR);
   for (size_t i = 0; i < count; i++) {
      WCHAR c = type->Buffer[i];
      if (!((c >= L'A' && c <= L'Z') || (c >= L'a' && c <= L'z') ||
            (c >= L'0' && c <= L'9') || c == L'_' || c == L'-' || c == L' ')) return 0;
   }
   for (size_t i = 0; i < count; i++) name[i] = (char)type->Buffer[i];
   name[count] = 0;
   return 1;
}
#endif
