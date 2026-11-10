import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
if (process.platform === 'win32' && process.env.CI) {
  const result = spawnSync('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', fileURLToPath(new URL('./probe-windows-portable.ps1', import.meta.url))], { stdio: 'inherit' });
  if (result.error) throw result.error;
  process.exitCode = result.status ?? 1;
}
