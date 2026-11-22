#!/usr/bin/env python3
"""Build one recorded kernel target without asserting release coverage."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TAG = re.compile(r'linux-msft-wsl-([0-9]+(?:\.[0-9]+){3})\Z')
COMMIT = re.compile(r'[0-9a-f]{40}\Z')
SHA = re.compile(r'[0-9a-f]{64}\Z')
VERSION = re.compile(r'([0-9]+(?:\.[0-9]+){3})-([0-9]+)\Z')


def has_vermagic_evidence(path):
    if path.is_symlink():
        raise ValueError('Unsafe target vermagic evidence')
    if not path.exists():
        return False
    if not path.is_file() or path.stat().st_size > 65536:
        raise ValueError('Unsafe target vermagic evidence')
    return True


def target_input(root, tag):
    target = TAG.fullmatch(tag)
    if not target:
        raise ValueError('Unsupported target tag')
    file = root / 'target-inputs' / (tag + '.json')
    if file.is_symlink() or not file.is_file() or file.stat().st_size > 65536:
        raise ValueError('Target package input is missing or unsafe')
    value = json.loads(file.read_text())
    if not isinstance(value, dict) or set(value) != {'version', 'sha256', 'package'}:
        raise ValueError('Invalid target package input')
    version = VERSION.fullmatch(value['version']) if isinstance(value['version'], str) else None
    if not version or version.group(1) != target.group(1) or not isinstance(value['sha256'], str) or not SHA.fullmatch(value['sha256']):
        raise ValueError('Target package version or digest is invalid')
    if not isinstance(value['package'], str):
        raise ValueError('Invalid target package path')
    relative = Path(value['package'])
    if relative.is_absolute() or '..' in relative.parts or not relative.parts:
        raise ValueError('Unsafe target package path')
    path = (root / relative).resolve(strict=True)
    if not path.is_relative_to(root.resolve()) or not path.is_file():
        raise ValueError('Target package is outside the coverage directory')
    if path.stat().st_size > 512 * 1024 * 1024:
        raise ValueError('Target package exceeds size limit')
    with path.open('rb') as stream:
        if hashlib.file_digest(stream, 'sha256').hexdigest() != value['sha256']:
            raise ValueError('Target package digest mismatch')
    return value, path


def restore_prebuilt(candidate, workspace, tag, commit, package, generation):
    if candidate is None:
        return False
    try:
        record = json.loads((candidate / 'receipt.json').read_text())
        source = candidate / 'workspace'
        build = json.loads((source / 'modules-build/build-result.json').read_text())
        inspection = json.loads((source / 'target-inspection/inspection.json').read_text())
        expected = {'tag': tag, 'target_source_commit': commit,
                    'package_version': package['version'], 'package_sha256': package['sha256'],
                    'build_generation': generation}
        if (record != expected or build['kernel_source_commit'] != commit
                or build['build_generation'] != generation
                or inspection['package_version'] != package['version']
                or inspection['package_sha256'] != package['sha256']
                or not source.is_dir() or source.is_symlink()
                or any(path.is_symlink() or not (path.is_file() or path.is_dir())
                       for path in source.rglob('*'))):
            return False
    except (OSError, ValueError, KeyError, TypeError):
        return False
    shutil.copytree(source, workspace)
    return True


def save_prebuilt(candidate, workspace, tag, commit, package, generation):
    candidate.mkdir()
    shutil.copytree(workspace, candidate / 'workspace')
    (candidate / 'receipt.json').write_text(json.dumps({
        'tag': tag, 'target_source_commit': commit, 'package_version': package['version'],
        'package_sha256': package['sha256'], 'build_generation': generation,
    }, sort_keys=True) + '\n')


def build(tag, commit, root):
    if re.fullmatch(r'linux-msft-wsl-[0-9]+(?:\.[0-9]+){2,3}-rc[1-9][0-9]*', tag):
        raise ValueError('RC target discovered, but its exact Microsoft kernel package mapping is not implemented: ' + tag)
    if not TAG.fullmatch(tag) or not COMMIT.fullmatch(commit):
        raise ValueError('Invalid build target')
    root = root.resolve(strict=True)
    input_file = root / 'target-inputs' / (tag + '.json')
    if not input_file.exists() and not input_file.is_symlink():
        spec = importlib.util.spec_from_file_location('target_fetcher', ROOT / 'scripts/fetch-target-kernel-package.py')
        fetcher = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fetcher)
        fetcher.fetch(tag, root)
    value, package = target_input(root, tag)
    attempts = root / 'diagnostic-builds' / tag
    attempts.mkdir(parents=True, exist_ok=True)
    attempt = Path(tempfile.mkdtemp(prefix='attempt-', dir=attempts))
    workspace = attempt / 'workspace'
    cache = Path(os.environ['WSX_KERNEL_SHARED_CACHE']) if os.environ.get('WSX_KERNEL_SHARED_CACHE') else root / 'build-cache'
    spec = importlib.util.spec_from_file_location('staging_snapshots', ROOT / 'scripts/kernel-staging-snapshots.py')
    snapshots = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(snapshots)
    index = snapshots.extract_current(root, attempt / 'prior-snapshot')
    spec = importlib.util.spec_from_file_location('kernel_generation', ROOT / 'scripts/kernel_build_generation.py')
    generation_module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(generation_module)
    current_generation, _ = generation_module.generation(ROOT)
    prescreen = None
    if index is not None and index['generation'] == current_generation and index['groups']:
        modprobe = shutil.which('modprobe')
        if not modprobe:
            raise ValueError('modprobe is required to screen existing modules')
        spec = importlib.util.spec_from_file_location('screen_kernel', ROOT / 'scripts/screen-kernel-target.py')
        screener = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(screener)
        prescreen = screener.crc_prescreen(package, value['sha256'], value['version'], attempt / 'prior-snapshot',
                                           current_generation, attempt / 'crc-prescreen', modprobe)
    proof_file = root / 'target-vermagic' / (tag + '.json')
    if prescreen is not None and prescreen['candidates'] and has_vermagic_evidence(proof_file):
        proof = json.loads(proof_file.read_text())
        if (not isinstance(proof, dict) or set(proof) != {'tag', 'source_commit', 'kernel_build_id', 'vermagic'}
                or proof['tag'] != tag or proof['source_commit'] != commit
                or not isinstance(proof['kernel_build_id'], str)
                or not re.fullmatch(r'[0-9a-f]{8,128}', proof['kernel_build_id'])
                or not isinstance(proof['vermagic'], str) or not proof['vermagic'].strip()
                or '\n' in proof['vermagic']):
            raise ValueError('Invalid target vermagic evidence; evidence retained at ' + str(attempt))
        modprobe, modinfo = shutil.which('modprobe'), shutil.which('modinfo')
        if not modprobe or not modinfo:
            raise ValueError('modprobe and modinfo are required to screen existing modules')
        spec = importlib.util.spec_from_file_location('screen_kernel', ROOT / 'scripts/screen-kernel-target.py')
        screener = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(screener)
        screening = screener.screen(package, value['sha256'], value['version'], attempt / 'prior-snapshot',
                                    current_generation, proof['vermagic'], attempt / 'screening', modprobe, modinfo)
        if (screening['kernel_build_id'] != proof['kernel_build_id']
                or screening['kernel_build_id'] != prescreen['kernel_build_id']):
            raise ValueError('Target vermagic evidence identifies a different kernel')
        if screening['decision'] == 'inspection_failed':
            raise ValueError('Existing module screening failed; evidence retained at ' + str(attempt))
        if screening['decision'] == 'candidate_requires_admission':
            raise ValueError('Existing module candidate requires independent admission; evidence retained at ' + str(attempt))
    prior_snapshot = attempt / 'prior-snapshot'
    if prior_snapshot.exists():
        if prior_snapshot.is_symlink() or not prior_snapshot.is_dir():
            raise ValueError('Unsafe temporary snapshot directory')
        shutil.rmtree(prior_snapshot)
    candidate_path = os.environ.get('WSX_KERNEL_PREBUILT_WORKSPACE')
    reused = restore_prebuilt(Path(candidate_path) if candidate_path else None, workspace,
                              tag, commit, value, current_generation)
    if reused:
        print(f'Reusing prebuilt workspace for {value["version"]}', flush=True)
    else:
        command = ['bash', str(ROOT / 'scripts/build-target-kernel-modules.sh'), tag, commit,
                   str(package), value['sha256'], value['version'], str(workspace), str(cache)]
        result = subprocess.run(command, check=False)
        if result.returncode:
            raise ValueError(f'Target build failed with exit code {result.returncode}; evidence retained at {attempt}')
    # The build driver records the CRC audit before cleaning its source tree.
    # Revalidate that same evidence instead of running a second, post-cleanup
    # scan which could fail after the diagnostic intermediates were removed.
    spec = importlib.util.spec_from_file_location('kernel_finalizer', ROOT / 'scripts/finalize-kernel-build.py')
    finalizer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(finalizer)
    result_data = json.loads((workspace / 'modules-build/build-result.json').read_text())
    if result_data.get('kernel_source_commit') != commit:
        raise ValueError('Build source differs from target; evidence retained at ' + str(attempt))
    finalizer.verify_static_crc(workspace, result_data)
    spec = importlib.util.spec_from_file_location('snapshot_assembler', ROOT / 'scripts/assemble-kernel-snapshot.py')
    assembler = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(assembler)
    snapshot = assembler.assemble(workspace / 'modules-build', workspace / 'static-crc-audit.json',
                                  tag, attempt / 'snapshot')
    merged = snapshots.merge_candidate(root, attempt / 'snapshot')
    save_path = os.environ.get('WSX_KERNEL_SAVE_PREBUILT_WORKSPACE')
    if save_path:
        save_prebuilt(Path(save_path), workspace, tag, commit, value, current_generation)
    report = {'schema_version': 1, 'tag': tag, 'target_source_commit': commit,
              'package_version': value['version'], 'package_sha256': value['sha256'],
              'attempt': str(attempt.relative_to(root)), 'static_crc_match': True,
              'snapshot_group': snapshot['group'],
              'staging_snapshot_sha256': merged['sha256'],
              'reuse_prescreen_candidates': prescreen['candidates'] if prescreen else [],
              'reuse_decision': 'verified_prebuilt_workspace' if reused else 'built_without_proven_reuse',
              'source_correspondence_verified': False, 'runtime_admitted': False,
              'release_ready': False}
    with (attempt / 'diagnostic-result.json').open('x', encoding='utf-8') as output:
        json.dump(report, output, indent=2)
        output.write('\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--coverage', required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(build(args.tag, args.commit, args.coverage)))


if __name__ == '__main__':
    main()
