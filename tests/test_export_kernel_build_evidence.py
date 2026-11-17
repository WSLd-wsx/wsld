import hashlib
import importlib.util
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

SCRIPT=Path(__file__).resolve().parents[1]/'scripts/export-kernel-build-evidence.py'
spec=importlib.util.spec_from_file_location('build_evidence',SCRIPT)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)


def write(root, name, value):
    path=root/name;path.parent.mkdir(parents=True,exist_ok=True)
    path.write_bytes(value if isinstance(value,bytes) else json.dumps(value).encode())


def fixture(root):
    commit='c'*40; tree='d'*40; build_id='a'*40; image_sha='f'*64
    modules={}
    for name in module.finalizer.MODULES:
        data=name.encode();write(root,'modules-build/modules/'+name,data)
        modules[name]={'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data)}
    config=b'config';symvers=b'symvers';original=b'original config'
    inputs={'scripts/build.py':'b'*64}
    generation=hashlib.sha256(json.dumps({'schema_version':1,'inputs':inputs},sort_keys=True,separators=(',',':')).encode()).hexdigest()
    result={'schema_version':1,'build_generation':generation,'build_inputs':inputs,'kernel_source_commit':commit,
            'kernel_config_sha256':hashlib.sha256(config).hexdigest(),
            'kernel_symvers_sha256':hashlib.sha256(symvers).hexdigest(),
            'kernel_symvers_evidence':{'kernel_build_id':build_id,'kernel_image_sha256':image_sha},'modules':modules}
    write(root,'modules-build/build-result.json',result);write(root,'modules-build/build.log',b'compile log')
    write(root,'kernel-evidence/config',config);write(root,'kernel-evidence/Module.symvers',symvers)
    source={'source_commit':commit,'source_tree':tree,'package_build_revision':commit,
            'package_build_tree':tree,'source_tag_verified':True,'source_tree_matches_package_metadata':True}
    write(root,'kernel-evidence/wsx-source-evidence.json',source)
    write(root,'kernel-evidence/wsx-target-prepared.json',{'ok':True})
    write(root,'kernel-evidence/wsx-symvers-evidence.json',{'ok':True})
    write(root,'target-inspection/inspection.json',{'kernel_config_sha256':hashlib.sha256(original).hexdigest(),
          'kernel_build_id':build_id,'kernel_image_sha256':image_sha,'upstream_build_metadata':{'git_revision':commit}})
    write(root,'target-inspection/kernel_config',original)
    write(root,'target-inspection/kernel-exports.json',{'kernel_build_id':build_id,'kernel_image_sha256':image_sha})
    write(root,'kernel-source.tar.gz',b'kernel source archive')
    write(root,'kernel-source-receipt.json',{'source_commit':commit,'source_tree':tree,
          'package_build_revision':commit,'package_build_tree':tree,'bytes':len(b'kernel source archive'),
          'sha256':hashlib.sha256(b'kernel source archive').hexdigest()})
    write(root,'cleanup-result.json',{'ok':True})
    write(root,'static-crc-audit.json',{'schema_version':1,'kernel_build_id':build_id,'crc_match':True,
          'modules':{name:{'sha256':entry['sha256'],'missing':[],'mismatches':[],
                           'has_module_layout':True,'import_count':1} for name,entry in modules.items()}})


class EvidenceTests(unittest.TestCase):
    def test_exports_recheckable_evidence_without_claiming_reproducibility(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);fixture(root);desc=module.export(root,root/'evidence.tar.gz')
            self.assertEqual(desc['sha256'],hashlib.sha256((root/'evidence.tar.gz').read_bytes()).hexdigest())
            with tarfile.open(root/'evidence.tar.gz') as tar:
                manifest=json.load(tar.extractfile('manifest.json'))
                self.assertFalse(manifest['source_correspondence_verified'])
                self.assertEqual(set(manifest['modules']),module.finalizer.MODULES)
                self.assertIn('kernel-evidence/Module.symvers',tar.getnames())

    def test_rejects_changed_module_or_input(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);fixture(root);(root/'modules-build/modules/virtio_msg.ko').write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError,'does not match module'):
                module.export(root,root/'evidence.tar.gz')
            self.assertFalse((root/'evidence.tar.gz').exists())
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);fixture(root);(root/'kernel-evidence/config').write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError,'do not match'):
                module.export(root,root/'evidence.tar.gz')

if __name__=='__main__':unittest.main()
