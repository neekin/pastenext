# PasteNext

[English](./README.en.md) | 简体中文

本地优先的开源跨平台剪贴板管理器,基于 **Tauri 2 + React + Rust** 构建,一个代码库同时产出 **macOS** 和 **Windows** 版本。

> 剪贴板数据只存储在本机 SQLite 数据库中,不上传任何服务器。macOS 上为菜单栏应用(不占 Dock 图标)。

## v1.0.30：隐藏贴图恢复

macOS 有隐藏贴图时，剪贴板面板顶部直接显示「恢复贴图（数量）」；托盘菜单立即更新并显示隐藏数量。一键恢复只处理隐藏的图片，保留其他贴图的鼠标穿透设置。详见 [v1.0.30 更新说明](docs/releases/v1.0.30.md)。

## v1.0.29：桌面贴图与便签

macOS 贴图支持右键重新编辑并替换原图、右下角等比缩放、连续透明度与上下层级；贴图窗口只显示图片。历史文字可发送到便签，设置中可选择系统便签或内置便签，内置便签自动保存并在重启后恢复。Windows 系统便签模式会打开便笺并复制文字供粘贴；系统便签不可用时自动使用内置便签。详见 [v1.0.29 更新说明](docs/releases/v1.0.29.md)。

## 新版截图工作台（v1.0.28）

集成截图、标注、OCR、长图拼接、贴图、录屏与局域网扫码传图，macOS 与 Windows 均有原生捕获入口。截图时直接用浮动栏或 **1 / 2 / 3** 切换区域、窗口、全屏，选区外暗化 60%。

v1.0.28 默认快捷键：macOS **⌘⇧A / R / G**，Windows **Ctrl+Alt+A / R / G**，分别对应截图、录屏启停与长截图；Windows 剪贴板面板仍为 **Ctrl+Alt+V**。升级时自动迁移旧截图默认组合，其他自定义组合保留。截图选区程序会提前初始化，减少按下快捷键后的等待。

