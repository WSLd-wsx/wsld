#!/usr/bin/env python3
"""Evaluate static module reuse evidence without claiming kernel admission."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


crc = load('module_crcs', 'compare-kernel-module-crcs.py')
requirements = load('module_requirements', 'kernel-module-load-requirements.py')


def evaluate(kernel, modules, expected_vermagic, modprobe, modinfo, reviewed_crc_vermagic=False):
    if not isinstance(expected_vermagic, str) or not expected_vermagic.strip() or '\n' in expected_vermagic:
        raise ValueError('Target vermagic evidence is required')
    expected_vermagic = expected_vermagic.strip()
    imports = crc.compare(kernel, modules, modprobe)
    load_info = requirements.inspect(modules, modinfo)
    reasons = []
    if not imports['crc_match']:
        reasons.append('symbol_crc_mismatch_or_missing')
    if load_info['external_dependencies']:
        reasons.append('external_module_dependencies_unverified')
    for name, info in load_info['modules'].items():
        if info['sha256'] != imports['modules'][info['file']]['sha256']:
            raise ValueError('Module changed while checking: ' + name)
        module_magic = info['vermagic']
        parts, target_parts = module_magic.split(' ', 1), expected_vermagic.split(' ', 1)
        versioned_match = (reviewed_crc_vermagic and len(parts) == 2 and len(target_parts) == 2
                           and parts[1] == target_parts[1]
                           and 'modversions' in parts[1].split()
                           and imports['modules'][info['file']]['has_module_layout'])
        if module_magic != expected_vermagic and not versioned_match:
            reasons.append('vermagic_mismatch:' + name)
        if imports['modules'][info['file']]['import_count'] == 0:
            reasons.append('missing_versioned_imports:' + name)
        if not imports['modules'][info['file']]['has_module_layout']:
            reasons.append('missing_module_layout_crc:' + name)
        reachable, pending = set(), list(info['depends'])
        while pending:
            dependency = pending.pop()
            if dependency in reachable:
                continue
            reachable.add(dependency)
            if dependency in load_info['modules']:
                pending.extend(load_info['modules'][dependency]['depends'])
        for provider in imports['modules'][info['file']]['module_symbol_providers']:
            if provider.removesuffix('.ko').replace('-', '_') not in reachable:
                reasons.append('unreachable_symbol_provider:' + name + ':' + provider)
    return {'schema_version': 1, 'kernel_build_id': imports['kernel_build_id'],
            'target_vermagic': expected_vermagic, 'reviewed_crc_vermagic': reviewed_crc_vermagic,
            'crc': imports, 'load_requirements': load_info,
            'static_candidate': not reasons, 'reasons': reasons, 'runtime_admitted': False,
            'scope': 'Preliminary static screening only; target provenance, signature enforcement, namespace and GPL export rules, loader behavior, component pairing and GPU acceptance are not established'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel-exports', required=True, type=Path)
    parser.add_argument('--modules', required=True, type=Path)
    parser.add_argument('--target-vermagic', required=True)
    parser.add_argument('--reviewed-crc-vermagic', action='store_true',
                        help='Caller has independently reviewed the target loader same_magic behavior; still only static screening')
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    modprobe, modinfo = shutil.which('modprobe'), shutil.which('modinfo')
    if not modprobe or not modinfo:
        raise ValueError('modprobe and modinfo are required')
    result = evaluate(json.loads(args.kernel_exports.read_text()), args.modules,
                      args.target_vermagic, modprobe, modinfo, args.reviewed_crc_vermagic)
    with args.output.open('x', encoding='utf-8') as output:
        json.dump(result, output, indent=2)
        output.write('\n')
    print(json.dumps({'static_candidate': result['static_candidate'],
                      'reasons': result['reasons'], 'runtime_admitted': False}))
    raise SystemExit(0 if result['static_candidate'] else 1)


if __name__ == '__main__':
    main()
