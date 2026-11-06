# Capture platform parity

Current work: original native macOS and Windows selectors; shared editor, library,
OCR entry points, image exports, pinning, long-image stitching and LAN transfers.

- v1.0.28 defaults: Command+Shift+A / R / G on macOS; Ctrl+Alt+A / R / G on
  Windows, alongside Ctrl+Alt+V for the clipboard panel. Existing capture defaults
  migrate once. Custom bindings and later choices of the old combinations persist.
  Earlier v1.0.27 installations upgrade from the previous Windows Ctrl+Shift defaults.
- Selection startup: prelaunch one idle native helper, initialize UI libraries,
  and send a fresh request over its stdin pipe when capture begins. Screen content
  is read only after that request. Closed/crashed workers fall back to a new helper;
  the next idle helper is prepared after completion or cancellation.
- The unconditional 140 ms startup sleep is removed. Visible Windows UI uses a
  compositor flush before capture; macOS waits 32 ms only when hiding visible UI.
  Region/display modes skip window enumeration until window selection is requested.
  Idle mouse movement no longer redraws the entire overlay.
- Selection: outside shade 60%, resize / move, floating region / window / display
  toolbar, 1/2/3 and Space/Tab switching, Enter to confirm, Esc/right-click cancel.
  Selection mode is persisted without opening the studio.
- macOS: AppKit native selector and system screenshot / recording APIs.
- Windows: Win32 native selector plus packaged, checksum-pinned FFmpeg LGPL engine.
  Image captures use GDI display/window input. Recordings stop via `q` on stdin
  so the MP4 is finalized; discard kills and removes temporary recordings.
  Child stderr is drained while recording to prevent full-pipe deadlocks.
- Windows record / export H.264 is capped at 3840×2160 to stay within the packaged
  OpenH264 encoder limits. Small videos keep their original dimensions. GIFs use
  10 FPS and at most 120 seconds, with a 960 / 640 pixel maximum edge.
- Runtime preparation: `python scripts/setup-windows-media.py`, then `pnpm build`
  and `pnpm tauri build`. Both installed and portable releases include the native
  selector, runtime and its license / source notices.
- These builds do not record audio. HEIC import and public/cloud transfers are not
  implemented. Protected system / media surfaces are subject to OS restrictions.

## Validation

macOS: native AppKit tests cover negative / above-primary display coordinates,
mode buttons, keyboard switching, moving/resizing, rendered shade and confirmation.
28 Rust tests and frontend build/tests pass locally. Native optimized preview is
rebuilt with the existing fixed signing identity.

Windows: `capture-check` workflow builds on windows-latest, runs Rust tests including
real GDI capture, graceful recording stop and MP4/MOV/GIF output verification,
drives Win32 selection messages, and builds the NSIS client. Run
[42096845051](https://github.com/neekin/pastenext/actions/runs/42096845051)
passed all 29 Rust tests and the installed-client smoke test: real OS screenshot
and recording shortcuts, 180×120 region capture, MP4 stop/save, embedded 1024×768
video playback, offline OCR of the fixture text, LAN download/upload and revocation.
A separate Preview identifier and executable name keep test installations from
replacing the stable application or using its data directory. The downloadable
Preview installer is rebuilt without the test-only WebView debugging port.

Release: GitHub Actions builds macOS Apple Silicon / Intel and Windows x64
installers from version tags. macOS packages reuse the existing fixed certificate;
both DMGs are mounted and their embedded app signatures verified before upload.
The release contains Windows EXE, MSI and portable ZIP packages plus `SHA256SUMS`.

Website: existing GitHub Pages Actions deploys `site/` from main. Capture features
are included in v1.0.27 and later stable downloads on macOS and Windows.
