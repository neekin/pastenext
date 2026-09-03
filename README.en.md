# PasteNext

[简体中文](./README.md) | English

A local-first, open-source clipboard manager for macOS and Windows, built with **Tauri 2 + React + Rust**. One codebase, two platforms.

> Clipboard data is stored only in a local SQLite database and never uploaded anywhere. On macOS it runs as a menu bar app (no Dock icon).

## Features

- **Unlimited clipboard history** — captures plain text, rich text (HTML), images and files in the background, with de-duplication (re-copying an item moves it to the top)
- **Card panel** — summoned by a global shortcut, it slides up from the bottom of the screen with a horizontal card flow and live previews (text summary / image thumbnail / file list)
- **Search & filters** — full-text search (matches notes and tags too) plus filtering by content type
- **Boards** — organize frequently used snippets into boards: create, rename, delete and move items
- **Edit / notes / tags** — edit clip content, attach notes and tags so useful fragments are easy to keep
- **Auto-paste** — selecting an item writes it back to the clipboard and synthesizes `Cmd/Ctrl+V` into the target app
- **Sensitive app exclusions** — copies made inside password managers and other apps you list (matched by app name / bundle ID) are never recorded
- **Multiple languages** — Simplified Chinese and English, switchable from Settings (the tray menu follows)
- **System integration** — menu bar / system tray, launch at login, system-following dark mode, custom global shortcut, retention limits

## Shortcuts

| Action | Shortcut |
|---|---|
| Show / hide panel | `Cmd+Shift+V` (macOS) / `Ctrl+Alt+V` (Windows), configurable |
| Select item | `←` `→` (or `↑` `↓`) |
| Paste selected item | `Enter` (or click the card) |
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

> On macOS use `pnpm app` for iteration: it builds the `.app` only (skipping the DMG), replaces the copy in `/Applications` and relaunches. Note that the Accessibility grant must be renewed whenever the binary changes.

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

Pushing to `main` or tagging `v*` triggers GitHub Actions (`.github/workflows/build.yml`):

- macOS Apple Silicon (`aarch64-apple-darwin` .dmg)
- macOS Intel (`x86_64-apple-darwin` .dmg)
- Windows x64 (`x86_64-pc-windows-msvc` .msi / .exe)

Download them from the Artifacts section of the corresponding run (PasteNext-macos-*/PasteNext-windows-*).

## Roadmap

- [ ] Cross-device sync (WebDAV / cloud drives)
- [ ] Rich-text WYSIWYG preview and rendering
- [ ] OCR text extraction from images

## Documents

- [Privacy Policy](./PRIVACY.md) — what we collect: nothing
- [Terms of Use / EULA](./TERMS.md)

## License

[MIT](./LICENSE) © 2026 Nee Kin
