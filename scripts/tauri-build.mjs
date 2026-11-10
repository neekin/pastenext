import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const require = createRequire(import.meta.url);
const args = process.argv.slice(2);
const result = spawnSync(process.execPath, [require.resolve('@tauri-apps/cli/tauri.js'), ...args], { stdio: 'inherit' });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);

// Validate the packaged release build too: debug or example startup can have
// different WebView2 timing and cannot substitute for launching shipping bytes.
if (process.platform === 'win32' && process.env.CI && args[0] === 'build') {
  const project = fileURLToPath(new URL('../', import.meta.url));
  const targetArg = args.indexOf('--target');
  const target = targetArg >= 0 ? args[targetArg + 1] : '';
  const directory = path.resolve(project, process.env.CARGO_TARGET_DIR || 'src-tauri/target', target, args.includes('--debug') ? 'debug' : 'release');
  const checked = spawnSync('pwsh.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.join(project, 'scripts/check-windows-package.ps1'), '-BinaryDirectory', directory], { stdio: 'inherit' });
  if (checked.error) throw checked.error;
  process.exit(checked.status ?? 1);
}
