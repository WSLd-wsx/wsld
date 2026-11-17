import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/build-kernel-modules.py'
sys.path.insert(0, str(SCRIPT.parent))
spec = importlib.util.spec_from_file_location('build_kernel_modules', SCRIPT)
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


class BuilderTests(unittest.TestCase):
    def test_drm_compatibility_is_selected_by_all_target_source_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            retained, target = root / 'retained', root / 'kernel/drivers/gpu/drm'
            retained.mkdir()
            target.mkdir(parents=True)
            entries = {}
            for name in builder.COMPAT_FILES:
                (retained / name).write_bytes(name.encode())
                (target / name).write_bytes(name.encode())
                entries[name] = builder.digest(retained / name)
            (retained / 'sources.lock.json').write_text(json.dumps({'schema_version': 1,
                'upstream_commit': 'a' * 40, 'files': entries}))
            with patch.object(builder, 'COMPAT_SOURCES', retained):
                self.assertEqual(builder.compatible_drm_sources(root / 'kernel'), entries)
                (target / builder.COMPAT_FILES[0]).write_bytes(b'different')
                self.assertEqual(builder.compatible_drm_sources(root / 'kernel'), {})
                (retained / builder.COMPAT_FILES[0]).write_bytes(b'changed')
                with self.assertRaisesRegex(ValueError, 'digest mismatch'):
                    builder.compatible_drm_sources(root / 'kernel')

    def test_target_drm_sources_uses_actual_checked_out_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / 'drivers/gpu/drm'
            target.mkdir(parents=True)
            for name in builder.COMPAT_FILES:
                (target / name).write_bytes(('target:' + name).encode())
            found = builder.target_drm_sources(root)
            self.assertEqual(found, {name: builder.digest(target / name) for name in builder.COMPAT_FILES})
            (target / builder.COMPAT_FILES[0]).unlink()
            with self.assertRaisesRegex(ValueError, 'missing or unsafe'):
                builder.target_drm_sources(root)

    def test_vblank_helper_tracks_target_source_and_header(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / 'output/graphics-module'
            output.mkdir(parents=True)
            definition = output / 'Kbuild'
            definition.write_text('drm_kms_helper-y := other.o kms/drm_vblank_helper.o\n')
            self.assertTrue(builder.adapt_target_kbuild(root / 'kernel', root / 'output'))
            self.assertNotIn('kms/drm_vblank_helper.o', definition.read_text())
            source = root / 'kernel/drivers/gpu/drm/drm_vblank_helper.c'
            source.parent.mkdir(parents=True); source.write_text('source')
            with self.assertRaisesRegex(ValueError, 'Incomplete'):
                builder.adapt_target_kbuild(root / 'kernel', root / 'output')
            header = root / 'kernel/include/drm/drm_vblank_helper.h'
            header.parent.mkdir(parents=True); header.write_text('header')
            self.assertFalse(builder.adapt_target_kbuild(root / 'kernel', root / 'output'))

    def test_successful_build_records_target_identity_and_modules(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            kernel, sources = root / 'kernel', root / 'sources'
            (kernel / 'include/config').mkdir(parents=True)
            (kernel / 'include/config/kernel.release').write_text('test-release')
            (kernel / '.config').write_bytes(b'config')
            (kernel / 'Module.symvers').write_bytes(b'symvers')
            image, notes = root / 'image', root / 'notes'
            image.write_bytes(b'image')
            notes.write_bytes(b'notes')
            identity = {'release': 'test-release', 'source_commit': 'a' * 40}
            prepared = dict(identity, prepared_config_sha256=builder.digest(kernel / '.config'),
                            kernel_notes_sha256=builder.digest(notes))
            (kernel / 'wsx-target-prepared.json').write_text(json.dumps(prepared))
            files = {}
            for group in builder.MODULES:
                (sources / group).mkdir(parents=True)
                source = sources / group / 'input.c'
                source.write_bytes(group.encode())
                files[group + '/input.c'] = builder.digest(source)
            (sources / 'sources.lock.json').write_text(json.dumps({'schema_version': 1,
                'reference_kernel_commit': builder.COMMIT, 'files': files}))
            def compile_modules(command, **kwargs):
                output = Path(next(arg[2:] for arg in command if arg.startswith('M=')))
                for name in builder.MODULES[output.name]:
                    (output / name).write_bytes(name.encode())
            with patch.object(builder, 'SOURCES', sources), \
                 patch.object(builder, 'target_drm_sources', return_value={}), \
                 patch.object(builder, 'adapt_target_kbuild', return_value=False), \
                 patch.object(builder.kernel_target, 'identity', return_value=identity), \
                 patch.object(builder.kernel_symvers, 'derive', return_value=(b'symvers', {})), \
                 patch.object(builder.subprocess, 'check_output', side_effect=['13.2.0', 'GNU ld 2.41']), \
                 patch.object(builder.subprocess, 'run', side_effect=compile_modules):
                result = builder.build(kernel, root / 'output', 'compiler-', 1, image, notes, True, root / 'inspection')
            self.assertEqual(result['diagnostic_target'], identity)
            self.assertEqual(result['kernel_source_commit'], identity['source_commit'])
            self.assertEqual(set(result['modules']), {name for names in builder.MODULES.values() for name in names})
            self.assertFalse(result['release_ready'])

    def test_source_changed_during_copy_is_rejected_before_make(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            kernel, sources = root / 'kernel', root / 'sources'
            (kernel / 'include/config').mkdir(parents=True)
            (kernel / 'include/config/kernel.release').write_text(builder.RELEASE)
            (kernel / '.config').write_bytes(b'config')
            (kernel / 'Module.symvers').write_bytes(b'symvers')
            (sources / 'graphics-module').mkdir(parents=True)
            (sources / 'owner-module').mkdir()
            locked = sources / 'graphics-module/input.c'
            locked.write_bytes(b'source')
            (sources / 'sources.lock.json').write_text(json.dumps({'schema_version': 1,
                'reference_kernel_commit': builder.COMMIT,
                'files': {'graphics-module/input.c': builder.digest(locked)}}))
            def changed_copy(source, destination):
                Path(destination).write_bytes(b'changed during copy')
            with patch.multiple(builder, SOURCES=sources, CONFIG=builder.digest(kernel / '.config'),
                                SYMVERS=builder.digest(kernel / 'Module.symvers')), \
                 patch.object(builder.subprocess, 'check_output', side_effect=['13.2.0', 'GNU ld 2.41']), \
                 patch.object(builder.subprocess, 'run') as run, \
                 patch.object(builder.shutil, 'copyfile', side_effect=changed_copy):
                with self.assertRaisesRegex(ValueError, 'Copied module source differs'):
                    builder.build(kernel, root / 'output', 'compiler-', 1)
            run.assert_not_called()

    def test_rejects_unlocked_source_before_compiling(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            kernel, sources = root / 'kernel', root / 'sources'
            (kernel / 'include/config').mkdir(parents=True)
            (kernel / 'include/config/kernel.release').write_text(builder.RELEASE)
            (kernel / '.config').write_bytes(b'config')
            (kernel / 'Module.symvers').write_bytes(b'symvers')
            (sources / 'graphics-module').mkdir(parents=True)
            (sources / 'owner-module').mkdir()
            locked = sources / 'graphics-module/input.c'
            locked.write_bytes(b'source')
            (sources / 'owner-module/unlocked.c').write_bytes(b'unlocked')
            (sources / 'sources.lock.json').write_text(json.dumps({'schema_version': 1,
                'reference_kernel_commit': builder.COMMIT,
                'files': {'graphics-module/input.c': builder.digest(locked)}}))
            with patch.multiple(builder, SOURCES=sources, CONFIG=builder.digest(kernel / '.config'),
                                SYMVERS=builder.digest(kernel / 'Module.symvers')):
                with self.assertRaisesRegex(ValueError, 'complete module source tree'):
                    builder.build(kernel, root / 'output', 'compiler-', 1)
            self.assertFalse((root / 'output').exists())

    def test_refuses_unverified_kernel_and_preserves_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            kernel = root / 'kernel'
            (kernel / 'include/config').mkdir(parents=True)
            (kernel / 'include/config/kernel.release').write_text('unrelated-kernel\n')
            output = root / 'output'
            with self.assertRaisesRegex(ValueError, 'kernel release'):
                builder.build(kernel, output, 'compiler-', 1)
            self.assertFalse(output.exists())

    def test_rejects_changed_locked_input_before_compiling(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            kernel, sources = root / 'kernel', root / 'sources'
            (kernel / 'include/config').mkdir(parents=True)
            (kernel / 'include/config/kernel.release').write_text(builder.RELEASE)
            (kernel / '.config').write_bytes(b'config')
            (kernel / 'Module.symvers').write_bytes(b'symvers')
            (sources / 'graphics-module').mkdir(parents=True)
            (sources / 'graphics-module/input.c').write_bytes(b'changed')
            lock = sources / 'sources.lock.json'
            lock.write_text(json.dumps({'schema_version': 1, 'reference_kernel_commit': builder.COMMIT,
                                        'files': {'graphics-module/input.c': '0' * 64}}))
            with patch.multiple(builder, SOURCES=sources, CONFIG=builder.digest(kernel / '.config'),
                                SYMVERS=builder.digest(kernel / 'Module.symvers')):
                with self.assertRaisesRegex(ValueError, 'Source digest mismatch'):
                    builder.build(kernel, root / 'output', 'compiler-', 1)
            self.assertFalse((root / 'output').exists())


if __name__ == '__main__':
    unittest.main()
