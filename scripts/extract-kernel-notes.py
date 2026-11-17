#!/usr/bin/env python3
"""Extract an image's own ELF notes; this does not prove its provenance."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy

ROOT = Path(__file__).resolve().parents[1]
EXTRACTOR = ROOT / 'core/guest/kernel-exports.py'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel-image', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists() or args.output.is_symlink():
        raise ValueError('Kernel notes output already exists')
    extractor = runpy.run_path(str(EXTRACTOR))
    image = extractor['bounded'](args.kernel_image)
    notes = extractor['embedded_notes'](image)
    with args.output.open('xb') as output:
        output.write(notes)
    print(json.dumps({'kernel_image_sha256': hashlib.sha256(image).hexdigest(),
                      'kernel_build_id': extractor['build_id'](notes),
                      'notes_sha256': hashlib.sha256(notes).hexdigest(),
                      'scope': 'Image-internal notes only; not evidence of a running kernel or publisher provenance'}))


if __name__ == '__main__':
    main()
