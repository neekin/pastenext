$ErrorActionPreference='Stop'
$target=Join-Path $env:RUNNER_TEMP 'pastenext-preview-installed'
$report=Join-Path $env:RUNNER_TEMP 'pastenext-native-report'
New-Item -ItemType Directory -Force -Path $report | Out-Null
# Compare the previous release using real OS shortcuts on this same machine.
$baselineInstaller=Join-Path $env:RUNNER_TEMP 'pastenext-baseline-1.0.27.exe'
$baselineTarget=Join-Path $env:RUNNER_TEMP 'pastenext-baseline-installed'
Invoke-WebRequest 'https://github.com/neekin/pastenext/releases/download/v1.0.27/PasteNext-Windows-x64-setup.exe' -OutFile $baselineInstaller
$baselineInstall=Start-Process $baselineInstaller -ArgumentList @('/S',"/D=$baselineTarget") -Wait -PassThru
if($baselineInstall.ExitCode -ne 0){throw 'Baseline installation failed'}
$baselineExe=Get-ChildItem "$baselineTarget/*.exe" | Where-Object {$_.Name -notmatch 'uninstall|capture_selector'} | Select-Object -First 1
Get-Process | Where-Object {try {$_.Path -eq $baselineExe.FullName} catch {$false}} | Stop-Process -Force -ErrorAction SilentlyContinue
$baseline=Start-Process $baselineExe.FullName -PassThru
try {
 Start-Sleep -Seconds 2
 $samples=@()
 for($i=0;$i -lt 3;$i++) {
  $samples+= [double](& powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/native-input-windows.ps1 -Action baseline-latency)
  if($LASTEXITCODE -ne 0){throw 'Baseline capture did not open'}
  & powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/native-input-windows.ps1 -Action cancel
  Start-Sleep -Milliseconds 300
 }
 @{version='1.0.27';selectionReadyMs=$samples} | ConvertTo-Json | Set-Content "$report/baseline-latency.json"
} finally {
 Stop-Process -Id $baseline.Id -Force -ErrorAction SilentlyContinue
 Get-Process capture_selector -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}
$installer=Get-ChildItem 'src-tauri/target/debug/bundle/nsis/*.exe' | Select-Object -First 1
if(-not $installer){throw 'Preview installer not found'}
$install=Start-Process $installer.FullName -ArgumentList @('/S',"/D=$target") -Wait -PassThru
if($install.ExitCode -ne 0){throw "Installer failed: $($install.ExitCode)"}
if(-not(Test-Path "$target/capture_selector.exe")){throw 'Installed selector is missing'}
if(-not(Test-Path "$target/ffmpeg/ffmpeg.exe")){throw 'Installed media runtime is missing'}
$executable=Get-ChildItem "$target/*.exe" | Where-Object {$_.Name -notmatch 'uninstall|capture_selector'} | Select-Object -First 1
# Kill only a copy started by the silent installer, then start with explicit test arguments.
Get-Process | Where-Object {try {$_.Path -eq $executable.FullName} catch {$false}} | Stop-Process -Force -ErrorAction SilentlyContinue
$client=Start-Process $executable.FullName -PassThru -RedirectStandardOutput "$report/client-stdout.log" -RedirectStandardError "$report/client-stderr.log"
Start-Sleep -Seconds 2
$client.Refresh()
if($client.HasExited){Get-Content "$report/client-stderr.log";throw "Installed client exited: $($client.ExitCode)"}
try {
 & node scripts/test-windows-client.mjs $report
 if($LASTEXITCODE -ne 0){throw 'Installed Windows native client smoke test failed'}
} catch {
 Get-Content "$report/native-windows.log" -ErrorAction SilentlyContinue
 Get-Content "$report/client-stderr.log" -ErrorAction SilentlyContinue
 Get-Content "$report/client-stdout.log" -ErrorAction SilentlyContinue
 Get-CimInstance Win32_Process | Where-Object {$_.ParentProcessId -eq $client.Id} | Select-Object Name,ProcessId,CommandLine | Format-List | Out-String | Tee-Object "$report/child-processes.log" | Write-Host
 Get-NetTCPConnection -LocalPort 9222 -ErrorAction SilentlyContinue | Format-List | Out-String | Tee-Object "$report/debug-port.log" | Write-Host
 throw
} finally {
 Stop-Process -Id $client.Id -Force -ErrorAction SilentlyContinue
 Get-Process capture_selector,ffmpeg -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}
