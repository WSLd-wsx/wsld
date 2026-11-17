import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('source_review_test', ROOT / 'scripts/review-kernel-source-materials.py')
review = importlib.util.module_from_spec(spec)
spec.loader.exec_module(review)


def digest(data): return hashlib.sha256(data).hexdigest()


def tar_bytes(payload):
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode='w:gz') as tar:
        for name, data in payload.items():
            info = tarfile.TarInfo(name); info.size = len(data); tar.addfile(info, io.BytesIO(data))
    return buf.getvalue()


def manifest_tar(files, **extra):
    manifest = {'files': {name: digest(data) for name, data in files.items()}, **extra}
    return tar_bytes({**files, 'manifest.json': json.dumps(manifest).encode()})


class SourceReviewTests(unittest.TestCase):
    def fixture(self, root):
        def asset(name, data):
            (root / name).write_bytes(data)
            return {'asset': name, 'sha256': digest(data), 'bytes': len(data)}
        generation = 'a' * 64; commit = 'b' * 40; tree = 'c' * 40; build_id = 'd' * 40
        recorded = {'kernel-modules/a.c': digest(b'code'), '@runner-dependencies': digest(b'deps')}
        module_record = {'one.ko': {'bytes': 6, 'sha256': digest(b'binary')}}
        module = asset('modules.tar.gz', tar_bytes({'one.ko': b'binary'}))
        sources = asset('sources.tar.gz', manifest_tar({'inputs/kernel-modules/a.c': b'code', 'runner-dependencies.json': b'deps'}))
        inputs = asset('inputs.tar.gz', b'inputs')
        kernel = asset('kernel.tar.gz', b'kernel')
        config = asset('config', b'config')
        build = {'build_generation': generation, 'build_inputs': recorded, 'kernel_source_commit': commit,
                 'modules': module_record, 'kernel_config_sha256': config['sha256']}
        audit = {'crc_match': True, 'kernel_build_id': build_id, 'modules': {'one.ko':
                 {'sha256': digest(b'binary'), 'missing': [], 'mismatches': [], 'has_module_layout': True, 'import_count': 1}}}
        evidence_files = {'modules-build/build-result.json': json.dumps(build).encode(),
            'static-crc-audit.json': json.dumps(audit).encode(), 'kernel-evidence/config': b'config',
            'target-inspection/inspection.json': json.dumps({'kernel_build_id': build_id, 'kernel_image_sha256': 'e'*64, 'upstream_build_metadata': {'git_revision': commit}}).encode(),
            'kernel-evidence/wsx-source-evidence.json': json.dumps({'source_tag_verified': True,
                'source_tree_matches_package_metadata': True, 'source_commit': commit, 'source_tree': tree, 'package_build_revision': commit}).encode(),
            'kernel-source-receipt.json': json.dumps({'sha256': kernel['sha256'], 'source_tree': tree, 'source_commit': commit, 'package_build_revision': commit, 'package_build_tree': tree}).encode()}
        evidence = asset('evidence.tar.gz', manifest_tar(evidence_files, modules=module_record, build_generation=generation))
        metadata = {'kernel_version': '6.18.58.1-1', 'archive': module['asset'], 'archive_sha256': module['sha256'],
                    'archive_bytes': module['bytes'], 'build_generation': generation, 'kernel_build_id': build_id, 'kernel_image_sha256': 'e'*64}
        metadata_path = root / 'metadata.json'; metadata_path.write_text(json.dumps(metadata))
        source = {'module_archive': module, 'build_inputs': sources, 'recorded_build_inputs': inputs,
                  'kernel_source_archive': kernel, 'kernel_config': config, 'build_evidence': evidence,
                  'build_generation': generation, 'kernel_build_id': build_id, 'kernel_source_commit': commit,
                  'module_metadata_sha256': review.sha(metadata_path), 'modules': module_record,
                  'kernel_source_tree_evidence': {'source_tree': tree, 'package_build_tree': tree, 'package_build_revision': commit}}
        source_path = root / 'source.json'; source_path.write_text(json.dumps(source))
        return source_path, metadata_path, recorded

    def test_report_binds_exact_assets_and_requires_specific_review(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); source, metadata, recorded = self.fixture(root)
            with patch.object(review.inputs_export, 'verify', return_value={'inputs': recorded}), \
                 patch.object(review.module_sources, 'SCRIPTS', ()), patch.object(review.tree_verify, 'verify'):
                result = review.report(root, source, metadata)
                self.assertTrue(result['source_correspondence_verified'])
                self.assertFalse(result['corresponding_source_complete'])
                attestation = {'schema_version': 1, 'decision': 'complete', 'scope': 'corresponding-source-and-recipient-access',
                    'binding': result['binding'], 'reviewer': 'reviewer', 'review_reference': 'review record'}
                approval = root / 'review.json'; approval.write_text(json.dumps(attestation))
                self.assertTrue(review.report(root, source, metadata, approval)['corresponding_source_complete'])
                attestation['binding']['module_sha256'] = '0' * 64; approval.write_text(json.dumps(attestation))
                with self.assertRaisesRegex(ValueError, 'not valid for these exact assets'):
                    review.report(root, source, metadata, approval)
                (root / 'modules.tar.gz').write_bytes(b'changed')
                with self.assertRaisesRegex(ValueError, 'hash or size mismatch'):
                    review.report(root, source, metadata)


if __name__ == '__main__': unittest.main()
