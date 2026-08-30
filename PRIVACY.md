# 隐私政策 / Privacy Policy

**生效日期 / Effective date:** 2026-08-31
**适用产品 / Applies to:** PasteNext（macOS / Windows 桌面应用）

> 如果你更习惯阅读英文，请直接跳到下半部分 [English](#english)。

---

## 中文版

### 1. 一句话总结

PasteNext **不收集、不上传、不共享**你的任何数据。你复制过的所有内容都只保存在你自己的电脑里。

### 2. 我们收集什么

**什么都不收集。** PasteNext：

- 没有账号系统，不需要注册或登录；
- 不包含任何分析、遥测、崩溃上报或广告 SDK；
- 不向任何服务器发送数据（应用内没有任何网络请求）；
- 不读取剪贴板以外的数据。

### 3. 数据存储在哪里

你复制到剪贴板的内容（文本、富文本、图片、文件路径）会被保存在本机的 SQLite 数据库中，位置在应用数据目录：

- macOS：`~/Library/Application Support/io.pastenext.app/`
- Windows：`%APPDATA%\io.pastenext.app\`

图片文件同样保存在该目录下。卸载应用并删除该目录即可彻底清除全部数据。

### 4. 需要哪些系统权限

| 平台 | 权限 | 用途 | 是否必须 |
|---|---|---|---|
| macOS | 辅助功能（Accessibility） | 在你选中条目后，合成 `Cmd+V` 把内容自动粘贴到当前应用 | 否，未授权时仍可手动 `Cmd+V` |
| Windows | 无特殊权限 | 通过 `SendInput` 模拟粘贴 | — |

你可以随时在系统设置中撤销这些权限，撤销后 PasteNext 仅失去"自动粘贴"能力，其余功能不受影响。

### 5. 敏感内容保护

剪贴板可能包含密码、令牌、身份证号等敏感信息。为此 PasteNext 提供：

- **排除应用规则**：来自密码管理器等指定应用（按应用名 / Bundle ID 模糊匹配）的复制内容不会被记录；
- **历史保留期限**：可设置为 1 / 3 / 12 个月或无限，到期自动清除；
- **一键清空**：设置中可立即清空全部历史（看板收藏不受影响）。

### 6. 儿童隐私

PasteNext 不面向 13 岁以下儿童，也不会有意收集任何儿童的个人信息。

### 7. 第三方组件

应用基于 Tauri、React、SQLite 等开源组件构建，这些组件同样不会向第三方发送数据。完整清单见项目仓库的 `README.md` 与设置页"关于"区域。

### 8. 政策更新

如果未来新增云端同步或崩溃上报等涉及数据传输的功能，我们会**先更新本政策并在应用内征得你的明确同意**，不会静默开启。

### 9. 联系我们

如对隐私有任何疑问，请通过项目仓库 Issue 或邮件联系开发者。

---

<a id="english"></a>

## English

### 1. Short version

PasteNext **does not collect, upload, or share any of your data.** Everything you copy stays on your own computer.

### 2. What we collect

**Nothing.** PasteNext:

- has no accounts, sign-up, or login;
- contains no analytics, telemetry, crash reporting, or advertising SDKs;
- sends no data to any server (the app makes no network requests);
- reads nothing other than the system clipboard.

### 3. Where your data is stored

Clipboard items (text, rich text, images, file paths) are stored in a local SQLite database inside the app data directory:

- macOS: `~/Library/Application Support/io.pastenext.app/`
- Windows: `%APPDATA%\io.pastenext.app\`

Images are stored in the same directory. Uninstalling the app and deleting that folder removes all data permanently.

### 4. System permissions

| Platform | Permission | Purpose | Required |
|---|---|---|---|
| macOS | Accessibility | Synthesize `Cmd+V` to auto-paste the selected item into the frontmost app | No — manual `Cmd+V` still works |
| Windows | None | Paste is simulated via `SendInput` | — |

You can revoke these permissions at any time. Doing so only disables auto-paste; every other feature keeps working.

### 5. Protecting sensitive content

A clipboard can contain passwords, tokens, or personal identifiers. PasteNext therefore provides:

- **Excluded apps**: clips copied from password managers and other apps you specify (matched by app name / bundle ID) are never saved;
- **Retention limits**: keep history for 1 / 3 / 12 months or forever, with automatic cleanup;
- **Clear history**: wipe all history instantly from Settings (boards are not affected).

### 6. Children's privacy

PasteNext is not directed at children under 13, and we do not knowingly collect personal information from children.

### 7. Third-party components

The app is built on open-source components such as Tauri, React, and SQLite. None of them transmit data to third parties. See `README.md` and the About section in Settings for the full list.

### 8. Changes to this policy

If we ever add features that transmit data (for example cloud sync or crash reporting), we will **update this policy and ask for your explicit consent in the app first** — never silently.

### 9. Contact

For any privacy question, please open an issue in the project repository or contact the developer by email.
