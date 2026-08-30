# PasteNext

[English](./README.en.md) | 简体中文

本地优先的开源跨平台剪贴板管理器,基于 **Tauri 2 + React + Rust** 构建,一个代码库同时产出 **macOS** 和 **Windows** 版本。

> 剪贴板数据只存储在本机 SQLite 数据库中,不上传任何服务器。macOS 上为菜单栏应用(不占 Dock 图标)。

## 功能

- **无限剪贴板历史** — 后台自动捕获复制的纯文本 / 富文本(HTML)/ 图片 / 文件,内容去重(同内容重新复制会置顶)
- **卡片式面板** — 全局快捷键唤起,屏幕底部滑出,横向卡片流预览(文本摘要 / 图片缩略图 / 文件列表)
- **搜索与筛选** — 全文搜索(同时命中笔记和标签)+ 按内容类型筛选
- **看板收藏** — 把常用片段整理进看板,支持创建 / 重命名 / 删除 / 移动条目
- **编辑 / 笔记 / 标签** — 直接编辑剪贴内容、为条目添加备注和标签,便于沉淀常用片段
- **自动粘贴** — 选中条目即写回剪贴板并合成 `Cmd/Ctrl+V` 粘贴到目标应用
- **敏感应用排除规则** — 密码管理器等应用中复制的内容不记录(按应用名 / Bundle ID 模糊匹配)
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
| 唤起 / 隐藏面板 | `Cmd+Shift+V`(macOS)/ `Ctrl+Shift+V`(Windows),可在设置中修改 |
| 选择条目 | `←` `→`(或 `↑` `↓`) |
| 粘贴选中条目 | `Enter`(或直接点击卡片) |
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

> macOS 日常迭代用 `pnpm app`:只构建 .app(跳过 DMG)、自动替换 /Applications 里的版本并重启,无需手动拖 DMG。注意二进制变化后辅助功能授权需要重新授予一次。

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

推送到 `main` 或打 `v*` tag 后,GitHub Actions(见 `.github/workflows/build.yml`)会构建:

- macOS Apple Silicon(`aarch64-apple-darwin` .dmg)
- macOS Intel(`x86_64-apple-darwin` .dmg)
- Windows x64(`x86_64-pc-windows-msvc` .msi / .exe)

在 Actions 页面对应 run 的 Artifacts 里下载(PasteNext-macos-*/PasteNext-windows-*)。

## Roadmap

- [ ] 跨设备同步(WebDAV / 网盘)
- [ ] 本地 MCP 服务器,向 Claude / Cursor 等 AI 工具暴露剪贴板历史
- [ ] 富文本所见即所得预览与渲染
- [ ] OCR 图片文字提取

## 文档

- [隐私政策](./PRIVACY.md) — 我们收集什么:什么都不收集
- [使用条款 / EULA](./TERMS.md)
- [English README](./README.en.md)

## License

[MIT](./LICENSE) © 2026 Nee Kin
