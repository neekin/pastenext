#!/usr/bin/env python3
"""Sign local Mac builds with one persistent, private code-signing identity.

This is for local installations, not Developer ID distribution/notarization.
Keep ~/.local/share/pastenext/signing backed up securely: replacing this identity
requires granting macOS permissions again. No credentials belong in the repo.
"""
import hashlib
import json
import os
from pathlib import Path
import plistlib
import secrets
import shutil
import subprocess
import sys
import tempfile

ROOT = Path.home() / ".local/share/pastenext/signing"
KEYCHAIN = ROOT / "local-signing.keychain-db"
PASSWORD = ROOT / "keychain-password"
CERT = ROOT / "certificate.pem"
MANIFEST = ROOT / "identity.json"


def run(args, **kwargs):
    # Never print argv: security requires passwords in some command arguments.
    result = subprocess.run([str(arg) for arg in args], capture_output=True, **kwargs)
    if result.returncode:
        raise RuntimeError(f"{Path(args[0]).name} failed: {result.stderr.decode(errors='replace').strip()}")
    return result.stdout


def openssl():
    return shutil.which("openssl") or "/usr/bin/openssl"


def certificate_hash():
    der = run([openssl(), "x509", "-in", CERT, "-outform", "DER"])
    return hashlib.sha1(der).hexdigest().upper()


def identity():
    if MANIFEST.exists():
        data = json.loads(MANIFEST.read_text())
        if not all(p.exists() for p in (KEYCHAIN, PASSWORD, CERT)):
            raise RuntimeError("Signing files are missing. Restore the signing directory from backup; do not create a replacement identity.")
        if certificate_hash() != data["sha1"]:
            raise RuntimeError("Signing certificate differs from the saved identity.")
        return data
    if ROOT.exists() and any(ROOT.iterdir()):
        raise RuntimeError(f"Incomplete signing setup in {ROOT}; inspect it before retrying. No identity was replaced.")
    ROOT.mkdir(mode=0o700, parents=True, exist_ok=True)
    ROOT.chmod(0o700)
    password = secrets.token_urlsafe(48)
    PASSWORD.write_text(password)
    PASSWORD.chmod(0o600)
    name = "PasteNext Local Signing " + secrets.token_hex(6)
    # A self-signed leaf is sufficient for stable local code identity. It is not
    # installed as a trusted root or added to the user's default keychain list.
    with tempfile.TemporaryDirectory(prefix="pastenext-signing-") as directory:
        temp = Path(directory)
        key, p12 = temp / "private.pem", temp / "identity.p12"
        config = temp / "certificate.cnf"
        config.write_text(f"""[req]
distinguished_name = subject
x509_extensions = signing
prompt = no
[subject]
CN = {name}
[signing]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
subjectKeyIdentifier = hash
""")
        run([openssl(), "req", "-new", "-x509", "-newkey", "rsa:3072", "-nodes",
             "-sha256", "-days", "3650", "-config", config, "-keyout", key, "-out", CERT])
        env = dict(os.environ, PASTENEXT_P12_PASSWORD=password)
        # Legacy encryption is understood by macOS's PKCS#12 importer.
        run([openssl(), "pkcs12", "-export", "-inkey", key, "-in", CERT,
             "-out", p12, "-name", name, "-keypbe", "PBE-SHA1-3DES",
             "-certpbe", "PBE-SHA1-3DES", "-macalg", "sha1",
             "-passout", "env:PASTENEXT_P12_PASSWORD"], env=env)
        run(["security", "create-keychain", "-p", password, KEYCHAIN])
        run(["security", "set-keychain-settings", "-l", "-u", "-t", "300", KEYCHAIN])
        run(["security", "unlock-keychain", "-p", password, KEYCHAIN])
        run(["security", "import", p12, "-k", KEYCHAIN, "-P", password,
             "-T", "/usr/bin/codesign"])
        run(["security", "set-key-partition-list", "-S", "apple-tool:,apple:,codesign:",
             "-s", "-k", password, KEYCHAIN])
    data = {"name": name, "sha1": certificate_hash()}
    MANIFEST.write_text(json.dumps(data, indent=2) + "\n")
    print(f"Created persistent local signing identity in {ROOT}")
    return data


def sign(app):
    app = app.resolve()
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    bundle_id = info["CFBundleIdentifier"]
    if bundle_id not in ("io.pastenext.app", "io.pastenext.preview"):
        raise RuntimeError(f"Refusing to sign unrelated application: {bundle_id}")
    data = identity()
    password = PASSWORD.read_text()
    run(["security", "unlock-keychain", "-p", password, KEYCHAIN])
    try:
        # Pin both the app ID and this exact signing certificate. Never use an
        # identifier-only requirement, which would permit other signers.
        requirement = f'identifier "{bundle_id}" and certificate leaf = H"{data["sha1"]}"'
        run(["codesign", "--force", "--deep", "--sign", data["sha1"],
             "--keychain", KEYCHAIN, "--timestamp=none", "--preserve-metadata=entitlements,flags,runtime",
             "--requirements", "=designated => " + requirement, app])
        run(["codesign", "--verify", "--deep", "--strict", "-R", "=" + requirement, app])
        print(f"Signed and verified {app.name}: {bundle_id}, certificate {data['sha1']}")
    finally:
        run(["security", "lock-keychain", KEYCHAIN])


if __name__ == "__main__":
    os.umask(0o077)
    try:
        if sys.platform != "darwin" or len(sys.argv) != 2:
            raise RuntimeError("Usage (macOS): python3 scripts/sign-macos-local.py /path/to/PasteNext.app")
        sign(Path(sys.argv[1]))
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
