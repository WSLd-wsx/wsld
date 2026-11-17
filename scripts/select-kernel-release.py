#!/usr/bin/env python3
"""Print the latest published kernel release tag from the source repository."""
import json
import subprocess
import sys
from kernel_release_selection import select


def main():
    repository = 'WSLd-wsx/wsld'
    info = json.loads(subprocess.check_output(['gh', 'api', 'repos/' + repository], text=True))
    if info.get('full_name') != repository:
        raise ValueError('Kernel repository identity mismatch')
    releases = []
    for page in range(1, 101):
        batch = json.loads(subprocess.check_output(['gh', 'api', f'repos/{repository}/releases?per_page=100&page={page}'], text=True))
        if not isinstance(batch, list): raise ValueError('Invalid kernel release page')
        releases.extend(batch)
        if len(batch) < 100: break
    else: raise ValueError('Kernel release inventory exceeds page limit')
    print(select(releases)['tag_name'])


if __name__ == '__main__': main()
