#!/usr/bin/env python3
"""Screen verified snapshot module groups against one kernel's exports."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


package = load('module_snapshot', 'kernel-package.py')
screen = load('module_screen', 'evaluate-kernel-module-group.py')


def select(snapshot, generation, exports, vermagic, modprobe, modinfo, reviewed_crc_vermagic=False):
    index = package.validate(snapshot)
    if index['generation'] != generation:
        raise ValueError('Snapshot build generation differs; modules must be rebuilt')
    results = []
    for group in sorted(index['groups']):
        try:
            result = screen.evaluate(exports, snapshot / 'groups' / group, vermagic, modprobe, modinfo,
                                     reviewed_crc_vermagic)
            results.append({'group': group, 'static_candidate': result['static_candidate'],
                            'inspection_failed': False, 'reasons': result['reasons']})
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            results.append({'group': group, 'static_candidate': False,
                            'inspection_failed': True, 'reasons': [str(error)]})
    candidates = [item['group'] for item in results if item['static_candidate']]
    return {'schema_version': 1, 'generation': generation, 'kernel_build_id': exports['kernel_build_id'],
            'static_candidates': candidates, 'groups': results, 'runtime_admitted': False,
            'inspection_complete': all(not item['inspection_failed'] for item in results),
            'reviewed_crc_vermagic': reviewed_crc_vermagic,
            'scope': 'Static screening only; source provenance, loader rules, component pairing and lifecycle require separate admission'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--snapshot', required=True, type=Path)
    parser.add_argument('--generation', required=True)
    parser.add_argument('--kernel-exports', required=True, type=Path)
    parser.add_argument('--target-vermagic', required=True)
    parser.add_argument('--reviewed-crc-vermagic', action='store_true',
                        help='Target loader behavior has been independently reviewed; static screening only')
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    modprobe, modinfo = shutil.which('modprobe'), shutil.which('modinfo')
    if not modprobe or not modinfo:
        raise ValueError('modprobe and modinfo are required')
    result = select(args.snapshot, args.generation, json.loads(args.kernel_exports.read_text()),
                    args.target_vermagic, modprobe, modinfo, args.reviewed_crc_vermagic)
    with args.output.open('x', encoding='utf-8') as output:
        json.dump(result, output, indent=2)
        output.write('\n')
    print(json.dumps({'static_candidates': len(result['static_candidates']),
                      'inspection_complete': result['inspection_complete'], 'runtime_admitted': False}))
    if not result['inspection_complete']:
        raise SystemExit(2)


if __name__ == '__main__':
    main()
