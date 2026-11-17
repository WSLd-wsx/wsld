import importlib.util
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/kernel-package.py'
spec = importlib.util.spec_from_file_location('kernel_package', SCRIPT)
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def create(root, tag, source='a' * 40, generation='e' * 64, schema=1, build_source=None):
    root.mkdir()
    files = {}
    staging = root / 'staging'
    staging.mkdir()
    for name in package.MODULES:
        file = staging / name
        file.write_bytes(name.encode())
        files[name] = {'bytes': file.stat().st_size, 'sha256': package.digest(file)}
    ident = package.group_id(files)
    group = root / 'groups' / ident
    group.parent.mkdir()
    staging.rename(group)
    (root / 'audits').mkdir()
    audit = root / 'audits' / 'temporary'
    audit.write_bytes(b'{"evidence":"test"}')
    audit_sha = package.digest(audit)
    audit.rename(root / 'audits' / (audit_sha + '.json'))
    index = {'schema_version': 1, 'generation': generation, 'groups': {ident: files}, 'targets': {tag: {
        'source_commit': source, 'group': ident, 'kernel_build_id': 'b' * 40,
        'kernel_image_sha256': 'c' * 64, 'audit_sha256': audit_sha}}}
    if schema in (2, 3):
        (root / 'builds').mkdir()
        build = root / 'builds' / 'temporary'
        build.write_text(json.dumps({'schema_version': 1, 'build_generation': generation,
                                     'kernel_source_commit': build_source or source, 'modules': files}))
        build_sha = package.digest(build)
        build.rename(root / 'builds' / (build_sha + '.json'))
        index['schema_version'] = schema
        index['targets'][tag]['build_sha256'] = build_sha
        if schema == 3:
            index['targets'][tag]['build_source_commit'] = build_source or source
    (root / 'index.json').write_text(json.dumps(index))
    return index


class PackageTests(unittest.TestCase):
    def test_build_input_manifest_binds_runner_dependencies(self):
        dependencies = {'schema_version': 1, 'packages': {'pahole': '1.0'}}
        encoded = json.dumps(dependencies, sort_keys=True, separators=(',', ':')).encode()
        inputs = {'source': 'a' * 64, '@runner-dependencies': hashlib.sha256(encoded).hexdigest()}
        manifest = json.dumps({'schema_version': 1, 'inputs': inputs}, sort_keys=True, separators=(',', ':')).encode()
        generation = hashlib.sha256(manifest).hexdigest()
        build = {'build_inputs': inputs, 'runner_dependencies': dependencies}
        package.verify_build_inputs(build, generation)
        dependencies['packages']['pahole'] = '2.0'
        with self.assertRaisesRegex(ValueError, 'dependency evidence'):
            package.verify_build_inputs(build, generation)
        inputs['source'] = 'b' * 64
        with self.assertRaisesRegex(ValueError, 'manifest'):
            package.verify_build_inputs(build, generation)

    def test_v3_separates_target_source_from_reused_module_build_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = create(root / 'base', 'linux-msft-wsl-6.18.58.1', source='a' * 40, schema=3)
            second = create(root / 'update', 'linux-msft-wsl-6.18.54.1', source='b' * 40, schema=3, build_source='a' * 40)
            merged = package.merge(root / 'base', root / 'update', root / 'merged')
            self.assertEqual(len(merged['groups']), 1)
            entry = merged['targets']['linux-msft-wsl-6.18.54.1']
            self.assertEqual(entry['source_commit'], 'b' * 40)
            self.assertEqual(entry['build_source_commit'], 'a' * 40)
            self.assertEqual(first['targets']['linux-msft-wsl-6.18.58.1']['build_sha256'], second['targets']['linux-msft-wsl-6.18.54.1']['build_sha256'])

    def test_v2_preserves_build_evidence_and_rejects_tampering(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tag = 'linux-msft-wsl-6.18.58.1'
            index = create(root / 'base', tag, schema=2)
            package.archive(root / 'base', root / 'snapshot.tar.gz')
            self.assertEqual(package.extract(root / 'snapshot.tar.gz', root / 'restored'), index)
            receipt = root / 'restored' / 'builds' / (index['targets'][tag]['build_sha256'] + '.json')
            receipt.write_text('tampered')
            with self.assertRaisesRegex(ValueError, 'Build evidence'):
                package.validate(root / 'restored')

    def test_mixing_evidence_formats_requires_migration(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            create(root / 'base', 'linux-msft-wsl-6.18.58.1')
            create(root / 'update', 'linux-msft-wsl-6.18.54.1', schema=2)
            with self.assertRaisesRegex(ValueError, 'formats differ'):
                package.merge(root / 'base', root / 'update', root / 'merged')

    def test_merge_reuses_identical_group_and_roundtrips(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            create(root / 'base', 'linux-msft-wsl-6.18.58.1')
            create(root / 'update', 'linux-msft-wsl-6.18.54.1')
            result = package.merge(root / 'base', root / 'update', root / 'merged')
            self.assertEqual(len(result['groups']), 1)
            self.assertEqual(len(result['targets']), 2)
            first = package.archive(root / 'merged', root / 'one.tar.gz')
            second = package.archive(root / 'merged', root / 'two.tar.gz')
            self.assertEqual(first, second)
            self.assertEqual(package.extract(root / 'one.tar.gz', root / 'restored'), result)

    def test_conflicting_target_cannot_be_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            create(root / 'base', 'linux-msft-wsl-6.18.58.1')
            create(root / 'update', 'linux-msft-wsl-6.18.58.1', source='d' * 40)
            with self.assertRaisesRegex(ValueError, 'conflict'):
                package.merge(root / 'base', root / 'update', root / 'merged')

    def test_new_generation_never_reuses_old_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            create(root / 'base', 'linux-msft-wsl-6.18.58.1')
            create(root / 'update', 'linux-msft-wsl-6.18.54.1', generation='f' * 64)
            with self.assertRaisesRegex(ValueError, 'rebuild all targets'):
                package.merge(root / 'base', root / 'update', root / 'merged')

    def test_tampered_module_and_archive_traversal_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            index = create(root / 'base', 'linux-msft-wsl-6.18.58.1')
            ident = next(iter(index['groups']))
            (root / 'base' / 'groups' / ident / 'virtio-gpu.ko').write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError, 'mismatch'):
                package.validate(root / 'base')
            with tarfile.open(root / 'unsafe.tar.gz', 'w:gz') as tar:
                entry = tarfile.TarInfo('../outside')
                entry.size = 1
                tar.addfile(entry, io.BytesIO(b'x'))
            with self.assertRaisesRegex(ValueError, 'Unsafe'):
                package.extract(root / 'unsafe.tar.gz', root / 'restored')
            self.assertFalse((root / 'outside').exists())


if __name__ == '__main__':
    unittest.main()
