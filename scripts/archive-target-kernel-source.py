#!/usr/bin/env python3
"""Archive the clean, pinned kernel source before build preparation."""
import argparse
import gzip
import importlib.util
import json
from pathlib import Path
import re
import shutil
import subprocess


def archive(kernel, output, receipt):
    if output.exists() or output.is_symlink() or receipt.exists() or receipt.is_symlink():
        raise ValueError('Kernel source archive or receipt already exists')
    kernel = kernel.resolve(strict=True)
    evidence_path = kernel / 'wsx-source-evidence.json'
    if evidence_path.is_symlink() or not evidence_path.is_file():
        raise ValueError('Kernel source evidence is missing')
    evidence = json.loads(evidence_path.read_text())
    commit, tree = evidence.get('source_commit'), evidence.get('source_tree')
    if (not isinstance(commit, str) or not re.fullmatch(r'[0-9a-f]{40}', commit)
            or not isinstance(tree, str) or not re.fullmatch(r'[0-9a-f]{40}', tree)
            or evidence.get('source_tag_verified') is not True
            or evidence.get('source_tree_matches_package_metadata') is not True
            or tree != evidence.get('package_build_tree')):
        raise ValueError('Kernel source identity and package metadata are not verified')
    git = ['git', '-C', str(kernel)]
    head = subprocess.check_output(git + ['rev-parse', 'HEAD'], text=True).strip()
    head_tree = subprocess.check_output(git + ['rev-parse', 'HEAD^{tree}'], text=True).strip()
    if head != commit or head_tree != tree or subprocess.check_output(git + ['status', '--porcelain', '--untracked-files=no'], text=True).strip():
        raise ValueError('Kernel source checkout is not clean at the verified commit')
    listing = subprocess.check_output(git + ['ls-tree', '-r', commit], text=True)
    if any(line.startswith('160000 ') for line in listing.splitlines()):
        raise ValueError('Kernel source contains unresolved submodules')
    try:
        with output.open('xb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed:
            process = subprocess.Popen(git + ['archive', '--format=tar', commit], stdout=subprocess.PIPE)
            try:
                shutil.copyfileobj(process.stdout, compressed, 1024 * 1024)
            finally:
                process.stdout.close()
            if process.wait() != 0:
                raise ValueError('Cannot archive kernel source tree')
        if subprocess.check_output(git + ['status', '--porcelain', '--untracked-files=no'], text=True).strip():
            raise ValueError('Kernel source changed while archiving')
        spec = importlib.util.spec_from_file_location('kernel_archive_verifier', Path(__file__).with_name('verify-kernel-source-archive.py'))
        verifier = importlib.util.module_from_spec(spec); spec.loader.exec_module(verifier)
        checked = verifier.verify(output, tree)
        result = {'schema_version': 1, 'source_commit': commit, 'source_tree': tree,
                  'package_build_revision': evidence['package_build_revision'],
                  'package_build_tree': evidence['package_build_tree'],
                  'sha256': checked['sha256'], 'bytes': checked['bytes'], 'source_files': checked['files']}
        receipt.write_text(json.dumps(result, sort_keys=True, indent=2) + '\n')
        return result
    except BaseException:
        output.unlink(missing_ok=True)
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--receipt', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(archive(args.kernel, args.output, args.receipt), sort_keys=True))
