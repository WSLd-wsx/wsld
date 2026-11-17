/* Compile the unchanged production C policy into the isolated real-VM probe. */
#include "aperture-exit.h"
HRESULT wsx_probe_reconcile(HANDLE system,void *address,UINT32 bytes,HRESULT status)
{return wsx_aperture_reconcile_exit(system,address,bytes,status);}
