"""Mocked control-boundary checks; no WSL, desktop or device operation."""
import importlib.util
import json
import contextlib
import io
from pathlib import Path
import subprocess
import struct
import runpy
import tempfile
from types import SimpleNamespace
import unittest
import uuid
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('control', ROOT/'core/guest/desktop-control.py')
control = importlib.util.module_from_spec(spec); spec.loader.exec_module(control)


class BoundaryTests(unittest.TestCase):
    def test_pinned_preflight_dispatches_to_exact_session(self):
        instance = str(uuid.uuid4())
        reply = {'protocol': 'wsx-guest', 'major': 1, 'id': 'unused', 'ok': True, 'result': {}}
        class Id:
            hex = 'unused'
        with patch.object(control.uuid, 'uuid4', return_value=Id()), \
             patch.object(control.subprocess, 'run', return_value=SimpleNamespace(stdout=json.dumps(reply))) as process:
            self.assertEqual(control.guest_call('capabilities', {}, instance), {})
        self.assertEqual(process.call_args.args[0], ['/usr/bin/python3', '-I', '/usr/lib/wsx/bootstrap-v2.py',
                                                     'dispatch', 'host-call', '--instance', instance])
        with patch.object(control.subprocess, 'run') as process:
            with self.assertRaises(ValueError): control.guest_call('capabilities', {}, str(uuid.UUID(int=0)))
            process.assert_not_called()

    def test_v2_prepare_uses_fixed_bootstrap_and_exact_session(self):
        instance = str(uuid.uuid4())
        self.assertEqual(control.guest_prepare_command(instance, 2),
                         ['/usr/bin/python3', '-I', '/usr/lib/wsx/bootstrap-v2.py',
                          'dispatch', 'prepare', '--instance', instance])
        self.assertEqual(control.guest_prepare_command(instance, 1), ['/usr/bin/wsx-guest', 'prepare'])
        for invalid in (0, 3, True, '2'):
            with self.assertRaises(ValueError): control.guest_prepare_command(instance, invalid)
        with self.assertRaises(ValueError): control.guest_prepare_command(str(uuid.UUID(int=0)), 2)

    def test_group_screen_checks_identity_before_crc_without_admitting(self):
        calls=[]
        def load(path):
            if path.endswith('module-group.py'):
                return {'verify': lambda directory, digest, reuse: calls.append(('verify', digest)) or {'manifest_sha256':digest}}
            if path.endswith('kernel-exports.py'):
                return {'audit': lambda kernel, directory: calls.append(('audit', kernel)) or {'symbol_crc_compatibility_admitted':True}}
            if path.endswith('module-reuse.py'):
                return {'inspect_loaded': lambda root, directory: calls.append(('loaded', str(directory))) or {}, 'MODULES': [1, 2], 'SYS': None}
            raise AssertionError(path)
        with patch.object(control.os,'geteuid',return_value=0), patch.object(control.runpy,'run_path',side_effect=load):
            result=control.screen_module_group(Path('/isolated'),'kernel','a'*64)
        self.assertEqual(calls,[('verify','a'*64),('audit','kernel'),('loaded','/isolated/module-group')])
        self.assertFalse(result['runtime_admitted'])

    def test_group_screen_rejects_partial_loaded_set(self):
        def load(path):
            if path.endswith('module-group.py'):
                return {'verify': lambda *args: {'manifest_sha256': 'a'*64}}
            if path.endswith('kernel-exports.py'):
                return {'audit': lambda *args: {'symbol_crc_compatibility_admitted': True}}
            return {'inspect_loaded': lambda *args: {'one': 'id'}, 'MODULES': [1, 2], 'SYS': None}
        with patch.object(control.os, 'geteuid', return_value=0), patch.object(control.runpy, 'run_path', side_effect=load):
            with self.assertRaisesRegex(RuntimeError, 'partially loaded'):
                control.screen_module_group(Path('/isolated'), 'kernel', 'a'*64)

    def test_group_screen_verifies_loaded_set_and_propagates_mismatch(self):
        calls = []
        def loaded_check(group, sys):
            calls.append((group['manifest_sha256'], sys))
            raise RuntimeError('Loaded module identity mismatch')
        def load(path):
            if path.endswith('module-group.py'):
                return {'verify': lambda *args: {'manifest_sha256': 'a'*64}, 'verify_loaded': loaded_check}
            if path.endswith('kernel-exports.py'):
                return {'audit': lambda *args: {'symbol_crc_compatibility_admitted': True}}
            return {'inspect_loaded': lambda *args: {'one': 'id', 'two': 'id'}, 'MODULES': [1, 2], 'SYS': '/sys'}
        with patch.object(control.os, 'geteuid', return_value=0), patch.object(control.runpy, 'run_path', side_effect=load):
            with self.assertRaisesRegex(RuntimeError, 'identity mismatch'):
                control.screen_module_group(Path('/isolated'), 'kernel', 'a'*64)
        self.assertEqual(calls, [('a'*64, '/sys')])

    def test_group_screen_stops_before_crc_on_invalid_identity(self):
        with patch.object(control.os,'geteuid',return_value=0), \
             patch.object(control.runpy,'run_path',return_value={'verify':lambda *args: (_ for _ in ()).throw(ValueError('bad group'))}) as load:
            with self.assertRaisesRegex(ValueError,'bad group'):
                control.screen_module_group(Path('/isolated'),'kernel','a'*64)
        self.assertEqual(load.call_count,1)

    def test_kernel_identity_reads_live_notes_without_admitting_modules(self):
        with tempfile.TemporaryDirectory() as temporary:
            notes=Path(temporary)/'notes'
            notes.write_bytes(struct.pack('<III',4,4,3)+b'GNU\0'+bytes.fromhex('12345678'))
            result=control.observed_kernel_identity(ROOT/'core/guest',notes)
            self.assertEqual(result['kernel_build_id'],'12345678')
            self.assertFalse(result['runtime_admitted'])
            notes.write_bytes(b'')
            with self.assertRaisesRegex(ValueError,'Invalid running kernel notes'):
                control.observed_kernel_identity(ROOT/'core/guest',notes)

    def test_kernel_identity_action_runs_as_a_script(self):
        original=Path.open
        notes=struct.pack('<III',4,4,3)+b'GNU\0'+bytes.fromhex('12345678')
        def open_path(path,*args,**kwargs):
            if path==Path('/sys/kernel/notes'):
                return io.BytesIO(notes)
            return original(path,*args,**kwargs)
        output=io.StringIO()
        with patch('sys.stdin',io.StringIO('{"action":"kernel-identity"}')), \
             patch.object(Path,'open',open_path),contextlib.redirect_stdout(output):
            runpy.run_path(str(ROOT/'core/guest/desktop-control.py'),run_name='__main__')
        self.assertEqual(json.loads(output.getvalue())['kernel_build_id'],'12345678')

    def test_preflight_forwards_explicit_module_manifest_digest(self):
        contract={'guest_protocol_minor':0,'required_capabilities':['desktop.preflight'],
                  'guest_profile_sha256':'p','sharing_path':'opaque-v1'}
        plan={'source':'release-declaration','contract':contract}
        checks=[]
        def load(path):
            if path.endswith('module-reuse.py'):
                return {'check':lambda root,previous,**kwargs: checks.append(kwargs) or {'reload_required':False}}
            if path.endswith('kernel-exports.py'):
                return {'audit':lambda kernel,guest:{'symbol_crc_compatibility_admitted':True}}
            raise AssertionError(path)
        with patch.object(control.os,'geteuid',return_value=0),patch.object(control.runpy,'run_path',side_effect=load), \
             patch.object(control,'protocol_preflight',return_value={'sharing_path':'opaque-v1','account':{'uid':1000}}), \
             patch.object(control,'screen_module_group',return_value={'kernel':{'symbol_crc_compatibility_admitted':True}}) as screen:
            control.preflight(Path('/isolated'),'kernel',compatibility=plan,module_manifest_sha256='a'*64)
        self.assertEqual(checks,[{'manifest_digest':'a'*64}])
        screen.assert_called_once_with(Path('/isolated'),'kernel','a'*64)

    def test_preflight_forwards_pinned_session_to_guest(self):
        instance = str(uuid.uuid4())
        contract = {'guest_protocol_minor': 0, 'required_capabilities': ['desktop.preflight'],
                    'guest_profile_sha256': 'p', 'sharing_path': 'opaque-v1'}
        plan = {'source': 'release-declaration', 'contract': contract}
        def load(path):
            if path.endswith('module-reuse.py'):
                return {'check': lambda *args, **kwargs: {'reload_required': False}}
            if path.endswith('kernel-exports.py'):
                return {'audit': lambda *args: {'symbol_crc_compatibility_admitted': True}}
            raise AssertionError(path)
        with patch.object(control.os, 'geteuid', return_value=0), patch.object(control.runpy, 'run_path', side_effect=load), \
             patch.object(control, 'protocol_preflight', return_value={'sharing_path': 'opaque-v1', 'account': {'uid': 1000}}) as guest:
            control.preflight(Path('/isolated'), 'kernel', compatibility=plan, session_instance=instance)
        self.assertEqual(guest.call_args.kwargs['session_instance'], instance)

    def test_prepare_loads_selected_independent_group(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); (root/'guest').mkdir(); (root/'module-group').mkdir()
            commands=[]
            def stop_after_modules(_instance):
                raise RuntimeError('Stop before device operations')
            with patch.object(control,'run',side_effect=lambda argv,**kwargs: commands.append(argv)), \
                 patch.object(control,'find_device',side_effect=stop_after_modules), \
                 patch.object(control.runpy,'run_path',return_value={'check':lambda *args:{'reload_required':False,'loaded':{}}}):
                with self.assertRaisesRegex(RuntimeError,'Stop before device'):
                    control.prepare(root,{},str(uuid.uuid4()))
            self.assertEqual(commands, [['/usr/bin/insmod',str(root/'module-group'/filename)]
                                        for _,filename in control.MODULES[:-1]])

    def test_prepare_passes_acknowledged_guest_path_to_actual_prepare_command(self):
        instance=str(uuid.UUID('11111111-1111-1111-1111-111111111111'))
        for sharing_path in ('opaque-v1','linear-v1'):
            with self.subTest(sharing_path=sharing_path), tempfile.TemporaryDirectory() as tmp:
                root=Path(tmp);device=root/'device';device.mkdir()
                (device/'vendor').write_text('0x1af4');(device/'device').write_text('0x1050')
                (root/'guest').mkdir();(root/'guest/wsx_virtio_msg_pci.ko').write_bytes(b'fixture')
                target=root/'wsx-virtio-msg-pci';target.mkdir()
                cards=root/'cards';cards.mkdir();(cards/'card0').symlink_to(device/'drm-card')
                (device/'drm-card').mkdir()
                probe=root/'drivers_probe';probe.write_text('')
                submitted=[]
                def fake_path(value):
                    return {'/sys/bus/pci/drivers_probe':probe,'/sys/class/drm':cards}.get(str(value),Path(value))
                def fake_run(argv,**kwargs):
                    if argv[0]=='/usr/bin/insmod': (device/'driver').symlink_to(target)
                    if argv[:2]==['/usr/bin/wsx-guest','prepare']: submitted.append(json.loads(kwargs['input']))
                    return SimpleNamespace(stdout='{}')
                loaded=[name for name,_ in control.MODULES[:-1]]
                with patch.object(control,'Path',side_effect=fake_path), \
                     patch.object(control,'find_device',return_value=device), \
                     patch.object(control,'run',side_effect=fake_run), \
                     patch.object(control.runpy,'run_path',return_value={'check':lambda *a:{'reload_required':False,'loaded':loaded}}):
                    result=control.prepare(root,dict(guest_protocol=True,vendor_id=0x1af4,device_id=0x1050,
                        port=20000,aperture_size=4096,device_number=1),instance,sharing_path=sharing_path)
                self.assertEqual(result['sharing_path'],sharing_path)
                self.assertEqual(submitted[0]['sharing_path'],sharing_path)
                self.assertEqual(submitted[0]['drm_node'],'/dev/dri/card0')

    def test_linear_selection_reaches_guest_preflight_but_legacy_is_refused(self):
        contract=dict(guest_protocol_minor=0,required_capabilities=['desktop.preflight'],
                      guest_profile_sha256='p',sharing_path='opaque-v1')
        plan=dict(source='release-declaration',contract=contract)
        modules={'reload_required':False}
        reply=dict(sharing_path='linear-v1',account={'initialized':True,'uid':1000})
        def load(path):
            if path.endswith('module-reuse.py'): return {'check':lambda root,previous:modules}
            if path.endswith('kernel-exports.py'): return {'audit':lambda kernel,guest:{'symbol_crc_compatibility_admitted':True}}
            raise AssertionError(path)
        with patch.object(control.os,'geteuid',return_value=0),patch.object(control.runpy,'run_path',side_effect=load), \
             patch.object(control,'protocol_preflight',return_value=reply) as call:
            result=control.preflight(Path('/isolated'),'kernel',compatibility=plan,requested_sharing_path='linear-v1')
            self.assertEqual(result['sharing_path'],'linear-v1')
            self.assertEqual(call.call_args.kwargs['requested_sharing_path'],'linear-v1')
        with patch.object(control.os,'geteuid',return_value=0),patch.object(control.runpy,'run_path') as load:
            with self.assertRaisesRegex(ValueError,'declared protocol'):
                control.preflight(Path('/isolated'),'kernel',compatibility={'source':'legacy'},requested_sharing_path='linear-v1')
            load.assert_not_called()

    def test_prepare_refuses_unadmitted_path_before_device_operations(self):
        instance=str(uuid.UUID('11111111-1111-1111-1111-111111111111'))
        for profile,path in [({},'linear-v1'),({'guest_protocol':True},'staging-copy'),
                             ({'guest_protocol':True},'unknown')]:
            with patch.object(control.runpy,'run_path') as load:
                with self.assertRaisesRegex(ValueError,'sharing path'):
                    control.prepare(Path('/isolated'),profile,instance,sharing_path=path)
                load.assert_not_called()

    def test_linear_preflight_requires_capability_and_exact_acknowledgement(self):
        contract = dict(guest_protocol_minor=0, required_capabilities=['desktop.preflight'],
                        guest_profile_sha256='p', sharing_path='opaque-v1')
        caps = dict(major=1, minor=0, capabilities=['desktop.preflight', 'graphics.linear-preflight-v1'])
        reply = dict(compatible=True, guest_interface_abi=1, guest_profile_sha256='p', sharing_path='linear-v1')
        with patch.object(control, 'guest_call', side_effect=[caps, reply]) as call:
            self.assertEqual(control.protocol_preflight(contract, True, 'linear-v1'), reply)
            self.assertEqual(call.call_args_list[1].args[1]['requested_sharing_path'], 'linear-v1')
        with patch.object(control, 'guest_call', return_value=dict(major=1,minor=0,capabilities=['desktop.preflight'])) as call:
            with self.assertRaisesRegex(ValueError, 'explicit LINEAR'):
                control.protocol_preflight(contract, requested_sharing_path='linear-v1')
            self.assertEqual(call.call_count, 1)
        with patch.object(control, 'guest_call', side_effect=[caps, dict(reply, sharing_path='opaque-v1')]):
            with self.assertRaisesRegex(ValueError, 'did not acknowledge'):
                control.protocol_preflight(contract, requested_sharing_path='linear-v1')
        for invalid in ('staging-copy', '', 'future'):
            with patch.object(control, 'guest_call', return_value=caps):
                with self.assertRaisesRegex(ValueError, 'Unsupported optional'):
                    control.protocol_preflight(contract, requested_sharing_path=invalid)

    def test_declared_guest_never_opens_private_metadata_or_falls_back(self):
        contract = dict(guest_protocol_minor=0, required_capabilities=['desktop.preflight'],
                        guest_profile_sha256='p', sharing_path='opaque-v1')
        reply = dict(compatible=True, guest_interface_abi=1, guest_profile_sha256='p', sharing_path='opaque-v1')
        plan = dict(source='release-declaration', contract=contract)
        with patch.object(control, 'trusted_bytes', side_effect=AssertionError('Private metadata access')), \
             patch.object(control, 'guest_call', side_effect=[dict(major=1,minor=4,capabilities=['desktop.preflight']), reply]) as call:
            self.assertEqual(control.verify_components('unknown-future-desktop', plan), reply)
            self.assertEqual(call.call_count, 2)
        with patch.object(control, 'trusted_bytes', side_effect=AssertionError('Legacy fallback')), \
             patch.object(control, 'guest_call', side_effect=ValueError('Unsupported major')):
            with self.assertRaisesRegex(ValueError, 'Unsupported major'):
                control.verify_components('wsx-omarchy', plan)

    def test_retirement_accepts_only_same_instance_success(self):
        namespace = dict(subprocess=subprocess)
        exec((ROOT/'integration/graphics-lifetime/protocol-retirement.py').read_text(), namespace)
        instance = uuid.UUID('11111111-1111-1111-1111-111111111111')
        reply = dict(protocol='wsx-guest',major=1,minor=0,id=instance.hex,ok=True,
                     result=dict(instance=str(instance),session_retired=True,gpu_retired=False))
        for kind in ('valid','wrong-id','wrong-instance','error'):
            value = json.loads(json.dumps(reply))
            if kind == 'wrong-id': value['id']='different'
            if kind == 'wrong-instance': value['result']['instance']='different'
            if kind == 'error': value['ok']=False
            with patch.object(subprocess, 'run', return_value=SimpleNamespace(stdout=json.dumps(value))) as call:
                if kind == 'valid': namespace['stop_protocol'](instance)
                else:
                    with self.assertRaises(RuntimeError): namespace['stop_protocol'](instance)
                self.assertEqual(call.call_count, 1)
                self.assertNotIn('timeout', call.call_args.kwargs)

    def test_retirement_preserves_guest_error_detail(self):
        namespace=dict(subprocess=subprocess)
        exec((ROOT/'integration/graphics-lifetime/protocol-retirement.py').read_text(),namespace)
        instance=uuid.UUID('11111111-1111-1111-1111-111111111111')
        reply=dict(protocol='wsx-guest',major=1,minor=0,id=instance.hex,ok=False,
                   error=dict(code='session_retained',message='Residual PID 42'))
        with patch.object(subprocess,'run',return_value=SimpleNamespace(stdout=json.dumps(reply))):
            with self.assertRaisesRegex(RuntimeError,'Residual PID 42'):
                namespace['stop_protocol'](instance)


if __name__ == '__main__': unittest.main()
