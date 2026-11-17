import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/community-mpl-sources.py'
spec = importlib.util.spec_from_file_location('mpl_sources', SCRIPT)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


def digest(data):
    return hashlib.sha256(data).hexdigest()


class CommunityMplTests(unittest.TestCase):
    def setup_fixture(self, root, modified=False):
        source = root / 'registry-src'; cache = root / 'registry-cache'
        source.mkdir(); cache.mkdir()
        crate = io.BytesIO()
        with tarfile.open(fileobj=crate, mode='w:gz') as tar:
            for name, content in {'LICENSE': b'MPL-2.0', 'src/lib.rs': b'original'}.items():
                info = tarfile.TarInfo('example-1.2.3/' + name); info.size = len(content)
                tar.addfile(info, io.BytesIO(content))
        data = crate.getvalue(); (cache / 'example-1.2.3.crate').write_bytes(data)
        target = source / 'example-1.2.3'; (target / 'src').mkdir(parents=True)
        (target / 'LICENSE').write_bytes(b'MPL-2.0')
        (target / 'src/lib.rs').write_bytes(b'modified' if modified else b'original')
        (target / '.cargo-ok').write_bytes(b'')
        lock = root / 'Cargo.lock'
        lock.write_text('version = 4\n[[package]]\nname = "example"\nversion = "1.2.3"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "' + digest(data) + '"\n', encoding='utf-8')
        tree = root / 'tree.txt'; tree.write_text('example v1.2.3|MPL-2.0\nexample v1.2.3|MPL-2.0 (*)\n', encoding='utf-8')
        return lock, tree, source, cache

    def package(self, root, notices, mutate=None):
        product = root / 'product.zip'; materials = root / 'materials.zip'
        with zipfile.ZipFile(product, 'w') as p, zipfile.ZipFile(materials, 'w') as m:
            for file in sorted(notices.rglob('*')):
                if not file.is_file(): continue
                name = 'gui-third-party/' + file.relative_to(notices).as_posix()
                data = file.read_bytes()
                p.writestr(name, data)
                m.writestr('notices/' + name, b'tampered' if name == mutate else data)
        return product, materials

    def test_collects_exact_locked_source_and_verifies_both_packages(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); lock, tree, source, cache = self.setup_fixture(root)
            notices = root / 'notices'; result = mod.collect(lock, tree, source, cache, notices/'mpl-sources')
            self.assertEqual(len(result['packages']), 1)
            self.assertEqual(result['packages'][0]['changes'], [])
            product, materials = self.package(root, notices)
            self.assertEqual(mod.verify(lock, tree, product, materials), 1)
            self.assertIn('mpl-sources/crates/example-1.2.3.crate', (notices/'MPL-SOURCES.md').read_text())

    def test_preserves_modified_source_and_validates_changes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); lock, tree, source, cache = self.setup_fixture(root, modified=True)
            notices = root/'notices'; result = mod.collect(lock, tree, source, cache, notices/'mpl-sources')
            self.assertEqual(result['packages'][0]['changes'], ['src/lib.rs'])
            snapshot = notices/'mpl-sources'/result['packages'][0]['actual_source']
            with zipfile.ZipFile(snapshot) as z: self.assertEqual(z.read('src/lib.rs'), b'modified')
            product, materials = self.package(root, notices)
            self.assertEqual(mod.verify(lock, tree, product, materials), 1)

    def test_rejects_archive_not_matching_cargo_lock(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); lock, tree, source, cache = self.setup_fixture(root)
            (cache/'example-1.2.3.crate').write_bytes(b'wrong')
            with self.assertRaisesRegex(ValueError, 'Cargo.lock checksum mismatch'):
                mod.collect(lock, tree, source, cache, root/'notices/mpl-sources')

    def test_rejects_missing_or_mismatched_delivered_material(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); lock, tree, source, cache = self.setup_fixture(root)
            notices=root/'notices'; mod.collect(lock, tree, source, cache, notices/'mpl-sources')
            product, materials = self.package(root, notices, 'gui-third-party/MPL-SOURCES.md')
            with self.assertRaisesRegex(ValueError, 'differs between package'):
                mod.verify(lock, tree, product, materials)

    def test_rejects_unpinned_mpl_patch_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); lock, tree, _, _=self.setup_fixture(root)
            lock.write_text(lock.read_text().replace('registry+https://github.com/rust-lang/crates.io-index', 'git+https://example.test/patch'))
            with self.assertRaisesRegex(ValueError, 'not a uniquely locked crates.io archive'):
                mod.selection(lock.read_bytes(), tree.read_bytes())

    def test_rejects_changed_dependency_graph(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); lock, tree, source, cache=self.setup_fixture(root)
            notices=root/'notices'; mod.collect(lock, tree, source, cache, notices/'mpl-sources')
            product, materials=self.package(root, notices)
            tree.write_text(tree.read_text() + 'other v1.0.0|MIT\n')
            with self.assertRaisesRegex(ValueError, 'does not match lock and actual dependency tree'):
                mod.verify(lock, tree, product, materials)

    def test_rejects_unknown_dependency_license(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); lock, tree, _, _=self.setup_fixture(root)
            tree.write_text(tree.read_text() + 'unidentified v1.0.0|unknown\n')
            with self.assertRaisesRegex(ValueError, 'Dependency license is unknown'):
                mod.selection(lock.read_bytes(), tree.read_bytes())


if __name__ == '__main__': unittest.main()
