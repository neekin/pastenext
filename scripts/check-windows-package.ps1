param([Parameter(Mandatory=$true)][string]$BinaryDirectory)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$report=Join-Path $env:RUNNER_TEMP 'pastenext-native-report'
New-Item -ItemType Directory -Force -Path $report | Out-Null
$root=Join-Path $env:RUNNER_TEMP ('pastenext-package-'+[guid]::NewGuid().ToString('N'))
$installed=Join-Path $root 'installed'
$portable=Join-Path $root 'portable 测试目录'
$installer=Get-ChildItem (Join-Path $BinaryDirectory 'bundle/nsis/*.exe') | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if(-not $installer){throw 'Windows startup validation requires an NSIS installer'}
$install=Start-Process $installer.FullName -ArgumentList @('/S',"/D=$installed") -Wait -PassThru
if($install.ExitCode -ne 0){throw "Installer failed: $($install.ExitCode)"}
$main=Get-ChildItem "$installed/*.exe" | Where-Object {$_.Name -notmatch 'uninstall|capture_selector'} | Select-Object -First 1
if(-not $main){throw 'Installed application executable is missing'}
if(-not(Test-Path "$installed/capture_selector.exe") -or -not(Test-Path "$installed/ffmpeg/ffmpeg.exe")){throw 'Packaged native resources are missing'}
# Silent installers may launch a copy. Only terminate this isolated installation.
Get-Process | Where-Object {try {$_.Path -eq $main.FullName} catch {$false}} | Stop-Process -Force -ErrorAction SilentlyContinue
# Tauri patches bundle-type metadata while packaging. The installed executable
# and the standalone portable executable can legitimately have different hashes;
# launch each actual artifact rather than treating that metadata as a mismatch.
$original=Get-Item (Join-Path $BinaryDirectory $main.Name)
if($original.VersionInfo.ProductVersion -ne $main.VersionInfo.ProductVersion){throw 'Installed and portable versions do not match'}
New-Item -ItemType Directory -Force -Path $portable | Out-Null
Copy-Item $original.FullName "$portable/PasteNext.exe"
Get-ChildItem "$BinaryDirectory/*.dll" -ErrorAction SilentlyContinue | Copy-Item -Destination $portable
Copy-Item "$installed/capture_selector.exe" $portable
Copy-Item "$installed/ffmpeg" $portable -Recurse
New-Item -ItemType File -Force -Path "$portable/portable.mode" | Out-Null
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
public static class ShippingStartup {
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
  keybd_event(0x11,0,0,UIntPtr.Zero);keybd_event(0x12,0,0,UIntPtr.Zero);Thread.Sleep(60);
  keybd_event(0x56,0,0,UIntPtr.Zero);Thread.Sleep(40);keybd_event(0x56,0,2,UIntPtr.Zero);
  keybd_event(0x12,0,2,UIntPtr.Zero);keybd_event(0x11,0,2,UIntPtr.Zero);
 }
}
'@
$checks=[System.Collections.Generic.List[object]]::new()
function CheckStartup([string]$executable,[string]$name) {
 $stderr=Join-Path $report "$name-stderr.log"
 $stdout=Join-Path $report "$name-stdout.log"
 $client=Start-Process $executable -WorkingDirectory (Split-Path $executable) -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
 try {
  # Optimized WebViews can issue startup IPC during construction. Keep the real
  # packaged process alive long enough to exercise it, with no debug browser flags.
  for($attempt=0;$attempt -lt 40;$attempt++) {
   Start-Sleep -Milliseconds 250
   $client.Refresh()
   if($client.HasExited){throw "Packaged $name exited during startup: $($client.ExitCode)`n$(Get-Content $stderr -Raw)"}
  }
  $windows=@([ShippingStartup]::Windows([uint32]$client.Id))
  if(-not($windows | Where-Object {$_ -match 'class=Tauri Window title=PasteNext$'})){throw "Packaged $name did not create its main window"}
  if($windows | Where-Object {$_ -match 'hung=True.*class=Tauri Window'}){throw "Packaged $name stopped responding during startup"}
  [ShippingStartup]::PanelShortcut()
  $shown=$false
  for($attempt=0;$attempt -lt 150;$attempt++) {
   if([ShippingStartup]::Windows([uint32]$client.Id) | Where-Object {$_ -match '^visible=True.*class=Tauri Window title=PasteNext$'}){$shown=$true;break}
   Start-Sleep -Milliseconds 20
  }
  if(-not $shown) {
   $second=Start-Process $executable -WorkingDirectory (Split-Path $executable) -PassThru
   for($attempt=0;$attempt -lt 150;$attempt++) {
    if([ShippingStartup]::Windows([uint32]$client.Id) | Where-Object {$_ -match '^visible=True.*class=Tauri Window title=PasteNext$'}){$shown=$true;break}
    Start-Sleep -Milliseconds 20
   }
   if(-not $second.HasExited){Stop-Process -Id $second.Id -Force -ErrorAction SilentlyContinue}
  }
  $entry=@{name=$name;alive=$true;panelShown=$shown;windows=$windows;sha256=(Get-FileHash $executable).Hash}
  $checks.Add($entry)
  $entry | ConvertTo-Json -Depth 5 -Compress | Write-Host
  if(-not $shown){throw "Packaged $name did not respond to its hotkey or second launch"}
 } finally {
  $children=@(Get-CimInstance Win32_Process | Where-Object {$_.ParentProcessId -eq $client.Id} | Select-Object -ExpandProperty ProcessId)
  if(-not $client.HasExited){Stop-Process -Id $client.Id -Force -ErrorAction SilentlyContinue}
  $children | ForEach-Object {Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue}
  Start-Sleep -Milliseconds 500
 }
}
try {
 CheckStartup $main.FullName 'packaged-installed'
 CheckStartup "$portable/PasteNext.exe" 'packaged-portable-clean'
 CheckStartup "$portable/PasteNext.exe" 'packaged-portable-restart'
 if(-not(Test-Path "$portable/Data/paste-next.db")){throw 'Portable data directory was not created'}
 Write-Host 'PASS: shipping installer and portable executable survive cold startup and reopen the panel'
} finally {
 $checks | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $report 'packaged-startup-report.json') -Encoding utf8
 $uninstaller=Get-ChildItem "$installed/*uninstall*.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
 if($uninstaller){Start-Process $uninstaller.FullName -ArgumentList '/S' -Wait}
}
