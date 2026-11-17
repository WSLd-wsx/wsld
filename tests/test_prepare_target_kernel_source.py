import hashlib
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/prepare-target-kernel-source.py'
spec = importlib.util.spec_from_file_location('prepare_target_kernel_source', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class TargetSourceTests(unittest.TestCase):
    def test_verifies_package_revision_tree_and_rejects_different_tree(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); upstream = root / 'upstream'
            subprocess.run(['git', 'init', '-q', str(upstream)], check=True)
            def git(*args):
                return subprocess.check_output(['git', '-C', str(upstream), *args], text=True).strip()
            git('config', 'user.name', 'WSX Test'); git('config', 'user.email', 'test@example.invalid')
            (upstream / 'source.c').write_text('original')
            git('add', 'source.c'); git('commit', '-qm', 'original'); base = git('rev-parse', 'HEAD')
            git('commit', '--allow-empty', '-qm', 'same tree'); tag_commit = git('rev-parse', 'HEAD')
            git('tag', 'linux-msft-wsl-6.18.54.1')
            (upstream / 'source.c').write_text('different'); git('add', 'source.c'); git('commit', '-qm', 'different')
            different = git('rev-parse', 'HEAD')
            config = root/'config'; config.write_bytes(b'config'); config_sha = hashlib.sha256(b'config').hexdigest()
            with patch.object(module, 'UPSTREAM', upstream.as_uri()):
                result = module.prepare('linux-msft-wsl-6.18.54.1', tag_commit, config, config_sha,
                                        root/'cache', root/'good', base)
                self.assertTrue(result['source_tree_matches_package_metadata'])
                self.assertEqual(result['source_tree'], result['package_build_tree'])
                self.assertFalse(result['binary_source_correspondence_verified'])
                with self.assertRaisesRegex(ValueError, 'differs from upstream package'):
                    module.prepare('linux-msft-wsl-6.18.54.1', tag_commit, config, config_sha,
                                   root/'cache', root/'bad', different)

    def test_real_git_cache_reuses_objects_for_distinct_targets(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            upstream = root / 'upstream'
            subprocess.run(['git', 'init', '-q', str(upstream)], check=True)
            def git(*args):
                return subprocess.check_output(['git', '-C', str(upstream), *args], text=True).strip()
            git('config', 'user.name', 'WSX Test')
            git('config', 'user.email', 'test@example.invalid')
            commits = []
            tags = ['linux-msft-wsl-6.18.54.1', 'linux-msft-wsl-6.18.58.1']
            for index, tag in enumerate(tags):
                (upstream / 'source.c').write_text(str(index), encoding='utf-8')
                git('add', 'source.c')
                git('commit', '-qm', tag)
                git('tag', tag)
                commits.append(git('rev-parse', 'HEAD'))
            config = root / 'config'
            config.write_text('CONFIG_TEST=y\n', encoding='utf-8')
            config_hash = hashlib.sha256(config.read_bytes()).hexdigest()
            with patch.object(module, 'UPSTREAM', upstream.as_uri()):
                for index, (tag, commit) in enumerate(zip(tags, commits)):
                    output = root / ('target-' + str(index))
                    result = module.prepare(tag, commit, config, config_hash, root / 'cache', output)
                    self.assertEqual(result['source_commit'], commit)
                    self.assertEqual((output / 'source.c').read_text(), str(index))
                    self.assertEqual((output / '.config').read_bytes(), config.read_bytes())
                    self.assertEqual(subprocess.check_output(
                        ['git', '-C', str(output), 'rev-parse', 'HEAD'], text=True).strip(), commit)
            self.assertTrue((root / 'cache' / 'objects').is_dir())

    def test_resolves_annotated_tag_and_rejects_missing_or_conflicting_refs(self):
        tag = 'linux-msft-wsl-6.18.54.1'
        refs = 'a' * 40 + '\trefs/tags/' + tag + '\n' + 'b' * 40 + '\trefs/tags/' + tag + '^{}\n'
        self.assertEqual(module.resolve_tag(refs, tag), 'b' * 40)
        with self.assertRaisesRegex(ValueError, 'not found'):
            module.resolve_tag('', tag)
        with self.assertRaisesRegex(ValueError, 'Conflicting'):
            module.resolve_tag(refs + 'c' * 40 + '\trefs/tags/' + tag + '\n', tag)

    def test_rejects_changed_config_or_wrong_tag_before_creating_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = root / 'config'
            config.write_bytes(b'config')
            with self.assertRaisesRegex(ValueError, 'configuration'):
                module.prepare('linux-msft-wsl-6.18.54.1', 'a' * 40, config, '0' * 64, root / 'cache', root / 'out')
            sha = hashlib.sha256(config.read_bytes()).hexdigest()
            with patch.object(module.subprocess, 'check_output', return_value='b' * 40 + '\trefs/tags/linux-msft-wsl-6.18.54.1\n'):
                with self.assertRaisesRegex(ValueError, 'tag does not match'):
                    module.prepare('linux-msft-wsl-6.18.54.1', 'a' * 40, config, sha, root / 'cache', root / 'out')
            self.assertFalse((root / 'out').exists())
            self.assertFalse((root / 'cache').exists())


if __name__ == '__main__':
    unittest.main()
