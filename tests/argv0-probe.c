/* Test-only launcher: verify application path wins over an arbitrary argv[0]. */
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <wchar.h>
int wmain(int argc, wchar_t **argv) {
    if (argc != 3) return 2;
    wchar_t *command = _wcsdup(argv[2]);
    if (!command) return 3;
    STARTUPINFOW startup = {0};
    PROCESS_INFORMATION process = {0};
    startup.cb = sizeof(startup);
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
    startup.hStdOutput = GetStdHandle(STD_OUTPUT_HANDLE);
    startup.hStdError = GetStdHandle(STD_ERROR_HANDLE);
    if (!CreateProcessW(argv[1], command, NULL, NULL, TRUE, 0, NULL, NULL, &startup, &process)) {
        fprintf(stderr, "CreateProcessW failed: %lu\n", GetLastError());
        free(command);
        return 4;
    }
    free(command);
    CloseHandle(process.hThread);
    DWORD wait = WaitForSingleObject(process.hProcess, 10000);
    if (wait != WAIT_OBJECT_0) {
        fprintf(stderr, "Probe did not exit; process retained: %lu\n", process.dwProcessId);
        CloseHandle(process.hProcess);
        return 5;
    }
    DWORD code;
    if (!GetExitCodeProcess(process.hProcess, &code)) return 6;
    CloseHandle(process.hProcess);
    return (int)code;
}
