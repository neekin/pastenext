# 隐私政策 / Privacy Policy

**生效日期 / Effective date:** 2026-11-06
**适用产品 / Applies to:** PasteNext（macOS / Windows 桌面应用）

> 如果你更习惯阅读英文，请直接跳到下半部分 [English](#english)。

---

## 中文版

### 1. 一句话总结

PasteNext 不向开发者收集或上传你的剪贴板、截图、录屏。历史和媒体保存在本机。只有你主动开启扫码传图时，选定文件才会通过局域网传给持有本次链接的设备；你也可以从手机上传图片到本机。

### 2. 我们收集什么

**什么都不收集。** PasteNext：

- 没有账号系统，不需要注册或登录；
- 不包含任何分析、遥测、崩溃上报或广告 SDK；
- 不把剪贴板、截图或录屏发送到开发者服务器；
- 在你主动截图、录屏或导入图片时，读取所选屏幕、窗口或文件；
- 在你开启扫码传图后，临时提供局域网 HTTP 服务，供下载选定文件及上传图片；链接最长 10 分钟有效，停止分享、关闭工作台或退出应用后停止该会话。局域网 HTTP 传输不加密，请使用可信网络。

### 3. 数据存储在哪里

你复制到剪贴板的内容（文本、富文本、图片、文件路径）会被保存在本机的 SQLite 数据库中，位置在应用数据目录：

- macOS：`~/Library/Application Support/io.pastenext.app/`
- Windows：`%APPDATA%\io.pastenext.app\`

图片和录屏文件同样保存在该目录的媒体子目录下。Windows 便携版则使用程序旁的 `Data/` 目录。卸载应用并删除该目录即可彻底清除全部数据。

### 4. 需要哪些系统权限

| 平台 | 权限 | 用途 | 是否必须 |
|---|---|---|---|
| macOS | 辅助功能（Accessibility） | 在你选中条目后，合成 `Cmd+V` 把内容自动粘贴到当前应用 | 否，未授权时仍可手动 `Cmd+V` |
| macOS | 屏幕录制 | 截取所选屏幕 / 窗口、录制屏幕 | 仅截图 / 录屏需要 |
| macOS / Windows | 本地网络 / 防火墙放行 | 同一网络内与手机交换选定文件 | 仅扫码传图需要 |
| Windows | 无管理员权限要求 | 普通桌面截图、录屏，以及通过 `SendInput` 模拟粘贴 | — |

你可以随时在系统设置中撤销这些权限，撤销辅助功能后自动粘贴不可用；撤销屏幕录制或本地网络权限后，对应的截图 / 录屏或扫码传图功能不可用。

### 5. 敏感内容保护

剪贴板可能包含密码、令牌、身份证号等敏感信息。为此 PasteNext 提供：

- **排除应用规则**：来自密码管理器等指定应用（按应用名 / Bundle ID 模糊匹配）的复制内容不会被记录；
- **历史保留期限**：可设置为 1 / 3 / 12 个月或无限，到期自动清除；
- **一键清空**：设置中可立即清空全部历史（看板收藏不受影响）。

### 6. 儿童隐私

PasteNext 不面向 13 岁以下儿童，也不会有意收集任何儿童的个人信息。

### 7. 第三方组件

应用基于 Tauri、React、SQLite 等开源组件构建。Windows 媒体处理使用随应用分发的 FFmpeg LGPL 构建，作为本地子进程运行；媒体处理不需要上传文件。完整清单见项目仓库的 `README.md` 与设置页"关于"区域。

### 8. 政策更新

如果未来新增云端同步或崩溃上报等涉及数据传输的功能，我们会**先更新本政策并在应用内征得你的明确同意**，不会静默开启。

### 9. 联系我们

如对隐私有任何疑问，请通过项目仓库 Issue 或邮件联系开发者。

---

<a id="english"></a>

## English

### 1. Short version

PasteNext does not send your clipboard, screenshots or recordings to the developer. History and media are stored locally. When you explicitly start a QR transfer session, the selected file can be downloaded by devices holding that session link on your local network. You may also upload phone images to your desktop.

### 2. What we collect

**Nothing.** PasteNext:

- has no accounts, sign-up, or login;
- contains no analytics, telemetry, crash reporting, or advertising SDKs;
- does not send clipboard or media content to developer servers;
- reads the screen, window or file you select when you start capture, recording or image import;
- temporarily starts a local HTTP server when you enable QR transfers. It exposes only the selected download and accepts image uploads. Sessions expire after 10 minutes and stop when you stop sharing, close the studio or exit the app. Local HTTP transfers are not encrypted; use a trusted network.

### 3. Where your data is stored

Clipboard items (text, rich text, images, file paths) are stored in a local SQLite database inside the app data directory:

- macOS: `~/Library/Application Support/io.pastenext.app/`
- Windows: `%APPDATA%\io.pastenext.app\`

Images and recordings are stored in media subdirectories. Windows portable builds use a `Data/` folder next to the executable. Uninstalling the app and deleting that folder removes all data permanently.

### 4. System permissions

| Platform | Permission | Purpose | Required |
|---|---|---|---|
| macOS | Accessibility | Synthesize `Cmd+V` to auto-paste the selected item into the frontmost app | No — manual `Cmd+V` still works |
| macOS | Screen Recording | Capture selected displays or windows and record the screen | Only for capture / recording |
| macOS / Windows | Local network / firewall access | Transfer selected files to and from a phone on the same network | Only for QR transfers |
| Windows | No administrator access required | Capture the normal desktop, record the screen and simulate paste with `SendInput` | — |

You can revoke these permissions at any time. Revoking Accessibility disables auto-paste. Revoking Screen Recording or local network access disables the corresponding capture / recording or phone transfer features.

### 5. Protecting sensitive content

A clipboard can contain passwords, tokens, or personal identifiers. PasteNext therefore provides:

- **Excluded apps**: clips copied from password managers and other apps you specify (matched by app name / bundle ID) are never saved;
- **Retention limits**: keep history for 1 / 3 / 12 months or forever, with automatic cleanup;
- **Clear history**: wipe all history instantly from Settings (boards are not affected).

### 6. Children's privacy

PasteNext is not directed at children under 13, and we do not knowingly collect personal information from children.

### 7. Third-party components

The app uses open-source components including Tauri, React and SQLite. Windows media processing uses a packaged LGPL build of FFmpeg as a local child process; processing does not require file uploads. See `README.md` and the About section in Settings for the full list.

### 8. Changes to this policy

If we ever add features that transmit data (for example cloud sync or crash reporting), we will **update this policy and ask for your explicit consent in the app first** — never silently.

### 9. Contact

For any privacy question, please open an issue in the project repository or contact the developer by email.
