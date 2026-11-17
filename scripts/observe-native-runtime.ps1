# Read-only diagnostic collector. Run from local NTFS independently of WSL.
# No Guest commands, service actions, process termination or retirement requests.
param([Parameter(Mandatory=$true)][string]$Receipt,
      [Parameter(Mandatory=$true)][string]$Evidence,
      [ValidateRange(2,3600)][int]$Seconds=600,
      [ValidateRange(250,10000)][int]$IntervalMilliseconds=1000)
$ErrorActionPreference='Stop'
foreach($path in @($Receipt,$Evidence)) {
 if($path -notmatch '^[A-Za-z]:\\'){throw 'Collector paths must be on a local Windows volume'}
}
if(Test-Path -LiteralPath $Evidence){throw 'Evidence directory must be new'}
$record=Get-Content -LiteralPath $Receipt -Raw | ConvertFrom-Json
if(!$record.creation_filetime -or !$record.executable -or !$record.log){throw 'Incomplete Native identity receipt'}
$process=Get-Process -Id $record.pid
if($process.StartTime.ToUniversalTime().ToFileTimeUtc().ToString() -cne $record.creation_filetime -or
   $process.Path -ine $record.executable){throw 'Native identity does not match receipt'}
New-Item -ItemType Directory -Path $Evidence | Out-Null
Copy-Item -LiteralPath $Receipt -Destination (Join-Path $Evidence 'native.json')

function Tail([string]$Path) {
 try {
  $stream=[IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
  try {
   $length=$stream.Length
   $offset=[Math]::Max(0,$length-65536)
   [void]$stream.Seek($offset,[IO.SeekOrigin]::Begin)
   $bytes=New-Object byte[] ([int]($length-$offset))
   $used=0
   while($used -lt $bytes.Length){$count=$stream.Read($bytes,$used,$bytes.Length-$used);if(!$count){break};$used+=$count}
   return @{bytes=$length;last_write_utc=[IO.File]::GetLastWriteTimeUtc($Path).ToString('o');offset=$offset;
    tail=[Text.Encoding]::UTF8.GetString($bytes,0,$used);error=$null}
  } finally {$stream.Dispose()}
 } catch {return @{error=$_.Exception.Message}}
}
$writer=New-Object IO.StreamWriter (Join-Path $Evidence 'samples.jsonl'),$false,(New-Object Text.UTF8Encoding $false)
$clock=[Diagnostics.Stopwatch]::StartNew()
$previous=@{}
try {
 while($clock.Elapsed.TotalSeconds -lt $Seconds) {
  $sample=@{utc=[DateTime]::UtcNow.ToString('o');elapsed_ms=$clock.ElapsedMilliseconds;pid=$record.pid;instance=$record.instance}
  try {
   $process.Refresh()
   if($process.HasExited){$sample.process_exited=$true}
   else {
    $sample.process_exited=$false
    $sample.responding=$process.Responding
    $sample.cpu_ms=$process.TotalProcessorTime.TotalMilliseconds
    $sample.working_set=$process.WorkingSet64
    $sample.private_bytes=$process.PrivateMemorySize64
    $sample.handle_count=$process.HandleCount
    $sample.threads=@($process.Threads | ForEach-Object {
     $thread=@{id=$_.Id;state=$_.ThreadState.ToString()}
     if($_.ThreadState -eq [Diagnostics.ThreadState]::Wait){$thread.wait_reason=$_.WaitReason.ToString()}
     $thread
    })
   }
  } catch {$sample.process_error=$_.Exception.Message}
  foreach($entry in @(@{name='native';path=$record.log},@{name='stderr';path=(Join-Path (Split-Path $Receipt) 'native.stderr')})) {
   $tail=Tail $entry.path
   # Keep changed suffixes only; every sample still records length and timestamp.
   if($null -ne $tail.tail -and $previous[$entry.name] -ceq $tail.tail){$tail.Remove('tail')}
   elseif($null -ne $tail.tail){$previous[$entry.name]=$tail.tail}
   $sample[$entry.name]=$tail
  }
  $writer.WriteLine((ConvertTo-Json -InputObject $sample -Depth 6 -Compress));$writer.Flush()
  if($sample.process_exited){break}
  Start-Sleep -Milliseconds $IntervalMilliseconds
 }
} finally {$writer.Dispose();$process.Dispose()}
@{finished_utc=[DateTime]::UtcNow.ToString('o');scope='Windows process and bounded log snapshots only';
 retirement_proven=$false;responsive_is_desktop_health=$false;guest_commands_executed=$false;
 processes_terminated=$false;elapsed_ms=$clock.ElapsedMilliseconds} |
 ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Evidence 'result.json')
