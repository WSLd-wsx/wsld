#!/usr/bin/env python3
"""Compare candidate imports with installed WSL module imports; never load modules."""
import argparse
import json
import os
import pathlib
import subprocess


def versions(path, option='--show-modversions'):
    result = subprocess.run(['/usr/bin/modprobe', option, str(path)],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            text=True, timeout=20, env={**os.environ, 'LC_ALL': 'C'})
    if result.returncode:
        if option == '--show-exports' and 'No data available' in result.stderr:
            return {}
        raise RuntimeError(f'Module version inspection failed: {path}: {result.stderr.strip()}')
    entries = {}
    for line in result.stdout.splitlines():
        crc, name = line.split()
        if not crc.startswith('0x') or name in entries:
            raise ValueError('Invalid or duplicate module version')
        entries[name] = crc.lower()
    return entries


def audit(candidate_dir, kernel_dir):
    baseline = {}
    installed = sorted(kernel_dir.rglob('*.ko'))
    if not installed or len(installed) > 10000:
        raise ValueError('Invalid installed module inventory')
    for module in installed:
        for name, crc in versions(module).items():
            baseline.setdefault(name, set()).add(crc)
    candidates = sorted(candidate_dir.glob('*.ko'))
    exports = {}
    for module in candidates:
        for name, crc in versions(module, '--show-exports').items():
            exports.setdefault(name, set()).add(crc)
    records = {}
    for module in candidates:
        imports = versions(module)
        matches, linked, mismatches, missing = [], [], [], []
        for name, crc in sorted(imports.items()):
            known = baseline.get(name)
            candidate = exports.get(name)
            if candidate:
                if candidate == {crc}:
                    linked.append(name)
                else:
                    mismatches.append({'symbol': name, 'candidate': crc, 'package_exports': sorted(candidate)})
            elif not known:
                missing.append(name)
            elif known == {crc}:
                matches.append(name)
            else:
                mismatches.append({'symbol': name, 'candidate': crc, 'installed': sorted(known)})
        records[module.name] = {'imports': len(imports), 'matched_installed_imports': len(matches),
                                'matched_candidate_exports': len(linked),
                                'unobserved': missing, 'mismatches': mismatches}
    if not records:
        raise ValueError('No candidate modules found')
    return {'installed_module_count': len(installed), 'modules': records,
            'scope': 'Installed-module imports and candidate package exports only; unobserved symbols and running kernel exports are not verified',
            'kernel_compatibility_admitted': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('candidate_dir', type=pathlib.Path)
    parser.add_argument('kernel_dir', type=pathlib.Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.candidate_dir, args.kernel_dir), sort_keys=True))
