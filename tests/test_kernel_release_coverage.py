import importlib.util
import contextlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile
from test_kernel_package import create
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/kernel-release-coverage.py'
spec = importlib.util.spec_from_file_location('kernel_release_coverage', SCRIPT)
coverage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(coverage)


class CoverageTests(unittest.TestCase):
    def test_kernel_image_can_be_rechecked_from_verified_package(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = root / 'image'
            image.write_bytes(b'kernel')
            package = root / 'kernel.nupkg'
            with zipfile.ZipFile(package, 'w') as output:
                output.writestr('build/native/bin/x64/kernel', image.read_bytes())
            record = {'kernel_package': 'kernel.nupkg', 'kernel_package_sha256': coverage.sha(package),
                      'kernel_image_sha256': coverage.sha(image)}
            self.assertEqual(coverage.verified_kernel_image_hash(root, record), record['kernel_image_sha256'])
            record['kernel_package_sha256'] = '0' * 64
            with self.assertRaisesRegex(ValueError, 'package digest mismatch'):
                coverage.verified_kernel_image_hash(root, record)
            record['kernel_package_sha256'] = coverage.sha(package)
            record['kernel_image_sha256'] = '0' * 64
            with self.assertRaisesRegex(ValueError, 'image digest mismatch'):
                coverage.verified_kernel_image_hash(root, record)

    def test_pending_attempt_prevents_reuse_of_stale_diagnostic(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pending = root / 'target.pending'
            with patch.object(coverage, 'diagnostic_built', return_value=True) as check:
                self.assertTrue(coverage.reusable_diagnostic(root, 'tag', 'commit', 'generation', None, pending))
                pending.write_text('unfinished')
                self.assertFalse(coverage.reusable_diagnostic(root, 'tag', 'commit', 'generation', None, pending))
                self.assertEqual(check.call_count, 1)

    def test_snapshot_cache_discards_old_extraction_only_after_new_validation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cache = coverage.DiagnosticSnapshotCache(root)
            try:
                create(root / 'one', 'linux-msft-wsl-6.18.58.1', schema=3)
                create(root / 'two', 'linux-msft-wsl-6.18.54.1', schema=3)
                cache.staging.merge_candidate(root, root / 'one')
                first, _ = cache.read()
                cache.staging.merge_candidate(root, root / 'two')
                second, index = cache.read()
                self.assertFalse(first.exists())
                self.assertTrue(second.is_dir())
                self.assertEqual(len(index['targets']), 2)
                self.assertEqual(len(list((root / 'staging-snapshots').glob('*.tar.gz'))), 2)
            finally:
                cache.close()

    def test_pending_marker_clears_only_after_successful_verified_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            marker = Path(temporary) / 'target.pending'
            for code, covered, diagnostic in ((1, True, True), (0, False, False)):
                marker.write_text('pending')
                coverage.finish_builder_attempt(marker, code, covered, diagnostic)
                self.assertTrue(marker.exists())
            coverage.finish_builder_attempt(marker, 0, False, True)
            self.assertFalse(marker.exists())
            marker.write_text('pending')
            coverage.finish_builder_attempt(marker, 0, True, False)
            self.assertFalse(marker.exists())

    def test_failure_report_contains_bounded_log_and_rejects_traversal(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'build.log').write_text('x' * 70000 + '\nreal failure\n')
            output = io.StringIO()
            with contextlib.redirect_stderr(output):
                coverage.report_builder_failure(root, 'linux-msft-wsl-6.18.54.1', 'build.log')
            self.assertIn('real failure', output.getvalue())
            self.assertIn('65536 log bytes', output.getvalue())
            self.assertLess(len(output.getvalue()), 66000)
            with self.assertRaisesRegex(ValueError, 'Unsafe'):
                coverage.report_builder_failure(root, 'test', '../outside')

    def test_rc_versions_are_discovered_and_never_stable(self):
        refs = ''.join('a' * 40 + '\trefs/tags/linux-msft-wsl-' + name + '\n'
                       for name in ('6.18.58.1', '6.19.0-rc2', '6.19.0-rc10', '6.19.0'))
        tags = coverage.inventory(refs)
        self.assertEqual(list(tags)[:3], ['linux-msft-wsl-6.19.0', 'linux-msft-wsl-6.19.0-rc10',
                                        'linux-msft-wsl-6.19.0-rc2'])
        with self.assertRaisesRegex(ValueError, 'cannot be treated as stable'):
            coverage.published_inventory(tags, [{'tag_name': 'linux-msft-wsl-6.19.0-rc2',
                                                  'draft': False, 'prerelease': False}])

    def test_older_preview_does_not_precede_newer_stable(self):
        refs = ''.join('a' * 40 + '\trefs/tags/linux-msft-wsl-' + name + '\n'
                       for name in ('6.20.0.1', '6.19.0-rc2', '6.18.58.1'))
        tags = coverage.inventory(refs)
        releases = [{'tag_name': 'linux-msft-wsl-6.20.0.1', 'draft': False, 'prerelease': False},
                    {'tag_name': 'linux-msft-wsl-6.18.58.1', 'draft': False, 'prerelease': False}]
        targets, preview = coverage.scan_inventory(tags, releases)
        self.assertEqual(list(targets), ['linux-msft-wsl-6.20.0.1', 'linux-msft-wsl-6.19.0-rc2',
                                         'linux-msft-wsl-6.18.58.1'])
        self.assertEqual(preview[0], 'linux-msft-wsl-6.19.0-rc2')

    def test_builder_failure_is_recorded_without_overwriting_previous_log(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            builder = root / 'builder'
            builder.write_text('#!/bin/sh\necho diagnostic\nexit 7\n')
            builder.chmod(0o700)
            target = root / 'coverage'
            target.mkdir()
            first = coverage.run_builder(builder, 'linux-msft-wsl-6.18.54.1', 'a' * 40, target)
            second = coverage.run_builder(builder, 'linux-msft-wsl-6.18.54.1', 'a' * 40, target)
            self.assertEqual(first[0], 7)
            self.assertNotEqual(first[1], second[1])
            self.assertTrue((target / 'builder-logs/linux-msft-wsl-6.18.54.1.pending').is_file())
            self.assertIn('diagnostic', (target / first[1]).read_text())
            with self.assertRaises(ValueError):
                coverage.run_builder(builder, '../bad', 'a' * 40, target)

    def test_inventory_peels_tags_and_excludes_other_tags(self):
        raw = ('a' * 40 + '\trefs/tags/linux-msft-wsl-6.18.58.1\n' +
               'b' * 40 + '\trefs/tags/linux-msft-wsl-6.18.58.1^{}\n' +
               'c' * 40 + '\trefs/tags/not-a-release\n')
        self.assertEqual(coverage.inventory(raw), {'linux-msft-wsl-6.18.58.1': 'b' * 40})

    def test_inventory_uses_numeric_newest_first(self):
        raw = ('a' * 40 + '\trefs/tags/linux-msft-wsl-6.18.9.1\n' +
               'b' * 40 + '\trefs/tags/linux-msft-wsl-6.18.58.1\n' +
               'c' * 40 + '\trefs/tags/linux-msft-wsl-6.18.10.1\n')
        self.assertEqual(list(coverage.inventory(raw)), ['linux-msft-wsl-6.18.58.1',
                                                        'linux-msft-wsl-6.18.10.1',
                                                        'linux-msft-wsl-6.18.9.1'])

    def test_only_published_stable_releases_are_selected(self):
        tags = {'linux-msft-wsl-6.18.58.1': 'a' * 40, 'linux-msft-wsl-6.18.54.1': 'b' * 40}
        releases = [{'tag_name': 'linux-msft-wsl-6.18.58.1', 'draft': False, 'prerelease': True},
                    {'tag_name': 'linux-msft-wsl-6.18.54.1', 'draft': False, 'prerelease': False}]
        self.assertEqual(coverage.published_inventory(tags, releases), {'linux-msft-wsl-6.18.54.1': 'b' * 40})
        releases.append({'tag_name': 'linux-msft-wsl-6.18.60.1', 'draft': False, 'prerelease': False})
        with self.assertRaisesRegex(ValueError, 'no corresponding source tag'):
            coverage.published_inventory(tags, releases)

    def test_conflicting_release_channels_are_rejected(self):
        tag = 'linux-msft-wsl-6.18.58.1'
        releases = [{'tag_name': tag, 'draft': False, 'prerelease': False},
                    {'tag_name': tag, 'draft': False, 'prerelease': True}]
        with self.assertRaisesRegex(ValueError, 'Conflicting upstream release channels'):
            coverage.scan_inventory({tag: 'a' * 40}, releases)

    def test_latest_kernel_preview_is_separate_from_stable(self):
        tags = {'linux-msft-wsl-6.18.60.1': 'a' * 40, 'linux-msft-wsl-6.18.59.1': 'b' * 40,
                'linux-msft-wsl-6.18.58.1': 'c' * 40}
        releases = [{'tag_name': 'linux-msft-wsl-6.18.60.1', 'draft': False, 'prerelease': True},
                    {'tag_name': 'linux-msft-wsl-6.18.59.1', 'draft': False, 'prerelease': True},
                    {'tag_name': 'linux-msft-wsl-6.18.58.1', 'draft': False, 'prerelease': False}]
        self.assertEqual(coverage.latest_kernel_preview(tags, releases), ('linux-msft-wsl-6.18.60.1', 'a' * 40))
        self.assertEqual(coverage.published_inventory(tags, releases), {'linux-msft-wsl-6.18.58.1': 'c' * 40})

    def test_unpublished_rc_tag_is_discovered_without_claiming_stable_release(self):
        raw = ('a' * 40 + '\trefs/tags/linux-msft-wsl-6.19.0-rc2\n' +
               'b' * 40 + '\trefs/tags/linux-msft-wsl-6.18.58.1\n')
        tags = coverage.inventory(raw)
        releases = [{'tag_name': 'linux-msft-wsl-6.18.58.1', 'draft': False, 'prerelease': False}]
        self.assertEqual(coverage.latest_kernel_preview(tags, releases), ('linux-msft-wsl-6.19.0-rc2', 'a' * 40))
        self.assertEqual(list(coverage.published_inventory(tags, releases)), ['linux-msft-wsl-6.18.58.1'])

    def test_missing_and_changed_targets_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tag = 'linux-msft-wsl-6.18.58.1'
            self.assertFalse(coverage.covered(root, tag, 'a' * 40)[0])
            (root / 'targets').mkdir()
            (root / 'targets' / (tag + '.json')).write_text(json.dumps(
                {'schema_version': 1, 'tag': tag, 'source_commit': 'b' * 40}))
            self.assertFalse(coverage.covered(root, tag, 'a' * 40)[0])

    def test_evidence_cannot_escape_coverage_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaises(ValueError):
                coverage.relative_file(Path(temporary), '../untrusted')

    def test_missing_target_blocks_packaging(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            refs, admission, report = root / 'refs', root / 'admission.json', root / 'report.json'
            refs.write_text('a' * 40 + '\trefs/tags/linux-msft-wsl-6.18.58.1\n')
            admission.write_text(json.dumps({'schema_version': 1, 'updates': []}))
            result = subprocess.run([sys.executable, str(SCRIPT), '--coverage', str(root / 'coverage'),
                                     '--admission', str(admission), '--report', str(report), '--refs-file', str(refs)],
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 1)
            outcome = json.loads(report.read_text())
            self.assertEqual(outcome['missing'], 1)
            self.assertEqual(outcome['targets'][0]['channel'], 'source_tag_only')
            self.assertFalse(outcome['release_gate_passed'])

    def test_existing_report_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            report = root / 'report.json'
            report.write_text('previous evidence')
            result = subprocess.run([sys.executable, str(SCRIPT), '--coverage', str(root / 'coverage'),
                                     '--admission', str(root / 'missing'), '--report', str(report),
                                     '--refs-file', str(root / 'missing-refs')], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn('previous evidence', result.stderr)
            self.assertEqual(report.read_text(), 'previous evidence')

    def test_complete_evidence_rechecks_modules_and_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'targets').mkdir()
            (root / 'pair' / 'guest').mkdir(parents=True)
            image = root / 'kernel'
            image.write_bytes(b'kernel fixture')
            receipt, entries = {}, {}
            for name in coverage.MODULES:
                path = root / 'pair' / 'guest' / name
                path.write_bytes(name.encode())
                digest = coverage.sha(path)
                receipt['guest/' + name] = {'bytes': path.stat().st_size, 'sha256': digest}
                entries[name] = {'sha256': digest, 'mismatches': [], 'unobserved': [],
                                 'imports': 1, 'has_module_layout': True}
            receipt_file = root / 'pair' / 'verified-files.json'
            receipt_file.write_text(json.dumps(receipt))
            build_file = root / 'build.json'
            build = {'schema_version': 1, 'kernel_source_commit': 'a' * 40,
                     'build_generation': None,
                     'modules': {name: receipt['guest/' + name] for name in coverage.MODULES}}
            build_file.write_text(json.dumps(build))
            audit = {'kernel_image_sha256': coverage.sha(image), 'kernel_build_id': 'abc',
                     'symbol_crc_compatibility_admitted': True, 'modules': entries}
            (root / 'audit.json').write_text(json.dumps(audit))
            tag = 'linux-msft-wsl-6.18.58.1'
            record = {'schema_version': 1, 'tag': tag, 'source_commit': 'a' * 40,
                      'audit': 'audit.json', 'kernel_image': 'kernel', 'kernel_image_sha256': coverage.sha(image),
                      'kernel_build_id': 'abc', 'receipt': 'pair/verified-files.json',
                      'receipt_sha256': coverage.sha(receipt_file), 'module_anchor': 'pair/guest/drm_kms_helper.ko',
                      'build': 'build.json', 'build_sha256': coverage.sha(build_file)}
            (root / 'targets' / (tag + '.json')).write_text(json.dumps(record))
            self.assertTrue(coverage.covered(root, tag, 'a' * 40)[0])
            package = root / 'kernel.nupkg'
            with zipfile.ZipFile(package, 'w') as output:
                output.writestr('build/native/bin/x64/kernel', image.read_bytes())
            del record['kernel_image']
            record['kernel_package'] = 'kernel.nupkg'
            record['kernel_package_sha256'] = coverage.sha(package)
            (root / 'targets' / (tag + '.json')).write_text(json.dumps(record))
            self.assertTrue(coverage.covered(root, tag, 'a' * 40)[0])
            package.write_bytes(b'corrupted package')
            self.assertFalse(coverage.covered(root, tag, 'a' * 40)[0])
            with zipfile.ZipFile(package, 'w') as output:
                output.writestr('build/native/bin/x64/kernel', image.read_bytes())
            record['kernel_package_sha256'] = coverage.sha(package)
            (root / 'targets' / (tag + '.json')).write_text(json.dumps(record))
            entries['virtio-gpu.ko']['has_module_layout'] = False
            (root / 'audit.json').write_text(json.dumps(audit))
            self.assertFalse(coverage.covered(root, tag, 'a' * 40)[0])
            entries['virtio-gpu.ko']['has_module_layout'] = True
            (root / 'audit.json').write_text(json.dumps(audit))
            self.assertFalse(coverage.covered(root, tag, 'a' * 40, 'f' * 64)[0])
            record['build_generation'] = 'f' * 64
            build['build_generation'] = 'f' * 64
            build_file.write_text(json.dumps(build))
            record['build_sha256'] = coverage.sha(build_file)
            (root / 'targets' / (tag + '.json')).write_text(json.dumps(record))
            self.assertTrue(coverage.covered(root, tag, 'a' * 40, 'f' * 64)[0])
            record['build_source_commit'] = 'b' * 40
            build['kernel_source_commit'] = 'b' * 40
            build_file.write_text(json.dumps(build))
            record['build_sha256'] = coverage.sha(build_file)
            (root / 'targets' / (tag + '.json')).write_text(json.dumps(record))
            self.assertTrue(coverage.covered(root, tag, 'a' * 40, 'f' * 64)[0])
            record['build_source_commit'] = 'c' * 40
            (root / 'targets' / (tag + '.json')).write_text(json.dumps(record))
            self.assertFalse(coverage.covered(root, tag, 'a' * 40, 'f' * 64)[0])
            record['build_source_commit'] = 'b' * 40
            build['modules']['virtio-gpu.ko']['sha256'] = '0' * 64
            build_file.write_text(json.dumps(build))
            record['build_sha256'] = coverage.sha(build_file)
            (root / 'targets' / (tag + '.json')).write_text(json.dumps(record))
            self.assertFalse(coverage.covered(root, tag, 'a' * 40, 'f' * 64)[0])
            build['modules']['virtio-gpu.ko']['sha256'] = coverage.sha(root / 'pair' / 'guest' / 'virtio-gpu.ko')
            build_file.write_text(json.dumps(build))
            record['build_sha256'] = coverage.sha(build_file)
            (root / 'targets' / (tag + '.json')).write_text(json.dumps(record))
            (root / 'pair' / 'guest' / 'virtio-gpu.ko').write_bytes(b'tampered')
            self.assertFalse(coverage.covered(root, tag, 'a' * 40)[0])


if __name__ == '__main__':
    unittest.main()
