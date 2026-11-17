import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('module_sources', root / 'scripts/export-kernel-module-sources.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

class SourceExportTests(unittest.TestCase):
    def test_reuses_when_workflow_changes_but_not_when_source_changes(self):
        with tempfile.TemporaryDirectory() as tmp:
            r=Path(tmp); (r/'kernel-modules').mkdir(); (r/'kernel-modules/source.c').write_text('one')
            for path in mod.SCRIPTS:
                p=r/path; p.parent.mkdir(parents=True, exist_ok=True); p.write_text(path)
            deps=r/'deps.json'; deps.write_text(json.dumps({'schema_version':1,'packages':{'gcc':'1'}}))
            a=mod.export(r,deps,r/'a.tar.gz')
            (r/'.github/workflows').mkdir(parents=True); (r/'.github/workflows/build.yml').write_text('changed')
            b=mod.export(r,deps,r/'b.tar.gz')
            self.assertEqual(a['sha256'],b['sha256'])
            (r/'kernel-modules/source.c').write_text('two')
            c=mod.export(r,deps,r/'c.tar.gz')
            self.assertNotEqual(a['sha256'],c['sha256'])
            deps.write_text(json.dumps({'schema_version':1,'packages':{'gcc':'2'}}))
            d=mod.export(r,deps,r/'d.tar.gz')
            self.assertNotEqual(c['sha256'],d['sha256'])

if __name__ == '__main__': unittest.main()
