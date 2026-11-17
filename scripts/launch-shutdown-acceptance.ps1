# Launch the Windows-local collector outside a WSL interop job/console.
# This does not arm or execute shutdown; the collector waits for its run ID.
param([Parameter(Mandatory=$true)][string]$Directory,
      [Parameter(Mandatory=$true)][string]$Receipt,
      [Parameter(Mandatory=$true)][string]$RunId)
$ErrorActionPreference='Stop'
foreach($path in @($Directory,$Receipt)){
 if($path -notmatch '^[A-Za-z]:\\' -or $path.Contains('"')){throw 'Expected a local Windows path'}
}
if(([guid]$RunId).ToString('D') -cne $RunId){throw 'Expected a canonical run ID'}
Add-Type @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
public static class WsxShutdownCollectorLauncher {
 [StructLayout(LayoutKind.Sequential)] struct SI {
  public uint cb; public IntPtr reserved,desktop,title;
  public uint x,y,width,height,xChars,yChars,fill,flags;
  public ushort show,reservedSize;public IntPtr reservedBytes,input,output,error;
 }
 [StructLayout(LayoutKind.Sequential)] struct PI {public IntPtr process,thread;public uint pid,tid;}
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]
 static extern bool CreateProcess(string app,StringBuilder command,IntPtr ps,IntPtr ts,bool inherit,uint flags,IntPtr env,string cwd,ref SI si,out PI pi);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool IsProcessInJob(IntPtr process,IntPtr job,out bool result);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
 public static uint Launch(string app,string command,string cwd) {
  SI si=new SI();si.cb=(uint)Marshal.SizeOf(typeof(SI));si.flags=1;si.show=0;PI pi;
  // BREAKAWAY_FROM_JOB | NEW_CONSOLE. A hidden independent console supplies
  // valid console handles to Windows PowerShell without inheriting WSL pipes.
  if(!CreateProcess(app,new StringBuilder(command),IntPtr.Zero,IntPtr.Zero,false,0x01000010,IntPtr.Zero,cwd,ref si,out pi))
   throw new Win32Exception(Marshal.GetLastWin32Error());
  try {
   bool inJob;
   if(!IsProcessInJob(pi.process,IntPtr.Zero,out inJob))throw new Win32Exception(Marshal.GetLastWin32Error());
   if(inJob)throw new InvalidOperationException("Collector remains in a job; do not arm shutdown");
   return pi.pid;
  } finally {CloseHandle(pi.thread);CloseHandle(pi.process);}
 }
}
'@
$app=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$script=Join-Path $Directory 'test-wsl-shutdown.ps1'
$evidence=Join-Path $Directory 'result'
$core=Join-Path $Directory 'package\wsld.exe'
$command='"'+$app+'" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "'+$script+'" -Receipt "'+$Receipt+'" -Evidence "'+$evidence+'" -CoreExe "'+$core+'" -RunId '+$RunId
$childPid=[WsxShutdownCollectorLauncher]::Launch($app,$command,$Directory)
$record=@{collector_pid=$childPid;outside_job=$true;console_mode='independent_hidden_console';run_id=$RunId;directory=$Directory;receipt=$Receipt;shutdown_armed=$false}
$record | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Directory 'collector-launch.json')
$record | ConvertTo-Json
