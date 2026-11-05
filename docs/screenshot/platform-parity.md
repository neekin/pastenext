# Capture platform parity

Current work: original native macOS and Windows selectors; shared editor, library,
OCR entry points, image exports, pinning, long-image stitching and LAN transfers.

- Defaults: `CmdOrCtrl+Shift+A`, `CmdOrCtrl+Shift+R`, `CmdOrCtrl+Shift+G`.
  This resolves to Command on macOS and Control on Windows. Known temporary
  development defaults migrate; unrelated user shortcuts remain unchanged.
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
27 Rust tests and frontend build/tests pass locally. Native optimized preview is
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

Website: existing GitHub Pages Actions deploys `site/` from main. New features are
marked development preview because the stable download buttons still point to
v1.0.26, which does not include the new capture studio. Native builds and media
tests now pass on both macOS and Windows.
