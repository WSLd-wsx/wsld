"""Select a completed, versioned kernel release by run and attempt number."""
import re

TAG = re.compile(r'wsx-kernel-(?:rebuild-)?([0-9]+)-([0-9]+)\Z')


def select(releases):
    if not isinstance(releases, list): raise ValueError('Invalid release inventory')
    candidates = []
    for item in releases:
        if not isinstance(item, dict): raise ValueError('Invalid release record')
        match = TAG.fullmatch(item.get('tag_name', '')) if isinstance(item.get('tag_name'), str) else None
        if match and item.get('draft') is False and item.get('prerelease') is False:
            candidates.append(((int(match.group(1)), int(match.group(2))), item))
    if not candidates: raise ValueError('No published kernel rebuild release')
    if len({order for order, _ in candidates}) != len(candidates): raise ValueError('Duplicate kernel rebuild identity')
    order, result = max(candidates, key=lambda entry: entry[0])
    if not isinstance(result.get('assets'), list): raise ValueError('Invalid selected kernel release assets')
    names = [asset.get('name') for asset in result['assets'] if isinstance(asset, dict)]
    archives = [name for name in names if isinstance(name, str) and re.fullmatch(r'modules_[A-Za-z0-9.-]{1,64}\.tar\.gz', name)]
    if not archives: raise ValueError('Selected kernel release contains no module archives')
    for archive in archives:
        version = archive[len('modules_'):-len('.tar.gz')]
        pattern = re.compile(r'modules_' + re.escape(version) + r'-[0-9]+-[0-9]+\.json\Z')
        metadata = [name for name in names if isinstance(name, str) and pattern.fullmatch(name)]
        if metadata != [f'modules_{version}-{order[0]}-{order[1]}.json']:
            raise ValueError('Selected kernel release is missing matching metadata: ' + version)
    return result
