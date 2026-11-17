import gzip
import importlib.util
import io
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

SCRIPT=Path(__file__).resolve().parents[1]/'scripts/verify-kernel-source-archive.py'
spec=importlib.util.spec_from_file_location('verify_kernel_source_archive',SCRIPT)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)


class VerifyKernelArchiveTests(unittest.TestCase):
    def test_archive_matches_git_tree_with_modes_and_symlink(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);repo=root/'repo';repo.mkdir()
            subprocess.run(['git','init','-q',str(repo)],check=True)
            def git(*args):return subprocess.check_output(['git','-C',str(repo),*args],text=True).strip()
            git('config','user.name','Test');git('config','user.email','test@example.invalid')
            (repo/'dir').mkdir();(repo/'dir'/'data').write_bytes(b'data')
            (repo/'exec').write_text('run');(repo/'exec').chmod(0o755)
            (repo/'link').symlink_to('dir/data');git('add','dir/data','exec','link');git('commit','-qm','source')
            tree=git('rev-parse','HEAD^{tree}')
            for prefix,strip in (('',0),('snapshot/',1)):
                data=subprocess.check_output(['git','-C',str(repo),'archive','--format=tar','--prefix='+prefix,'HEAD'])
                archive=root/f'kernel-{strip}.tar.gz';archive.write_bytes(gzip.compress(data,mtime=0))
                self.assertEqual(module.verify(archive,tree,strip)['git_tree'],tree)
                with self.assertRaisesRegex(ValueError,'does not match'):
                    module.verify(archive,'0'*40,strip)

    def test_rejects_traversal(self):
        with tempfile.TemporaryDirectory() as tmp:
            archive=Path(tmp)/'malicious.tar.gz'
            with tarfile.open(archive,'w:gz') as tar:
                info=tarfile.TarInfo('../escape');info.size=1;tar.addfile(info,io.BytesIO(b'x'))
            with self.assertRaisesRegex(ValueError,'Unsafe'):
                module.compute_tree(archive)

if __name__=='__main__':unittest.main()
