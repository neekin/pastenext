#!/usr/bin/env python3
"""Sign a release app with the existing identity and package/verify its DMG."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import plistlib
import secrets
import subprocess
import sys
import tempfile

CERTIFICATE_SHA1 = "8354B309E96DCC10F7AA8E956120F4FBA4988DEF"
CERTIFICATE_NAME = "PasteNext Local Signing 1145230436be"


def run(args, **kwargs):
    # Never echo commands: security's password options contain credentials.
    result = subprocess.run([str(a) for a in args], capture_output=True, **kwargs)
    if result.returncode:
        raise RuntimeError(f"{Path(args[0]).name} failed: {result.stderr.decode(errors='replace').strip()}")
    return result.stdout


def package(target):
    repo = Path(__file__).resolve().parents[1]
    version = json.loads((repo / "src-tauri/tauri.conf.json").read_text())["version"]
    arch = "arm64" if target.startswith("aarch64-") else "x86_64"
    bundle = repo / "src-tauri/target" / target / "release/bundle"
    app = bundle / "macos/PasteNext.app"
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    if info.get("CFBundleIdentifier") != "io.pastenext.app" or info.get("CFBundleShortVersionString") != version:
        raise RuntimeError("Release bundle identifier or version differs from source")
    for executable in [app / "Contents/MacOS" / info["CFBundleExecutable"], *[app / "Contents/Resources" / name for name in ("ocr_helper", "media_export", "capture_selector")]]:
        if run(["lipo", "-archs", executable]).decode().strip() != arch:
            raise RuntimeError(f"Wrong architecture: {executable.name}")

    certificate = os.environ.get("PASTENEXT_MACOS_CERTIFICATE", "")
    password = os.environ.get("PASTENEXT_MACOS_CERTIFICATE_PASSWORD", "")
    if not certificate or not password:
        raise RuntimeError("Fixed macOS signing secrets are missing; refusing an ad-hoc release")

    requirement = f'identifier "io.pastenext.app" and certificate leaf = H"{CERTIFICATE_SHA1}"'
    with tempfile.TemporaryDirectory(prefix="pastenext-release-signing-") as directory:
        root = Path(directory)
        keychain = root / "local-signing.keychain-db"
        p12 = root / "identity.p12"
        p12.write_bytes(base64.b64decode(certificate, validate=True))
        keychain_password = secrets.token_urlsafe(48)
        created = False
        try:
            run(["security", "create-keychain", "-p", keychain_password, keychain])
            created = True
            run(["security", "set-keychain-settings", "-l", "-u", "-t", "600", keychain])
            run(["security", "unlock-keychain", "-p", keychain_password, keychain])
            run(["security", "import", p12, "-f", "pkcs12", "-t", "agg", "-k", keychain, "-P", password, "-T", "/usr/bin/codesign"])
            pem = run(["security", "find-certificate", "-a", "-p", keychain])
            der = run(["openssl", "x509", "-outform", "DER"], input=pem)
            if hashlib.sha1(der).hexdigest().upper() != CERTIFICATE_SHA1:
                raise RuntimeError("CI signing certificate differs from the existing identity")
            (root / "certificate.pem").write_bytes(pem)
            (root / "keychain-password").write_text(keychain_password)
            (root / "identity.json").write_text(json.dumps({"name": CERTIFICATE_NAME, "sha1": CERTIFICATE_SHA1}))
            run(["security", "set-key-partition-list", "-S", "apple-tool:,apple:,codesign:", "-s", "-k", keychain_password, keychain])
            signing_env = dict(os.environ, PASTENEXT_SIGNING_DIR=str(root))
            run([sys.executable, repo / "scripts/sign-macos-local.py", app], env=signing_env)
        finally:
            if created:
                run(["security", "delete-keychain", keychain])

        stage = root / "image"
        stage.mkdir()
        stage.chmod(0o755)
        run(["ditto", app, stage / "PasteNext.app"])
        (stage / "Applications").symlink_to("/Applications")
        dmg_dir = bundle / "dmg"
        dmg_dir.mkdir(parents=True, exist_ok=True)
        suffix = "aarch64" if arch == "arm64" else "x64"
        output = dmg_dir / f"PasteNext_{version}_{suffix}.dmg"
        run(["hdiutil", "create", "-volname", "PasteNext", "-srcfolder", stage, "-ov", "-format", "UDZO", output])
        output.chmod(0o644)
        mount = root / "mounted"
        mount.mkdir()
        run(["hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", mount, output])
        try:
            run(["codesign", "--verify", "--deep", "--strict", "-R", "=" + requirement, mount / "PasteNext.app"])
        finally:
            run(["hdiutil", "detach", mount])
        print(f"Verified {output.name}: {arch}, {version}, fixed certificate {CERTIFICATE_SHA1}")


if __name__ == "__main__":
    os.umask(0o077)
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True, choices=["aarch64-apple-darwin", "x86_64-apple-darwin"])
    try:
        package(parser.parse_args().target)
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
