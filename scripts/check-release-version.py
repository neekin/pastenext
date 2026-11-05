#!/usr/bin/env python3
"""Reject mismatched application versions and release tags before packaging."""
import json
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parents[1]
versions = {
    "package.json": json.loads((root / "package.json").read_text(encoding="utf-8"))["version"],
    "tauri.conf.json": json.loads((root / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))["version"],
    "Cargo.toml": re.search(r'^version\s*=\s*"([^"]+)"', (root / "src-tauri/Cargo.toml").read_text(encoding="utf-8"), re.M)[1],
    "Cargo.lock": re.search(r'name = "paste-next"\s+version = "([^"]+)"', (root / "src-tauri/Cargo.lock").read_text(encoding="utf-8"))[1],
}
if len(set(versions.values())) != 1:
    raise SystemExit(f"Version mismatch: {versions}")
version = versions["package.json"]
ref = sys.argv[1] if len(sys.argv) > 1 else ""
if ref.startswith("v") and ref != "v" + version:
    raise SystemExit(f"Tag {ref} does not match application version {version}")
if not (root / "docs/releases" / f"v{version}.md").is_file():
    raise SystemExit(f"Release notes are missing for {version}")
print(f"Verified release version {version}")
