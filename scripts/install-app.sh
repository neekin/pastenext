#!/usr/bin/env bash
# pnpm app                     构建并安装本地客户端
# pnpm app /path/PasteNext.app  安装已有正式构建(不重新编译许可密钥)
# 固定本地签名保存在 ~/.local/share/pastenext/signing，勿删除或提交到仓库。
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "此安装脚本仅支持 macOS" >&2
  exit 1
fi
if [[ $# -gt 1 ]]; then
  echo "用法: pnpm app [已有 .app 路径]" >&2
  exit 1
fi
if [[ $# -eq 0 ]]; then
  echo "==> 构建本地 .app"
  pnpm tauri build --bundles app
  APP_SRC="${CARGO_TARGET_DIR:-src-tauri/target}/release/bundle/macos/PasteNext.app"
else
  APP_SRC="$1"
fi
python3 scripts/install-macos-local.py "$APP_SRC"
