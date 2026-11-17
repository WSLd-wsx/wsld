"""Guest observation contract tests; no real session or WSL lifecycle calls."""
import importlib.util
from pathlib import Path
import subprocess
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('desktop_observe', Path(__file__).parents[1] / 'guest/desktop-observe.py')
observer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(observer)
INSTANCE = '01234567-1234-4321-8765-0123456789ab'


class DesktopObservationTests(unittest.TestCase):
    def test_missing_initialization_does_not_invent_account_or_session(self):
        with patch.object(observer, 'trusted_json', side_effect=FileNotFoundError), patch.object(observer.subprocess, 'run') as run:
            result = observer.observe({})
            self.assertFalse(result['initialized'])
            self.assertEqual(result['session']['state'], 'not_queried')
            run.assert_not_called()

    def test_wrong_instance_never_queries_another_session(self):
        with patch.object(observer, 'trusted_json', side_effect=[{'user': 'alice'}, {'instance_id': '11234567-1234-4321-8765-0123456789ab'}]), patch.object(observer.pwd, 'getpwnam', return_value=SimpleNamespace(pw_name='alice', pw_uid=1234)), patch.object(observer.subprocess, 'run') as run:
            with self.assertRaisesRegex(ValueError, 'another instance'):
                observer.observe({'instance': INSTANCE})
            run.assert_not_called()

    def test_reports_live_unit_properties_and_actual_uid(self):
        response = subprocess.CompletedProcess([], 0, 'LoadState=loaded\nActiveState=inactive\nSubState=dead\nResult=success\nExecMainStatus=0\n', '')
        with patch.object(observer, 'trusted_json', side_effect=[{'user': 'alice', 'timezone': 'America/New_York'}, {'instance_id': INSTANCE}]), patch.object(observer.pwd, 'getpwnam', return_value=SimpleNamespace(pw_name='alice', pw_uid=1234)), patch.object(observer.subprocess, 'run', return_value=response) as run:
            result = observer.observe({'instance': INSTANCE})
            self.assertEqual(result['uid'], 1234)
            self.assertEqual(result['session']['properties']['ActiveState'], 'inactive')
            self.assertIn('wsx-omarchy-012345671234432187650123456789ab.service', run.call_args.args[0])
            self.assertEqual(run.call_args.args[0][1], 'show')

    def test_failed_query_is_not_reported_as_stopped(self):
        with patch.object(observer, 'trusted_json', side_effect=[FileNotFoundError, {'instance_id': INSTANCE}]), patch.object(observer.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1, '', 'failed')):
            with self.assertRaisesRegex(RuntimeError, 'query failed'):
                observer.observe({'instance': INSTANCE})


if __name__ == '__main__':
    unittest.main()
