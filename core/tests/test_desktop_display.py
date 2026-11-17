"""Host-side protocol tests; these do not claim a live Guest/GPU test."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('display', Path(__file__).parents[1]/'guest/desktop-display.py')
display = importlib.util.module_from_spec(spec)
spec.loader.exec_module(display)


class DisplayControl(unittest.TestCase):
    def setUp(self):
        self.calls = []
        self.monitor = dict(name='Virtual-1', transform=0, disabled=False,
                            availableModes=['1600x900@60.00Hz', 'bad'], width=1600, height=900, scale=1)
        self.request = dict(instance='test', action='apply', signature='current',
                            fit=dict(mode=dict(width=1600, height=900, refresh=60.0), guest_scale=1))

    def run_request(self):
        def call(args):
            self.calls.append(args)
            return 'ok'
        with patch.object(display, 'context', return_value=(call, 123, 'current')), patch.object(display, 'output', return_value=self.monitor):
            return display.execute(self.request)

    def test_stale_compositor_never_receives_mutation(self):
        self.request['signature'] = 'previous'
        with self.assertRaisesRegex(ValueError, 'changed'):
            self.run_request()
        self.assertEqual(self.calls, [])

    def test_unadvertised_mode_and_nonfinite_scale_are_rejected(self):
        self.request['fit']['mode']['width'] = 1920
        with self.assertRaises(ValueError):
            self.run_request()
        self.request['fit']['mode']['width'] = 1600
        self.request['fit']['guest_scale'] = float('nan')
        with self.assertRaises(ValueError):
            self.run_request()
        self.assertEqual(self.calls, [])

    def test_query_never_mutates_and_only_reports_parsed_modes(self):
        self.request['action'] = 'query'
        actual = self.run_request()
        self.assertEqual(self.calls, [])
        self.assertEqual(actual['modes'], [self.request['fit']['mode']])

    def test_real_output_must_acknowledge_requested_mode(self):
        actual = self.run_request()
        self.assertTrue(actual['applied'])
        self.assertEqual(self.calls[0][0], 'eval')
        self.monitor['width'] = 800
        with patch.object(display.time, 'monotonic', side_effect=[0, 10]):
            with self.assertRaisesRegex(RuntimeError, 'did not acknowledge'):
                self.run_request()


if __name__ == '__main__':
    unittest.main()
