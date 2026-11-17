"""ABI 2 module reuse policy, without loading modules or creating devices."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('reuse',Path(__file__).resolve().parents[1]/'core/guest/module-reuse.py')
r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)

class MultiOwnerReuse(unittest.TestCase):
    def test_independent_group_allows_empty_but_rejects_partial_load(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);sys=root/'sys';run=root/'run';group=root/'module-group'
            group.mkdir();sys.mkdir();run.mkdir()
            (group/'wsx_virtio_msg_pci.ko').write_bytes(b'wsx_owner_configuration_abi=2\0')
            verified={'owner_configuration_abi':2}
            with patch.object(r,'SYS',sys),patch.object(r,'RUN',run), \
                 patch.object(r.runpy,'run_path',return_value={'verify':lambda *args:verified}), \
                 patch.object(r,'inspect_loaded',return_value={}):
                result=r.check(root,None)
                self.assertEqual(result['loaded'],{})
                self.assertTrue(result['multi_owner'])
            with patch.object(r,'SYS',sys),patch.object(r,'RUN',run), \
                 patch.object(r.runpy,'run_path',return_value={'verify':lambda *args:verified}), \
                 patch.object(r,'inspect_loaded',return_value={'virtio_msg':'note'}):
                with self.assertRaisesRegex(RuntimeError,'partially loaded'):
                    r.check(root,None)

    def test_explicit_manifest_digest_is_forwarded_and_missing_group_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);sys=root/'sys';run=root/'run';group=root/'module-group'
            group.mkdir();sys.mkdir();run.mkdir()
            (group/'wsx_virtio_msg_pci.ko').write_bytes(b'wsx_owner_configuration_abi=2\0')
            seen=[]
            def verify(_directory,digest,_script):
                seen.append(digest);return {'owner_configuration_abi':2}
            with patch.object(r,'SYS',sys),patch.object(r,'RUN',run), \
                 patch.object(r.runpy,'run_path',return_value={'verify':verify}), \
                 patch.object(r,'inspect_loaded',return_value={}):
                r.check(root,None,manifest_digest='a'*64)
                self.assertEqual(seen,['a'*64])
                (group/'wsx_virtio_msg_pci.ko').unlink();group.rmdir()
                with self.assertRaisesRegex(ValueError,'Both module group'):
                    r.check(root,None,manifest_digest='a'*64)

    def test_bound_peer_does_not_authorize_unload_or_block_reuse(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);sys=root/'sys';run=root/'run';(root/'guest').mkdir();run.mkdir()
            (root/'guest/wsx_virtio_msg_pci.ko').write_bytes(b'wsx_owner_configuration_abi=2\0')
            (sys/'module/wsx_virtio_msg_pci').mkdir(parents=True)
            driver=sys/'bus/pci/drivers/wsx-virtio-msg-pci';driver.mkdir(parents=True)
            (driver/'configuration_abi').write_text('2\n')
            (driver/'0001:00:00.0').symlink_to('/sys/devices/foreign-peer')
            with patch.object(r,'SYS',sys),patch.object(r,'RUN',run),patch.object(r,'inspect_loaded',return_value={'wsx_virtio_msg_pci':'verified-note'}),patch.object(r.subprocess,'run') as mutate:
                self.assertTrue(r.check(root,None)['multi_owner'])
                self.assertFalse(r.recycle(root,None,False)['owner_module_removed'])
                mutate.assert_not_called()
                (driver/'configuration_abi').write_text('1\n')
                with self.assertRaises(RuntimeError):r.check(root,None)
                (driver/'configuration_abi').write_text('2\n')
                (run/'wsx').mkdir();(run/'wsx/local-session.json').write_text('{}')
                with self.assertRaises(RuntimeError):r.check(root,None)

    def test_descriptor_archive_commits_without_cross_filesystem_link(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);source=root/'run';archive=root/'var';source.mkdir();archive.mkdir()
            p=source/'instance.json';p.write_bytes(b'{"owned":true}')
            real_link=r.os.link
            def local_link(a,b,**kwargs):
                self.assertEqual(Path(a).parent,archive)
                return real_link(a,b,**kwargs)
            with patch.object(r.os,'link',side_effect=local_link):r.archive_descriptor(p,archive)
            self.assertFalse(p.exists());self.assertEqual((archive/p.name).read_bytes(),b'{"owned":true}')
            p.write_bytes(b'{"conflict":true}')
            with self.assertRaises(RuntimeError):r.archive_descriptor(p,archive)
            self.assertTrue(p.exists())

if __name__=='__main__':unittest.main()
