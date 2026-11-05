$ErrorActionPreference='Stop'
$target=Join-Path $env:RUNNER_TEMP 'pastenext-preview-installed'
$report=Join-Path $env:RUNNER_TEMP 'pastenext-native-report'
New-Item -ItemType Directory -Force -Path $report | Out-Null
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222'
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
 Get-Content "$report/client-stderr.log" -ErrorAction SilentlyContinue
 Get-Content "$report/client-stdout.log" -ErrorAction SilentlyContinue
 Get-CimInstance Win32_Process | Where-Object {$_.ParentProcessId -eq $client.Id} | Select-Object Name,ProcessId,CommandLine | Format-List | Out-String | Tee-Object "$report/child-processes.log" | Write-Host
 Get-NetTCPConnection -LocalPort 9222 -ErrorAction SilentlyContinue | Format-List | Out-String | Tee-Object "$report/debug-port.log" | Write-Host
 throw
} finally {
 Stop-Process -Id $client.Id -Force -ErrorAction SilentlyContinue
 Get-Process capture_selector,ffmpeg -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}
