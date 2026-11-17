import hashlib
import importlib.util
import io
import json
import contextlib
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from urllib.error import HTTPError
from urllib.request import Request

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/kernel-package-registry.py'
spec = importlib.util.spec_from_file_location('kernel_package_registry', SCRIPT)
registry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(registry)


class Response(io.BytesIO):
    def __init__(self, data, headers=None):
        super().__init__(data)
        self.headers = headers or {}


class Opener:
    def __init__(self, archive, missing=False, tamper=False):
        self.archive, self.missing, self.tamper = archive, missing, tamper

    def open(self, request, timeout):
        if '/token?' in request.full_url:
            return Response(b'{"token":"test-token"}')
        if '/manifests/latest' in request.full_url:
            if self.missing:
                raise HTTPError(request.full_url, 404, 'not found', {}, None)
            layer = {'mediaType': registry.LAYER_TYPE, 'size': len(self.archive),
                     'digest': 'sha256:' + hashlib.sha256(self.archive).hexdigest()}
            manifest = {'schemaVersion': 2, 'mediaType': registry.MANIFEST_TYPE,
                        'artifactType': registry.LAYER_TYPE, 'layers': [layer],
                        'config': {'mediaType': 'application/vnd.oci.empty.v1+json',
                                   'digest': 'sha256:' + hashlib.sha256(b'{}').hexdigest(), 'size': 2}}
            raw = json.dumps(manifest).encode()
            return Response(raw, {'Docker-Content-Digest': 'sha256:' + hashlib.sha256(raw).hexdigest()})
        if '/blobs/' in request.full_url:
            return Response((b'x' + self.archive[1:]) if self.tamper else self.archive)
        raise AssertionError('Unexpected request')


class RegistryTests(unittest.TestCase):
    def test_empty_group_index_is_rejected(self):
        value = {'schema_version': 1, 'generation': 'a' * 64, 'groups': {}, 'targets': {},
                 'evidence': {'archive_sha256': 'b' * 64, 'oci_manifest_sha256': 'c' * 64, 'bytes': 1},
                 'runtime_admitted': False, 'release_ready': False}
        with self.assertRaisesRegex(ValueError, 'Invalid module group index'):
            registry.validate_group_index(value)

    def test_group_publication_cli_routes_to_checked_publisher(self):
        args = ['kernel-package-registry.py', '--archive', 'snapshot', '--publish-groups',
                '--exported', 'exported', '--coverage', 'coverage', '--report', 'report',
                '--expected-prior-index', 'sha256:' + 'a' * 64]
        with patch.object(registry.sys, 'argv', args), \
             patch.object(registry, 'publish_groups', return_value={'latest_updated': False}) as publish, \
             contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(registry.main(), 0)
        self.assertEqual(publish.call_args.args[:4], (Path('snapshot'), Path('exported'), Path('coverage'), Path('report')))
        self.assertEqual(publish.call_args.kwargs['expected_prior'], 'sha256:' + 'a' * 64)

    def test_prior_index_change_or_missing_proof_stops_publication(self):
        with patch.object(registry, 'latest_index_digest', return_value='sha256:' + 'b' * 64):
            with self.assertRaisesRegex(ValueError, 'changed since bootstrap'):
                registry.check_prior_index('sha256:' + 'a' * 64, 'actor', 'token', None)
            with self.assertRaisesRegex(ValueError, 'required'):
                registry.check_prior_index(None, 'actor', 'token', None)

    def test_group_publication_cli_requires_complete_arguments(self):
        args = ['kernel-package-registry.py', '--archive', 'snapshot', '--publish-groups']
        errors = io.StringIO()
        with patch.object(registry.sys, 'argv', args), \
             patch.object(registry, 'publish_groups') as publish, \
             contextlib.redirect_stderr(errors):
            self.assertEqual(registry.main(), 1)
        publish.assert_not_called()
        self.assertIn('publication failed', errors.getvalue())

    def make_archive(self, root):
        snapshot = root / 'snapshot'
        snapshot.mkdir()
        (snapshot / 'index.json').write_text(json.dumps({'schema_version': 1, 'generation': 'a' * 64,
                                                          'groups': {}, 'targets': {}}))
        archive = root / 'source.tar.gz'
        registry.package.archive(snapshot, archive)
        return archive.read_bytes()

    def test_pull_checks_digest_and_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            data = self.make_archive(root)
            result = registry.pull(root / 'download.tar.gz', root / 'restored', opener=Opener(data))
            self.assertEqual(result['generation'], 'a' * 64)
            self.assertEqual(result['targets'], 0)
            self.assertTrue((root / 'restored/index.json').is_file())

    def test_missing_never_becomes_empty_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(ValueError, 'independently verified'):
                registry.pull(root / 'download', root / 'restored', opener=Opener(b'', missing=True))
            self.assertFalse((root / 'restored').exists())

    def test_tampered_layer_is_rejected_before_extract(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            data = self.make_archive(root)
            with self.assertRaisesRegex(ValueError, 'digest mismatch'):
                registry.pull(root / 'download', root / 'restored', opener=Opener(data, tamper=True))
            self.assertTrue((root / 'download').exists())
            self.assertFalse((root / 'restored').exists())

    def test_cross_host_redirect_strips_authorization(self):
        request = Request('https://ghcr.io/v2/example', headers={'Authorization': 'Bearer secret'})
        redirected = registry.SafeRedirect().redirect_request(request, None, 302, 'redirect', {}, 'https://storage.example/file')
        self.assertIsNone(redirected.get_header('Authorization'))

    def test_generated_manifest_has_exact_immutable_digest(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.make_archive(root)
            manifest, config = registry.build_manifest(root / 'source.tar.gz')
            digest = 'sha256:' + hashlib.sha256(manifest).hexdigest()
            self.assertEqual(registry.parse_manifest(manifest, digest)['size'], (root / 'source.tar.gz').stat().st_size)
            self.assertEqual(config, b'{}')
            with self.assertRaisesRegex(ValueError, 'digest mismatch'):
                registry.parse_manifest(manifest, 'sha256:' + '0' * 64)

    def test_failed_release_gate_never_contacts_registry(self):
        class NoNetwork:
            def open(self, *args, **kwargs):
                raise AssertionError('Registry must not be contacted')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(registry.subprocess, 'run', return_value=SimpleNamespace(returncode=1)):
                with self.assertRaisesRegex(ValueError, 'no package was published'):
                    registry.publish_immutable(root / 'missing', root / 'coverage', root / 'report',
                                               'actor', 'secret', opener=NoNetwork())
                with self.assertRaisesRegex(ValueError, 'no group was published'):
                    registry.publish_groups(root / 'snapshot', root / 'exported', root / 'coverage',
                                            root / 'group-report', 'actor', 'secret', opener=NoNetwork())

    def test_upload_rejects_untrusted_registry_location(self):
        class ForeignUpload:
            def open(self, request, timeout):
                response = Response(b'', {'Location': 'https://attacker.example/upload'})
                response.status = 202
                return response
        with self.assertRaisesRegex(ValueError, 'outside the trusted registry'):
            registry.upload_blob(ForeignUpload(), 'secret', 'sha256:' + 'a' * 64, 2, b'{}')


if __name__ == '__main__':
    unittest.main()
