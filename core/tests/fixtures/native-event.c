/* Inert lifecycle fixture. No window, GPU, HDV, WSL or Guest operations. */
#include <windows.h>
#include <objbase.h>
#include <stdio.h>
#include <wchar.h>
int wmain(int argc, wchar_t **argv) {
    if(argc != 4 || wcscmp(argv[1], L"--direct")) return 3;
    GUID id; wchar_t guid[40], name[96];
    if(CLSIDFromString(argv[3], &id) != S_OK || !StringFromGUID2(&id,guid,40)) return 4;
    if(swprintf(name,96,L"Local\\WSXDirectStop-%ls",guid) < 0) return 5;
    HANDLE event=CreateEventW(NULL,TRUE,FALSE,name);
    if(!event || GetLastError()==ERROR_ALREADY_EXISTS) return 6;
    puts("READY: inert native event fixture"); fflush(stdout);
    DWORD result=WaitForSingleObject(event,45000);
    CloseHandle(event);
    puts(result==WAIT_OBJECT_0 ? "STOP_REQUEST_OBSERVED" : "FIXTURE_TIMEOUT");
    return result==WAIT_OBJECT_0 ? 0 : 2;
}
