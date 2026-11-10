param(
 [string]$ReleaseTag='v1.0.32',
 [string]$BaselineTag='v1.0.31',
 [string]$ReportDirectory=(Join-Path $env:RUNNER_TEMP 'pastenext-native-report')
)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
New-Item -ItemType Directory -Force -Path $ReportDirectory | Out-Null
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class StartupProbe {
 public delegate bool EnumProc(IntPtr window,IntPtr data);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc proc,IntPtr data);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
 [DllImport("user32.dll")] static extern bool IsHungAppWindow(IntPtr window);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window,StringBuilder value,int size);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window,StringBuilder value,int size);
 [DllImport("user32.dll")] static extern void keybd_event(byte key,byte scan,uint flags,UIntPtr extra);
 public static string[] Windows(uint process) {
  var result=new List<string>();
  EnumWindows((h,d)=>{uint p;GetWindowThreadProcessId(h,out p);if(p==process){var t=new StringBuilder(256);var c=new StringBuilder(256);GetWindowText(h,t,256);GetClassName(h,c,256);result.Add("visible="+IsWindowVisible(h)+" hung="+IsHungAppWindow(h)+" class="+c+" title="+t);}return true;},IntPtr.Zero);
  return result.ToArray();
 }
 public static void PanelShortcut() {
  foreach(byte key in new byte[]{0x11,0x12,0x56})keybd_event(key,0,0,UIntPtr.Zero);
  foreach(byte key in new byte[]{0x56,0x12,0x11})keybd_event(key,0,2,UIntPtr.Zero);
 }
}
'@
$root=Join-Path $env:RUNNER_TEMP ('pastenext-portable-startup-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $root | Out-Null
$archives=@{}
foreach($tag in @($BaselineTag,$ReleaseTag)) {
 Write-Host "Downloading portable $tag"
 $archive=Join-Path $root "$tag.zip"
 Invoke-WebRequest "https://github.com/neekin/pastenext/releases/download/$tag/PasteNext-Windows-x64-portable.zip" -OutFile $archive -TimeoutSec 120
 $archives[$tag]=$archive
}
$reports=@()
function Probe([string]$directory,[string]$name) {
 Write-Host "Probing $name"
 $executable=Join-Path $directory 'PasteNext.exe'
 $stdout=Join-Path $ReportDirectory "$name-stdout.log"
 $stderr=Join-Path $ReportDirectory "$name-stderr.log"
 $process=Start-Process $executable -WorkingDirectory $directory -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
 try {
  $windows=@()
  for($attempt=0;$attempt -lt 80;$attempt++) {
   Start-Sleep -Milliseconds 250
   $process.Refresh()
   if($process.HasExited){break}
   $windows=@([StartupProbe]::Windows([uint32]$process.Id))
   if($windows.Count -gt 0 -and $attempt -ge 12){break}
  }
  $entry=@{name=$name;exited=$process.HasExited;pid=$process.Id;beforeShortcut=$windows}
  if($process.HasExited){$entry.exitCode=$process.ExitCode}
  else {
   [StartupProbe]::PanelShortcut()
   Start-Sleep -Seconds 2
   $entry.afterShortcut=@([StartupProbe]::Windows([uint32]$process.Id))
   $entry.children=@(Get-CimInstance Win32_Process | Where-Object {$_.ParentProcessId -eq $process.Id} | Select-Object Name,ProcessId)
  }
  $entry.stderr=Get-Content $stderr -Raw -ErrorAction SilentlyContinue
  $entry | ConvertTo-Json -Depth 8 -Compress | Write-Host
  return $entry
 } finally {
  if(-not $process.HasExited){Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue}
  Start-Sleep -Milliseconds 500
 }
}
$baseline=Join-Path $root 'baseline'
Expand-Archive $archives[$BaselineTag] $baseline
$reports+=Probe $baseline 'baseline-clean'
$clean=Join-Path $root 'portable 测试目录'
Expand-Archive $archives[$ReleaseTag] $clean
$reports+=Probe $clean 'release-clean'
Expand-Archive $archives[$ReleaseTag] $baseline -Force
$reports+=Probe $baseline 'release-upgrade'
$reports | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $ReportDirectory 'portable-startup-report.json') -Encoding utf8
foreach($entry in $reports) {
 if($entry.exited -or -not ($entry.afterShortcut | Where-Object {$_ -match 'visible=True.*title=PasteNext'})) {
  throw "Portable startup failed: $($entry.name)"
 }
}
