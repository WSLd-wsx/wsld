import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/fetch-target-kernel-package.py'
spec = importlib.util.spec_from_file_location('fetch_target_kernel_package', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class Response(io.BytesIO):
    def __init__(self, data, declared=None):
        super().__init__(data)
        self.headers = {} if declared is None else {'Content-Length': str(declared)}


class FetchTests(unittest.TestCase):
    def test_orphaned_package_is_reused_only_after_matching_download(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'kernel-packages').mkdir()
            orphan = root / 'kernel-packages/6.18.54.1-1.nupkg'
            orphan.write_bytes(b'package')
            def opener(request, timeout):
                if request.full_url.endswith('/index.json'):
                    return Response(b'{"versions":["6.18.54.1-1"]}')
                return Response(b'package', 7)
            result = module.fetch('linux-msft-wsl-6.18.54.1', root, opener)
            self.assertEqual(result['package_sha256'], hashlib.sha256(b'package').hexdigest())
            self.assertEqual(orphan.read_bytes(), b'package')

    def test_mismatched_orphan_and_download_are_retained(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'kernel-packages').mkdir()
            orphan = root / 'kernel-packages/6.18.54.1-1.nupkg'
            orphan.write_bytes(b'old data')
            def opener(request, timeout):
                if request.full_url.endswith('/index.json'):
                    return Response(b'{"versions":["6.18.54.1-1"]}')
                return Response(b'new data', 8)
            with self.assertRaisesRegex(ValueError, 'differs.*partial retained'):
                module.fetch('linux-msft-wsl-6.18.54.1', root, opener)
            self.assertEqual(orphan.read_bytes(), b'old data')
            self.assertEqual(len(list((root / 'kernel-packages').glob('*.partial'))), 1)
            self.assertFalse((root / 'target-inputs/linux-msft-wsl-6.18.54.1.json').exists())

    def test_exact_version_download_and_conflicting_cache_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            calls = []
            def opener(request, timeout):
                calls.append(request.full_url)
                if request.full_url.endswith('/index.json'):
                    return Response(json.dumps({'versions': ['6.18.54.1-1', '6.18.54.1-2', '6.18.58.1-1']}).encode())
                return Response(b'package', 7)
            result = module.fetch('linux-msft-wsl-6.18.54.1', root, opener)
            self.assertEqual(result['package_version'], '6.18.54.1-2')
            self.assertEqual(result['package_sha256'], hashlib.sha256(b'package').hexdigest())
            self.assertFalse(result['publisher_verified'])
            self.assertIn('6.18.54.1-2', calls[-1])
            with self.assertRaisesRegex(ValueError, 'already exists'):
                module.fetch('linux-msft-wsl-6.18.54.1', root, opener)

    def test_explicit_revision_is_never_replaced_by_newer_revision(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            calls = []
            def opener(request, timeout):
                calls.append(request.full_url)
                if request.full_url.endswith('/index.json'):
                    return Response(b'{"versions":["6.18.54.1-1","6.18.54.1-2"]}')
                return Response(b'package', 7)
            result = module.fetch('linux-msft-wsl-6.18.54.1', root, opener, expected_version='6.18.54.1-1')
            self.assertEqual(result['package_version'], '6.18.54.1-1')
            self.assertIn('6.18.54.1-1', calls[-1])
        with tempfile.TemporaryDirectory() as temporary:
            def opener(request, timeout): return Response(b'{"versions":["6.18.54.1-2"]}')
            with self.assertRaisesRegex(ValueError, 'Exact requested'):
                module.fetch('linux-msft-wsl-6.18.54.1', Path(temporary), opener, expected_version='6.18.54.1-1')

    def test_incomplete_download_keeps_partial_and_no_success_record(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def opener(request, timeout):
                if request.full_url.endswith('/index.json'):
                    return Response(b'{"versions":["6.18.54.1-1"]}')
                return Response(b'short', 100)
            with self.assertRaisesRegex(ValueError, 'incomplete'):
                module.fetch('linux-msft-wsl-6.18.54.1', root, opener)
            self.assertFalse((root / 'target-inputs/linux-msft-wsl-6.18.54.1.json').exists())
            self.assertEqual(len(list((root / 'kernel-packages').glob('*.partial'))), 1)


if __name__ == '__main__':
    unittest.main()
