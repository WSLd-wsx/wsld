import importlib.util
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/wsl-kernel-feed.py'
spec = importlib.util.spec_from_file_location('wsl_kernel_feed', SCRIPT)
feed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(feed)


class FeedTests(unittest.TestCase):
    def test_only_exact_version_is_selected_and_revisions_are_sorted(self):
        result = feed.candidates({'versions': ['6.18.58.1-1', '6.18.58.1-10',
                                                '6.18.58.2-4', '6.18.58.1-preview']},
                                 'linux-msft-wsl-6.18.58.1')
        self.assertEqual([item['version'] for item in result], ['6.18.58.1-10', '6.18.58.1-1'])
        self.assertTrue(all(item['url'].startswith(feed.BASE + '/') for item in result))

    def test_missing_is_explicit_and_duplicate_is_rejected(self):
        self.assertEqual(feed.candidates({'versions': []}, 'linux-msft-wsl-6.18.58.1'), [])
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            feed.candidates({'versions': ['6.18.58.1-1', '6.18.58.1-1']}, 'linux-msft-wsl-6.18.58.1')
        with self.assertRaises(ValueError):
            feed.candidates({'versions': []}, '../unsafe')


if __name__ == '__main__':
    unittest.main()
