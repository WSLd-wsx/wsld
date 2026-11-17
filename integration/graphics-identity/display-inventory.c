/* SPDX-License-Identifier: MIT */
/* Read-only Windows display paths; no Vulkan load or display configuration change. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
static void path_json(const wchar_t *s) {
 char text[4096]; int n=WideCharToMultiByte(CP_UTF8,0,s,-1,text,sizeof(text),NULL,NULL);
 if(!n){fputs("null",stdout);return;} putchar('"');
 for(int i=0;i<n-1;i++){unsigned char c=(unsigned char)text[i];if(c=='"'||c=='\\')putchar('\\');if(c<32)printf("\\u%04x",c);else putchar(c);}putchar('"');
}
int wsx_display_inventory(void) {
 UINT32 np=0,nm=0; LONG r=GetDisplayConfigBufferSizes(QDC_ALL_PATHS,&np,&nm);
 if(r||np>4096||nm>4096)return 1;
 DISPLAYCONFIG_PATH_INFO *p=calloc(np,sizeof(*p));DISPLAYCONFIG_MODE_INFO *m=calloc(nm,sizeof(*m));
 if(!p||!m){free(p);free(m);return 2;}
 r=QueryDisplayConfig(QDC_ALL_PATHS,&np,p,&nm,m,NULL);if(r){free(p);free(m);return 3;}
 printf("{\"paths\":[");int sep=0;
 for(UINT32 i=0;i<np;i++)for(int side=0;side<2;side++) {
  LUID id=side?p[i].targetInfo.adapterId:p[i].sourceInfo.adapterId;
  DISPLAYCONFIG_ADAPTER_NAME a={0};a.header.type=DISPLAYCONFIG_DEVICE_INFO_GET_ADAPTER_NAME;a.header.size=sizeof(a);a.header.adapterId=id;
  r=DisplayConfigGetDeviceInfo(&a.header);if(r)continue;
  printf("%s{\"luid\":\"",sep++?",":"");for(unsigned j=0;j<sizeof(id);j++)printf("%02x",((uint8_t*)&id)[j]);
  printf("\",\"active\":%s,\"device_path\":",p[i].flags&DISPLAYCONFIG_PATH_ACTIVE?"true":"false");path_json(a.adapterDevicePath);putchar('}');
 }
 puts("],\"vulkan_loaded\":false,\"gpu_submissions\":0}");free(p);free(m);return 0;
}
