# PasteNext

[简体中文](./README.md) | English

A local-first, open-source clipboard manager for macOS and Windows, built with **Tauri 2 + React + Rust**. One codebase, two platforms.

> Clipboard data is stored only in a local SQLite database and never uploaded anywhere. On macOS it runs as a menu bar app (no Dock icon).

## v1.0.29: Desktop pins and sticky notes

macOS pins now support editing and replacing the original image, proportional corner resizing, continuous opacity controls, and adjacent layer ordering. Text history can be sent to system or built-in sticky notes; built-in notes save locally and return after restart. Windows system notes open with the text copied for pasting, with built-in notes as a fallback when unavailable. See the [v1.0.29 release notes](docs/releases/v1.0.29.md).

## Capture studio (v1.0.28)

Screenshots, annotations, OCR, long-image stitching, pinned images, recording and local phone transfers now have native capture entry points on macOS and Windows. Switch region, window and display modes directly in the capture toolbar or with **1 / 2 / 3**, with a 60% outside dim mask.

v1.0.28 uses **Command+Shift+A / R / G** on macOS and **Ctrl+Alt+A / R / G** on Windows for capture, recording start/stop and long screenshots. The Windows panel remains **Ctrl+Alt+V**. Old capture defaults migrate once; unrelated custom shortcuts are preserved. The native selector is initialized ahead of time to reduce the wait after pressing a shortcut.

Before building on Windows, run `python scripts/setup-windows-media.py` to prepare the pinned local media runtime. Both installed and portable packages include it. See [platform parity](docs/screenshot/platform-parity.md). Capture startup and shortcut updates are included in the [v1.0.28](https://github.com/neekin/pastenext/releases/tag/v1.0.28) stable release.

## Features

- **Unlimited clipboard history** — captures plain text, rich text (HTML), images and files in the background, with de-duplication (re-copying an item moves it to the top)
- **Card panel** — summoned by a global shortcut, it slides up from the bottom of the screen with a horizontal card flow and live previews (text summary / image thumbnail / file list)
- **Search & filters** — full-text search (matches notes and tags too) plus filtering by content type
- **Boards** — organize frequently used snippets into boards: create, rename, delete and move items
- **Edit / notes / tags** — edit clip content, attach notes and tags so useful fragments are easy to keep
- **Auto-paste** — selecting an item writes it back to the clipboard and synthesizes `Cmd/Ctrl+V` into the target app
- **Offline image OCR** — macOS Vision and Windows WinRT recognize text for copying and search. Windows recognition languages depend on installed system OCR language packs.
- **Sensitive app exclusions** — copies made inside password managers and other apps you list (matched by app name / bundle ID) are never recorded
- **Multiple languages** — Simplified Chinese and English, switchable from Settings (the tray menu follows)
- **System integration** — menu bar / system tray, launch at login, system-following dark mode, custom global shortcut, retention limits

## Shortcuts

| Action | Shortcut |
|---|---|
| Show / hide panel | `Cmd+Shift+V` (macOS) / `Ctrl+Alt+V` (Windows), configurable |
| Select item | `←` `→` (or `↑` `↓`) |
| Select an item | Click the card |
| Paste selected item | `Enter` or double-click the card |
| Close panel | `Esc` / click another window |

> **First run on macOS**: auto-paste requires the **Accessibility** permission (System Settings → Privacy & Security → Accessibility). The panel shows a shortcut to it; without the permission you can still copy and press `Cmd+V` manually.
>
> **“App is damaged” after downloading?** The build is not Apple-notarized. Run `xattr -cr /Applications/PasteNext.app` in Terminal (see the [download page](https://neekin.github.io/pastenext/) for details).

## Development

Requirements: Node 20+, pnpm, Rust (stable), Xcode CLT on macOS, MSVC Build Tools and WebView2 on Windows.

```bash
pnpm install
pnpm tauri dev      # dev mode with hot reload
pnpm app            # build .app and replace the one in /Applications (daily testing)
pnpm tauri build    # full build (DMG/MSI installers) for distribution
```

> On macOS use `pnpm app` for iteration: it builds the `.app` only, signs with a persistent local certificate, verifies it, replaces the copy in `/Applications` and relaunches. Use `pnpm app "/path/to/PasteNext.app"` to install an existing official build without recompiling its license keys. Switching to this identity requires granting Accessibility once again; subsequent updates reuse the certificate and app identifier to preserve code identity.
>
> Securely back up the entire `~/.local/share/pastenext/signing` directory, which holds a dedicated keychain and an owner-only password file. Do not delete, regenerate, or commit these credentials. This is local signing, not Apple Developer ID signing or notarization. Continue using this installer: replacing the app through another installation method also replaces its signature.

Output locations:

- macOS: `src-tauri/target/release/bundle/dmg/PasteNext_*.dmg`
- Windows: `src-tauri/target/release/bundle/{msi,nsis}/*`

### Project layout

```
src/                    # React frontend (panel + settings share one bundle, routed by entry html)
├── i18n/               # locale dictionaries (zh-CN / en) and the i18n provider
├── panel/              # card-flow panel, cards, edit drawer (content/notes/tags)
├── settings/           # settings window
└── api.ts              # Tauri invoke wrapper
src-tauri/src/
├── monitor.rs          # clipboard polling (400ms) → dedupe → SQLite → event broadcast
├── platform/           # platform layer
│   ├── macos.rs        # NSPasteboard / NSWorkspace / CGEvent paste synthesis
│   └── win32.rs        # Win32 clipboard / SendInput / foreground process
├── db.rs               # SQLite (clips/boards/tags/clip_tags/settings)
├── commands.rs         # all Tauri commands
├── i18n.rs             # Rust-side strings (tray menu)
└── tray.rs             # tray menu
```

Content fingerprints: SHA-256 over text content, decoded RGBA pixels for images, and the path set for files — so a "copy → paste → capture again" round trip never creates duplicate entries.

## CI builds for Windows / macOS

Pushing a `v*` tag triggers GitHub Actions (`.github/workflows/build.yml`) to build these installers and publish a Release after all builds succeed. Manual workflow runs only build Artifacts.

- macOS Apple Silicon (`aarch64-apple-darwin` .dmg)
- macOS Intel (`x86_64-apple-darwin` .dmg)
- Windows x64 (`x86_64-pc-windows-msvc` .msi / .exe)

Download them from the Artifacts section of the corresponding run (PasteNext-macos-*/PasteNext-windows-*).

Keep the package, Tauri and Cargo versions in sync and run `python3 scripts/check-release-version.py v1.0.29` before tagging. Release builds require the repository's `PASTENEXT_SIGN_SECRET` / `PASTENEXT_MAIL_SECRET`. macOS also requires encrypted `PASTENEXT_MACOS_CERTIFICATE` / `PASTENEXT_MACOS_CERTIFICATE_PASSWORD` secrets to reuse the persistent identity. The workflow verifies signatures inside each DMG and publishes `SHA256SUMS`.

## Roadmap

- [ ] Cross-device sync (WebDAV / cloud drives)
- [ ] Rich-text WYSIWYG preview and rendering
- [x] Offline image OCR (macOS Vision / Windows WinRT)

## Documents

- [Privacy Policy](./PRIVACY.md) — what we collect: nothing
- [Terms of Use / EULA](./TERMS.md)

## License

[MIT](./LICENSE) © 2026 Nee Kin
