import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/kernel-release-download.py'
spec = importlib.util.spec_from_file_location('wsx_release_download_test', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class Opener:
    def __init__(self, body):
        self.body = body
        self.url = None

    def open(self, request, timeout):
        self.url = request.full_url
        return io.BytesIO(self.body)


class DownloadTests(unittest.TestCase):
    def test_download_checks_bytes_and_never_replaces_existing_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            body = b'kernel asset'
            sha = hashlib.sha256(body).hexdigest()
            item = {'asset': 'modules-' + sha + '.tar.gz', 'sha256': sha, 'bytes': len(body)}
            opener = Opener(body)
            module.fetch_asset(item, root / 'asset', opener)
            self.assertEqual((root / 'asset').read_bytes(), body)
            self.assertTrue(opener.url.startswith(module.BASE))
            with self.assertRaisesRegex(ValueError, 'already exists'):
                module.fetch_asset(item, root / 'asset', opener)
            with self.assertRaisesRegex(ValueError, 'digest mismatch'):
                module.fetch_asset(item, root / 'bad', Opener(b'x' * len(body)))
            self.assertEqual({p.name for p in root.iterdir()}, {'asset'})

    def test_redirect_rejects_untrusted_host(self):
        handler = module.ReleaseRedirect()
        with self.assertRaisesRegex(ValueError, 'Unsafe'):
            handler.redirect_request(None, None, 302, '', {}, 'https://example.com/file')


if __name__ == '__main__':
    unittest.main()
