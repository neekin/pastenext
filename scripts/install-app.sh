#!/usr/bin/env bash
# 一键更新本地安装的 PasteNext:
#   1. 只构建 .app bundle(--bundles app,跳过 DMG,更快也不受应用运行影响)
#   2. 退出正在运行的实例
#   3. 直接替换 /Applications/PasteNext.app(不需要打开 DMG 拖拽)
#   4. 重新启动
#
# 用法:pnpm app
set -euo pipefail
cd "$(dirname "$0")/.."

APP_SRC="src-tauri/target/release/bundle/macos/PasteNext.app"
APP_DST="/Applications/PasteNext.app"

echo "==> 构建 .app(跳过 DMG)"
pnpm tauri build --bundles app >/dev/null

echo "==> 退出运行中的 PasteNext"
osascript -e 'tell application "PasteNext" to quit' >/dev/null 2>&1 || true
pkill -f "MacOS/paste-next" 2>/dev/null || true
sleep 1

echo "==> 替换 $APP_DST"
rm -rf "$APP_DST"
ditto "$APP_SRC" "$APP_DST"

echo "==> 重新启动"
open "$APP_DST"

echo ""
echo "✅ 已更新并重启 /Applications/PasteNext.app"
echo "⚠️  二进制变了,自动粘贴的辅助功能授权需要重新授予一次:"
echo "    按 ⌘+Shift+V 唤起面板,系统授权弹窗会自动出现"
