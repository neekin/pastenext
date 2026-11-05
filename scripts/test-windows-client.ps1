$ErrorActionPreference='Stop'
$target=Join-Path $env:RUNNER_TEMP 'pastenext-preview-installed'
$report=Join-Path $env:RUNNER_TEMP 'pastenext-native-report'
$installer=Get-ChildItem 'src-tauri/target/debug/bundle/nsis/*.exe' | Select-Object -First 1
if(-not $installer){throw 'Preview installer not found'}
$install=Start-Process $installer.FullName -ArgumentList @('/S',"/D=$target") -Wait -PassThru
if($install.ExitCode -ne 0){throw "Installer failed: $($install.ExitCode)"}
if(-not(Test-Path "$target/capture_selector.exe")){throw 'Installed selector is missing'}
if(-not(Test-Path "$target/ffmpeg/ffmpeg.exe")){throw 'Installed media runtime is missing'}
$executable=Get-ChildItem "$target/*.exe" | Where-Object {$_.Name -notmatch 'uninstall|capture_selector'} | Select-Object -First 1
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222'
$client=Start-Process $executable.FullName -PassThru
try {
 & node scripts/test-windows-client.mjs $report
 if($LASTEXITCODE -ne 0){throw 'Installed Windows native client smoke test failed'}
} finally {
 Stop-Process -Id $client.Id -Force -ErrorAction SilentlyContinue
 Get-Process capture_selector,ffmpeg -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}
