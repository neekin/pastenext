#!/usr/bin/env python3
"""Stage, persistently sign, and replace the local client with rollback on failure."""
from pathlib import Path
import os
import plistlib
import shutil
import signal
import subprocess
import sys
import tempfile
import time

DEST = Path("/Applications/PasteNext.app")


def running_pids():
    rows = subprocess.check_output(["ps", "-axo", "pid=,comm="]).decode().splitlines()
    executable = str(DEST / "Contents/MacOS/paste-next")
    return [int(parts[0]) for row in rows if len(parts := row.strip().split(None, 1)) == 2 and parts[1] == executable]


def stop():
    for pid in running_pids():
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    for _ in range(100):
        if not running_pids():
            return
        time.sleep(.1)
    raise RuntimeError("客户端未退出，安装已停止。")


def install(source):
    if sys.platform != "darwin":
        raise RuntimeError("This installer requires macOS.")
    source = source.resolve()
    info = plistlib.loads((source / "Contents/Info.plist").read_bytes())
    if info.get("CFBundleIdentifier") != "io.pastenext.app":
        raise RuntimeError("请选择正式 PasteNext.app；测试版不能覆盖正式客户端。")
    directory = Path(tempfile.mkdtemp(prefix=".pastenext-install-", dir=DEST.parent))
    staging = directory / "PasteNext.app"
    backup = directory / "previous.app"
    installed = False
    try:
        subprocess.run(["ditto", str(source), str(staging)], check=True)
        subprocess.run([sys.executable, str(Path(__file__).with_name("sign-macos-local.py")), str(staging)], check=True)
        # Finish all validation before stopping the existing client.
        stop()
        if DEST.exists():
            DEST.rename(backup)
        try:
            staging.rename(DEST)
            subprocess.run(["open", str(DEST)], check=True)
            time.sleep(2)
            if not running_pids():
                raise RuntimeError("新客户端启动失败。")
        except Exception:
            stop()
            if DEST.exists():
                DEST.rename(staging)
            if backup.exists():
                backup.rename(DEST)
                subprocess.run(["open", str(DEST)], check=False)
            raise
        installed = True
    finally:
        # If rollback itself fails, preserve the old application for recovery.
        # User history/settings in Application Support are never touched.
        if installed or not backup.exists():
            shutil.rmtree(directory)
        else:
            print(f"旧应用保留在 {backup}", file=sys.stderr)
    print(f"已安装并启动 PasteNext {info.get('CFBundleShortVersionString', '')}；旧应用已替换，历史和设置保留。")
    print("首次切换固定签名后请重新授予辅助功能权限。后续安装继续复用本机证书。")


if __name__ == "__main__":
    try:
        if len(sys.argv) != 2:
            raise RuntimeError("Usage: python3 scripts/install-macos-local.py /path/PasteNext.app")
        install(Path(sys.argv[1]))
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