Windows 构建前运行 `python scripts/setup-windows-media.py` 准备固定版本的本地媒体引擎；安装版与便携版都会包含运行文件。详见 [平台对齐说明](docs/screenshot/platform-parity.md)。截图唤醒与快捷键更新包含在 [v1.0.28](https://github.com/neekin/pastenext/releases/tag/v1.0.28) 正式版中。

## 功能

- **无限剪贴板历史** — 后台自动捕获复制的纯文本 / 富文本(HTML)/ 图片 / 文件,内容去重(同内容重新复制会置顶)
- **卡片式面板** — 全局快捷键唤起,屏幕底部滑出,横向卡片流预览(文本摘要 / 图片缩略图 / 文件列表)
- **搜索与筛选** — 全文搜索(同时命中笔记和标签)+ 按内容类型筛选
- **看板收藏** — 把常用片段整理进看板,支持创建 / 重命名 / 删除 / 移动条目
- **编辑 / 笔记 / 标签** — 直接编辑剪贴内容、为条目添加备注和标签,便于沉淀常用片段
- **自动粘贴** — 选中条目即写回剪贴板并合成 `Cmd/Ctrl+V` 粘贴到目标应用
- **敏感应用排除规则** — 密码管理器等应用中复制的内容不记录(按应用名 / Bundle ID 模糊匹配)
- **OCR 图片文字提取** — macOS 使用系统 Vision，Windows 使用系统 WinRT OCR，离线识别图片文字；结果可复制及全文搜索。Windows 的可识别语言取决于已安装的系统 OCR 语言包。
- **多语言界面** — 内置简体中文 / English,设置中可随时切换(托盘菜单同步跟随)
- **系统集成** — 菜单栏 / 系统托盘、开机自启、深色模式跟随系统、自定义全局快捷键、历史保留上限

## 多语言

应用界面支持 **简体中文** 与 **English**:

- 首次启动按系统语言自动选择;
- 设置 →「语言」可随时切换,切换后立即生效并同步到托盘菜单;
- 文案集中在 `src/i18n/`(前端)与 `src-tauri/src/i18n.rs`(Rust 侧)。

## 快捷键

| 操作 | 快捷键 |
|---|---|
| 唤起 / 隐藏面板 | `Cmd+Shift+V`(macOS)/ `Ctrl+Alt+V`(Windows),可在设置中修改 |
| 选择条目 | `←` `→`(或 `↑` `↓`) |
| 选中条目 | 单击卡片 |
| 粘贴选中条目 | `Enter` 或双击卡片 |
| 关闭面板 | `Esc` / 点击其他窗口 |

> **macOS 首次使用**:自动粘贴需要授予「辅助功能」权限(系统设置 → 隐私与安全性 → 辅助功能),面板内会显示引导入口;未授权时仍可复制后手动 `Cmd+V`。
>
> **从 Release 下载提示「已损坏」?** 应用未做 Apple 公证,终端运行 `xattr -cr /Applications/PasteNext.app` 即可(详见[下载页](https://neekin.github.io/pastenext/)安装说明)。

## 开发

依赖:Node 20+、pnpm、Rust(stable)、macOS 需 Xcode CLT,Windows 需 MSVC Build Tools 与 WebView2。

```bash
pnpm install
pnpm tauri dev      # 开发模式(带热重载)
pnpm app            # 构建 .app 并直接替换 /Applications 里的 PasteNext(日常测试用)
pnpm tauri build    # 完整构建(含 DMG/MSI 安装包,用于分发)
```

> macOS 日常迭代用 `pnpm app`:只构建 .app(跳过 DMG)、使用固定本地证书签名、验证后替换 /Applications 里的版本并重启。安装已下载的正式客户端可执行 `pnpm app "/path/to/PasteNext.app"`,不会重新编译许可密钥。首次切换固定签名需重新授予辅助功能权限,后续更新复用同一证书和应用标识以保持授权身份。
>
> 证书和私钥保存在 `~/.local/share/pastenext/signing` 的独立钥匙串中。请安全备份整个目录,不要删除、重新生成或提交到仓库;签名密码文件仅当前用户可读。此方案用于本机安装,不提供 Apple Developer ID 或公证。直接用其他安装方式覆盖应用会失去此固定签名,应继续使用上述安装命令。

产物位置:

- macOS:`src-tauri/target/release/bundle/dmg/PasteNext_*.dmg`
- Windows:`src-tauri/target/release/bundle/{msi,nsis}/*`

### 代码结构

```
src/                    # React 前端(面板 + 设置窗口共用一个 bundle,按 hash 路由)
├── panel/              # 卡片流面板、卡片、编辑抽屉(内容/笔记/标签)
├── settings/           # 设置窗口
└── api.ts              # Tauri invoke 封装
src-tauri/src/
├── monitor.rs          # 剪贴板轮询监听(400ms)→ 去重 → SQLite → 事件广播
├── platform/           # 平台层
│   ├── macos.rs        # NSPasteboard / NSWorkspace / CGEvent 合成粘贴
│   └── win32.rs        # Win32 剪贴板 / SendInput / 前台进程
├── db.rs               # SQLite(clips/boards/tags/clip_tags/settings)
├── commands.rs         # 全部 Tauri 命令
└── tray.rs             # 托盘菜单
```

数据指纹:文本按内容、图片按解码后 RGBA 像素、文件按路径集合做 SHA-256,保证同一路内容在「复制 → 粘贴 → 再捕获」往返中不会产生重复条目。

## CI 构建 Windows / macOS 安装包

推送 `v*` 标签后，GitHub Actions（见 `.github/workflows/build.yml`）会构建以下安装包，并在全部成功后发布 GitHub Release。手动运行工作流仅构建 Artifacts。

- macOS Apple Silicon(`aarch64-apple-darwin` .dmg)
- macOS Intel(`x86_64-apple-darwin` .dmg)
- Windows x64(`x86_64-pc-windows-msvc` .msi / .exe)

在 Actions 页面对应 run 的 Artifacts 里下载(PasteNext-macos-*/PasteNext-windows-*)。

发布前同步更新 `package.json`、Tauri 配置、Cargo 包与锁文件版本，并运行 `python3 scripts/check-release-version.py v1.0.30`。正式构建使用仓库的 `PASTENEXT_SIGN_SECRET` / `PASTENEXT_MAIL_SECRET`；macOS 另需 `PASTENEXT_MACOS_CERTIFICATE` / `PASTENEXT_MACOS_CERTIFICATE_PASSWORD` 加密 Secrets，以复用固定签名。发布流程会校验 DMG 内的签名并提供 `SHA256SUMS`。

## Roadmap

- [ ] 跨设备同步(WebDAV / 网盘)
- [x] 富文本所见即所得预览(复制的富文本按原样式渲染,只读——本就是剪贴板应用而非编辑器)
- [x] OCR 图片文字提取（macOS Vision / Windows WinRT，离线识别）

## 文档

- [隐私政策](./PRIVACY.md) — 我们收集什么:什么都不收集
- [使用条款 / EULA](./TERMS.md)
- [English README](./README.en.md)

## License

[MIT](./LICENSE) © 2026 Nee Kin

### macOS 桌面贴图与工作台编辑

在图片历史卡片上点击「贴到桌面」，或在截图工作台中点击「贴图」。macOS 两个入口均使用持久化桌面贴图；重复点击同一来源会恢复已有贴图。Windows 保留原截图工作台贴图。

- 窗口只显示图片。拖动图片移动；拖拽右下角等比缩放，也支持滚轮、触控板捏合和 `+` / `-`。开启 macOS 三指拖移后，可用同样方式拖拽缩放角。右键「透明度」可拖动滑块或输入百分比（15–100%），右键菜单也可设置缩放比例和鼠标穿透。
- 多张贴图重叠时，右键「向上一层」「向下一层」在可见贴图间逐层移动，保持桌面置顶。
- 右键「编辑图片…」暂时隐藏原贴图，在截图工作台以原图分辨率继续裁剪和标注；点击「使用图片」替换并显示原贴图，取消编辑或关闭工作台则恢复原图。数字标注等编辑功能统一在工作台使用。旧贴图的数字标记会保留为可撤销、重做的工作台标注。
- 右键「复制图片」或 `⌘C` 按原图分辨率合成图片和标记到剪贴板，不受贴图缩放、显示透明度影响，也不会覆盖原历史图片。
- 按 `Esc` 或右键「隐藏贴图」隐藏贴图。
- 有隐藏贴图时，剪贴板面板顶部会直接显示「恢复贴图（数量）」，可一键恢复隐藏的图片，也可进入「贴图」逐张恢复。托盘「桌面贴图」会显示隐藏数量，菜单状态随贴图操作立即更新。
- 右键「鼠标穿透」让点击落到下方应用。通过面板「贴图」或托盘「桌面贴图 → 解除贴图鼠标穿透」恢复操作。
- 托盘菜单将「截图与录屏」「桌面贴图」分别分组，设置和退出独立显示；仅在需要时显示权限与授权入口。
- 面板和托盘「桌面贴图」子菜单均可隐藏/恢复全部贴图；恢复会解除穿透，并把离屏窗口放回可见屏幕。

贴图、数字标记、位置、缩放、透明度和隐藏状态会自动保存到本机，退出应用后再次启动会恢复。恢复时解除鼠标穿透；原显示器不可用时将贴图移回可见屏幕。「关闭」会移除这张贴图及其保存状态；「隐藏」会保留。贴图拥有独立图片副本，清理原剪贴板历史后仍可恢复。持久化贴图与工作台编辑支持 macOS 图片历史和截图工作台，两个入口合计最多同时 20 张；敏感图片需先取消敏感标记。

文字历史卡片可点击「发送到便签」，图片历史卡片可直接「贴到桌面」。设置中的「便签方式」可选择系统便签或内置便签：macOS 调用系统便笺服务；Windows 打开已安装的 Sticky Notes，并复制文字供粘贴。系统便签不可用时自动创建内置便签。内置便签支持编辑和自动保存，重启后恢复；关闭只隐藏，可通过托盘「便签」重新显示。便签内容独立保存，清理剪贴板历史不会删除便签。

验证：`npm test`、`npm run build`、`cargo test --manifest-path src-tauri/Cargo.toml --locked`。
原生窗口冒烟测试（需要已登录的 macOS 桌面与前端构建产物）：

```bash
cargo run --manifest-path src-tauri/Cargo.toml --example pins_smoke --features tauri/custom-protocol --locked
```

测试依次运行三个独立 macOS 应用进程，验证右下角缩放、连续透明度与数字输入、重叠贴图逐层移动、工作台编辑回填与取消、便签自动保存及重启恢复、关闭贴图后不再恢复。测试使用独立临时数据库，不启动剪贴板监听，不修改系统剪贴板。可用 `PIN_SMOKE_IMAGE=/absolute/path/image.png` 指定测试图片；完成后打印测试证据目录。
