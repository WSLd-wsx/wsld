/* Process fixture for private pipes; never executes WSL or a shell. */
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <io.h>
#include <fcntl.h>
int main(int argc,char **argv)
{
    _setmode(_fileno(stdin),_O_BINARY);_setmode(_fileno(stdout),_O_BINARY);
    if(argc<2)return 2;
    const char *log=getenv("WSX_TEST_LAUNCH_LOG");if(!log)return 3;
    HANDLE mutex=CreateMutexW(NULL,FALSE,L"Local\\WSX-Retirement-Fixture-Log");
    if(!mutex || WaitForSingleObject(mutex,5000)!=WAIT_OBJECT_0)return 4;
    FILE *f=fopen(log,"ab");if(!f)return 4;
    fprintf(f,"%lu %s\n",GetCurrentProcessId(),argv[1]);fclose(f);
    ReleaseMutex(mutex);CloseHandle(mutex);
    int system=!strcmp(argv[1],"--system");
    if(!system && strcmp(argv[1],"--distribution"))return 5;
    puts(system?"WSX_SYSTEM_READY":"WSX_GUEST_READY");fflush(stdout);
    char command[32];if(!fgets(command,sizeof(command),stdin))return 0;
    if(strcmp(command,"retire\n"))return 6;
    const char *fail=getenv("WSX_TEST_HELPER_FAIL");
    if(fail && !strcmp(fail,"1"))return 7;
    printf("fixture_retired=%s\n",system?"system":"guest");return 0;
}
