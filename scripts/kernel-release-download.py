#!/usr/bin/env python3
"""Download diagnostic kernel release assets with strict size and digest checks."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
from urllib.parse import quote, urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('kernel_release_download_export', HERE / 'export-kernel-release-assets.py')
assets = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assets)
BASE = 'https://github.com/WSLd-wsx/wsld/releases/download/wsx-kernel-modules/'
HOSTS = {'github.com', 'objects.githubusercontent.com', 'release-assets.githubusercontent.com'}


class ReleaseRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        target = urlsplit(newurl)
        if target.scheme != 'https' or target.hostname not in HOSTS or target.username or target.password:
            raise ValueError('Unsafe kernel release redirect')
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def fetch_index(opener=None):
    opener = opener or build_opener(ReleaseRedirect())
    request = Request(BASE + 'index.json', headers={'User-Agent': 'WSX-kernel-release'})
    with opener.open(request, timeout=120) as response:
        raw = response.read(1024 * 1024 + 1)
    if len(raw) > 1024 * 1024:
        raise ValueError('Kernel release index exceeds limit')
    return assets.validate(json.loads(raw))


def fetch_asset(item, output, opener=None):
    if output.exists() or output.is_symlink():
        raise ValueError('Kernel asset output already exists')
    if (not isinstance(item, dict) or not isinstance(item.get('asset'), str)
            or '/' in item['asset'] or '\\' in item['asset'] or item['asset'] in ('.', '..')
            or not isinstance(item.get('sha256'), str) or not assets.HEX.fullmatch(item['sha256'])
            or not isinstance(item.get('bytes'), int) or isinstance(item['bytes'], bool)
            or not 0 < item['bytes'] <= 1024 * 1024 * 1024):
        raise ValueError('Invalid kernel asset descriptor')
    opener = opener or build_opener(ReleaseRedirect())
    request = Request(BASE + quote(item['asset'], safe=''), headers={'User-Agent': 'WSX-kernel-release'})
    fd, temporary = tempfile.mkstemp(prefix='.' + output.name + '.', suffix='.tmp', dir=output.parent)
    temporary = Path(temporary)
    try:
        import os
        h, count = hashlib.sha256(), 0
        with os.fdopen(fd, 'wb') as target, opener.open(request, timeout=120) as source:
            while True:
                block = source.read(1024 * 1024)
                if not block:
                    break
                count += len(block)
                if count > item['bytes']:
                    raise ValueError('Kernel asset exceeds declared size')
                h.update(block)
                target.write(block)
        if count != item['bytes'] or h.hexdigest() != item['sha256']:
            raise ValueError('Kernel asset size or digest mismatch')
        os.link(temporary, output)
    finally:
        temporary.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--index', action='store_true')
    args = parser.parse_args()
    if not args.index:
        parser.error('Choose --index')
    print(json.dumps(fetch_index(), sort_keys=True))


if __name__ == '__main__':
    main()
