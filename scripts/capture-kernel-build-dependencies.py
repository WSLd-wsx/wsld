#!/usr/bin/env python3
"""Record Runner package versions that can affect kernel module builds."""
import argparse
import json
from pathlib import Path
import subprocess
from kernel_runner_dependencies import PACKAGES



def capture(output):
    if output.exists() or output.is_symlink():
        raise ValueError('Dependency evidence already exists')
    versions = {}
    for name in PACKAGES:
        value = subprocess.check_output(['dpkg-query', '-W', '-f=${db:Status-Status}\t${Version}', name],
                                        text=True, timeout=30).strip()
        status, version = value.split('\t', 1)
        if status != 'installed' or not version or '\n' in version:
            raise ValueError('Required build dependency is not installed: ' + name)
        versions[name] = version
    result = {'schema_version': 1, 'packages': versions}
    with output.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, sort_keys=True, indent=2)
        stream.write('\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(capture(args.output), sort_keys=True))


if __name__ == '__main__':
    main()
