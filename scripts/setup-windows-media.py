#!/usr/bin/env python3
"""Fetch the pinned LGPL Windows media runtime used by packaged releases and CI."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile
import urllib.request
import zipfile

URL = 'https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-11-05-12-48/ffmpeg-n8.1.3-34-ga35157abb9-win64-lgpl-8.1.zip'
SHA256 = 'aeea3020a16e8d37aeac67df9ce3cdbf93b79f5ea96eddd26f4a79ba8154cc86'
ROOT = Path(__file__).resolve().parents[1] / 'src-tauri' / 'ffmpeg'
CACHE = Path(os.environ.get('PASTENEXT_MEDIA_CACHE_DIR', tempfile.gettempdir())) / ('pastenext-ffmpeg-' + SHA256[:16] + '.zip')

def digest(path):
    result = hashlib.sha256()
    with path.open('rb') as file:
        for block in iter(lambda: file.read(1024 * 1024), b''): result.update(block)
    return result.hexdigest()

def main():
    if not CACHE.exists() or digest(CACHE) != SHA256:
        pending = CACHE.with_suffix('.partial')
        with urllib.request.urlopen(URL, timeout=120) as response, pending.open('wb') as file:
            shutil.copyfileobj(response, file)
        if digest(pending) != SHA256:
            pending.unlink(missing_ok=True)
            raise RuntimeError('Media runtime checksum mismatch')
        pending.replace(CACHE)
    ROOT.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(CACHE) as archive:
        members = archive.namelist()
        binary = next(name for name in members if name.endswith('/bin/ffmpeg.exe'))
        with archive.open(binary) as source, (ROOT / 'ffmpeg.exe').open('wb') as dest:
            shutil.copyfileobj(source, dest)
        for name in members:
            # Preserve all shipped notices; do not include unused ffplay/ffprobe binaries.
            base = Path(name).name
            if base.lower().startswith(('license', 'copying', 'readme')) and not name.endswith('/'):
                with archive.open(name) as source, (ROOT / base).open('wb') as dest: shutil.copyfileobj(source, dest)
            if name.endswith('.dll') and '/bin/' in name:
                with archive.open(name) as source, (ROOT / base).open('wb') as dest: shutil.copyfileobj(source, dest)
    (ROOT / 'SOURCE.txt').write_text('FFmpeg LGPL build by BtbN\nBuild: ' + URL + '\nSHA-256: ' + SHA256 + '\nBuild scripts: https://github.com/BtbN/FFmpeg-Builds/tree/autobuild-2026-11-05-12-48\nFFmpeg source: https://github.com/FFmpeg/FFmpeg/tree/a35157abb9\nThis executable is invoked as a separate process. Its original license and notices are included.\n')
    (ROOT / 'runtime.json').write_text(json.dumps({'url': URL, 'sha256': SHA256}) + '\n')
    print('Verified Windows media runtime:', ROOT)

if __name__ == '__main__': main()
