"""Mock protocol cases; not Windows, Guest or GPU acceptance."""
import importlib.util
import json
from pathlib import Path
import runpy
import subprocess
import unittest
from unittest.mock import MagicMock, patch
import uuid

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('display', ROOT/'core/guest/desktop-display.py')
display = importlib.util.module_from_spec(spec)
spec.loader.exec_module(display)


def output(width=800, height=600, current='1', scale=1):
    return {'outputs': [{'id': 1, 'name': 'Virtual-1', 'connected': True, 'enabled': True,
        'rotation': 1, 'currentModeId': current, 'scale': scale, 'modes': [
            {'id': '1', 'size': {'width': 800, 'height': 600}, 'refreshRate': 60},
            {'id': '2', 'size': {'width': 1600, 'height': 900}, 'refreshRate': 60}]}]}


class Display(unittest.TestCase):
    def test_uses_current_mode_pixels_not_logical_size(self):
        value = output(current='2', scale=1.25)
        value['outputs'][0]['size'] = {'width': 1280, 'height': 720}
        _, monitor, modes = display.kwin_output(value)
        self.assertEqual((monitor['width'], monitor['height'], monitor['scale']), (1600, 900, 1.25))
        self.assertEqual(len(modes), 2)

    def test_ambiguous_or_rotated_outputs_reject(self):
        value = output()
        value['outputs'].append(value['outputs'][0].copy())
        with self.assertRaises(ValueError): display.kwin_output(value)
        value = output(); value['outputs'][0]['rotation'] = 2
        with self.assertRaises(ValueError): display.kwin_output(value)

    def test_atomic_mode_scale_and_acknowledgment(self):
        call = MagicMock(side_effect=[json.dumps(output()), '', json.dumps(output(current='2', scale=1.25))])
        with patch.object(display, 'kwin_context', return_value=(call, 123, 'signature')):
            result = display.execute_kwin({'instance': str(uuid.uuid4()), 'action': 'apply',
                'signature': 'signature', 'fit': {'mode': {'width': 1600, 'height': 900, 'refresh': 60}, 'guest_scale': 1.25}})
        self.assertTrue(result['applied'])
        self.assertEqual(call.call_args_list[1].args[0], ['output.1.mode.2', 'output.1.scale.1.250000000000'])

    def test_replaced_compositor_never_changes_mode(self):
        call = MagicMock(return_value=json.dumps(output()))
        with patch.object(display, 'kwin_context', return_value=(call, 123, 'new')):
            with self.assertRaises(ValueError):
                display.execute_kwin({'instance': str(uuid.uuid4()), 'action': 'apply', 'signature': 'old'})
        self.assertEqual(call.call_count, 1)


class Retirement(unittest.TestCase):
    def run_helper(self, states, retained=False):
        instance = uuid.uuid4()
        path = MagicMock(); path.__truediv__.return_value = path
        path.exists.return_value = retained; path.glob.return_value = []
        run = MagicMock(side_effect=[subprocess.CompletedProcess([], 0, stdout=s) for s in states])
        helper = runpy.run_path(str(ROOT/'integration/graphics-lifetime/plasma-retirement.py'),
            init_globals={'Path': MagicMock(return_value=path), 'subprocess': MagicMock(run=run)})
        return helper['stop_plasma'], instance, run

    def test_omarchy_falls_through_without_plasma_mutation(self):
        helper, instance, run = self.run_helper(['not-found\n'])
        self.assertFalse(helper(instance)); self.assertEqual(run.call_count, 1)

    def test_plasma_guest_owns_ordered_logout(self):
        helper, instance, run = self.run_helper(['loaded\n', '', 'inactive\n'])
        self.assertTrue(helper(instance))
        self.assertEqual(run.call_args_list[1].args[0], ['/usr/bin/wsx-guest', 'stop', '--instance', str(instance)])
        self.assertFalse(any('terminate-session' in c.args[0] for c in run.call_args_list))

    def test_failed_unit_or_retained_seat_cannot_acknowledge_retirement(self):
        for state, retained in [('failed\n', False), ('inactive\n', True)]:
            helper, instance, _ = self.run_helper(['loaded\n', '', state], retained)
            with self.assertRaises(RuntimeError): helper(instance)


if __name__ == '__main__':
    unittest.main()
