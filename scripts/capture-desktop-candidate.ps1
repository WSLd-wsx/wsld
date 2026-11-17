param([Parameter(Mandatory=$true)][string]$Receipt,[Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class WsxCandidateCapture {
 public delegate bool EnumCallback(IntPtr window, IntPtr context);
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumCallback callback,IntPtr context);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window,out Rect rect);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 public static IntPtr Find(uint pid) { IntPtr found=IntPtr.Zero; EnumWindows((w,c)=>{uint p;GetWindowThreadProcessId(w,out p);if(p==pid&&IsWindowVisible(w)){found=w;return false;}return true;},IntPtr.Zero);return found; }
}
'@
[void][WsxCandidateCapture]::SetProcessDPIAware()
$record=Get-Content -LiteralPath $Receipt -Raw | ConvertFrom-Json
$process=Get-Process -Id $record.pid
if($record.creation_filetime){$same=$process.StartTime.ToUniversalTime().ToFileTimeUtc().ToString() -eq $record.creation_filetime}else{$same=$process.StartTime.ToUniversalTime().ToString('o') -eq $record.started_utc}
if(!$same -or $process.Path -ne $record.executable){throw 'Native receipt identity mismatch'}
$window=[WsxCandidateCapture]::Find($record.pid)
if($window -eq [IntPtr]::Zero){throw 'No visible window belonging to the recorded native Host'}
[void][WsxCandidateCapture]::SetForegroundWindow($window)
Start-Sleep -Milliseconds 500
$rect=New-Object WsxCandidateCapture+Rect
if(![WsxCandidateCapture]::GetWindowRect($window,[ref]$rect)){throw 'Cannot inspect native window rectangle'}
$bitmap=New-Object Drawing.Bitmap ($rect.Right-$rect.Left),($rect.Bottom-$rect.Top)
$graphics=[Drawing.Graphics]::FromImage($bitmap)
try{$graphics.CopyFromScreen($rect.Left,$rect.Top,0,0,$bitmap.Size);$bitmap.Save($Output,[Drawing.Imaging.ImageFormat]::Png)}finally{$graphics.Dispose();$bitmap.Dispose()}
@{pid=$record.pid;instance=$record.instance;window=$window.ToInt64();width=$rect.Right-$rect.Left;height=$rect.Bottom-$rect.Top;screenshot=$Output} | ConvertTo-Json
