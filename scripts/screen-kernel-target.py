#!/usr/bin/env python3
"""Inspect one kernel package and screen a verified module snapshot."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


inspector = load('kernel_package_inspector', 'inspect-wsl-kernel-package.py')
selector = load('kernel_group_selector', 'select-kernel-module-group.py')
generation_module = load('kernel_build_generation', 'kernel_build_generation.py')
crc_module = load('kernel_crc_prescreen', 'compare-kernel-module-crcs.py')


def crc_prescreen(package, package_hash, version, snapshot, generation, destination, modprobe):
    if destination.exists() or destination.is_symlink():
        raise ValueError('Target screening destination already exists')
    index = selector.package.validate(snapshot)
    if index['generation'] != generation:
        raise ValueError('Snapshot build generation differs; all targets need rebuilding')
    destination.mkdir(parents=True)
    evidence = inspector.inspect(package, package_hash, version, destination / 'kernel')
    exports = json.loads((destination / 'kernel/kernel-exports.json').read_text())
    if exports['kernel_build_id'] != evidence['kernel_build_id']:
        raise ValueError('Kernel export identity differs from inspected package')
    groups = {}
    for group in sorted(index['groups']):
        audit = crc_module.compare(exports, snapshot / 'groups' / group, modprobe)
        valid_imports = all(item['import_count'] > 0 and item['has_module_layout']
                            for item in audit['modules'].values())
        groups[group] = {'crc_match': audit['crc_match'], 'valid_versioned_imports': valid_imports}
    candidates = [group for group, value in groups.items()
                  if value['crc_match'] and value['valid_versioned_imports']]
    result = {'schema_version': 1, 'kernel_build_id': evidence['kernel_build_id'],
              'groups': groups, 'candidates': candidates, 'runtime_admitted': False,
              'scope': 'CRC prescreen only; matching groups need full loader and component checks'}
    with (destination / 'crc-prescreen.json').open('x', encoding='utf-8') as output:
        json.dump(result, output, indent=2)
        output.write('\n')
    return result


def screen(package, package_hash, version, snapshot, generation, vermagic, destination, modprobe, modinfo):
    if destination.exists() or destination.is_symlink():
        raise ValueError('Target screening destination already exists')
    index = selector.package.validate(snapshot)
    if index['generation'] != generation:
        raise ValueError('Snapshot build generation differs; all targets need rebuilding')
    destination.mkdir(parents=True)
    evidence = inspector.inspect(package, package_hash, version, destination / 'kernel')
    exports = json.loads((destination / 'kernel/kernel-exports.json').read_text())
    if exports['kernel_build_id'] != evidence['kernel_build_id']:
        raise ValueError('Kernel export identity differs from inspected package')
    selection = selector.select(snapshot, generation, exports, vermagic, modprobe, modinfo)
    if not selection['inspection_complete']:
        decision = 'inspection_failed'
    elif selection['static_candidates']:
        decision = 'candidate_requires_admission'
    else:
        decision = 'build_required'
    report = {'schema_version': 1, 'package_version': version, 'package_sha256': package_hash,
              'kernel_build_id': evidence['kernel_build_id'], 'generation': generation,
              'decision': decision, 'selection': selection, 'release_ready': False,
              'scope': 'Diagnostic screening only; image provenance, source correspondence and runtime admission remain unverified'}
    with (destination / 'screening.json').open('x', encoding='utf-8') as output:
        json.dump(report, output, indent=2)
        output.write('\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', required=True, type=Path)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--snapshot', required=True, type=Path)
    parser.add_argument('--generation', required=True)
    parser.add_argument('--target-vermagic', required=True)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    current_generation, _ = generation_module.generation(ROOT)
    if args.generation != current_generation:
        raise ValueError('Requested generation differs from current build inputs')
    modprobe, modinfo = shutil.which('modprobe'), shutil.which('modinfo')
    if not modprobe or not modinfo:
        raise ValueError('modprobe and modinfo are required')
    report = screen(args.package, args.sha256, args.version, args.snapshot, args.generation,
                    args.target_vermagic, args.output, modprobe, modinfo)
    print(json.dumps({'decision': report['decision'], 'release_ready': False}))
    if report['decision'] == 'inspection_failed':
        raise SystemExit(2)


if __name__ == '__main__':
    main()
