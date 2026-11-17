"""Read the existing Guest account and exact session without lifecycle changes."""
import json
import os
from pathlib import Path
import pwd
import stat
import subprocess
import sys
import uuid


def trusted_json(path, limit=65536):
    flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC
    fd = os.open(path, flags)
    with os.fdopen(fd, 'r') as stream:
        st = os.fstat(stream.fileno())
        if not stat.S_ISREG(st.st_mode) or st.st_uid != 0 or st.st_mode & 0o022 or st.st_size > limit:
            raise ValueError('Untrusted Guest descriptor')
        return json.load(stream)


def observe(request):
    result = {'source': 'guest_live_query', 'initialized': False}
    path = Path('/var/lib/wsx/initialized.json')
    try:
        account = trusted_json(path)
        user = pwd.getpwnam(account['user'])
        result.update(initialized=True, user=user.pw_name, uid=user.pw_uid,
                      timezone=account.get('timezone'), guest_interface_abi=account.get('guest_interface_abi'))
    except FileNotFoundError:
        result['initialization'] = 'not_found'
    instance = request.get('instance')
    if not instance:
        result['session'] = {'state': 'not_queried', 'reason': 'No verified live Native Host instance'}
        return result
    instance = uuid.UUID(instance)
    spec = Path('/run/wsx') / (instance.hex + '.json')
    try:
        descriptor = trusted_json(spec)
        if uuid.UUID(descriptor['instance_id']) != instance:
            raise ValueError('Guest descriptor belongs to another instance')
    except FileNotFoundError:
        result['session'] = {'state': 'descriptor_not_found', 'instance': str(instance)}
        return result
    consumer = Path('/usr/lib/wsx/runtime/kwin-opaque/manifest.json')
    if consumer.exists():
        if trusted_json(consumer, 2 * 1024 * 1024)['abi'] != 2:
            raise ValueError('Unsupported Guest KWin ABI')
        prefix = 'wsx-cachyos-plasma-'
    else:
        prefix = 'wsx-omarchy-'
    unit = prefix + instance.hex + '.service'
    query = subprocess.run(['/usr/bin/systemctl', 'show', unit, '--no-pager',
                            '--property=LoadState,ActiveState,SubState,Result,ExecMainStatus'],
                           capture_output=True, text=True, timeout=10)
    if query.returncode:
        raise RuntimeError('Guest session query failed')
    fields = dict(line.split('=', 1) for line in query.stdout.splitlines() if '=' in line)
    if not fields.get('LoadState') or not fields.get('ActiveState'):
        raise ValueError('Incomplete Guest session response')
    result['session'] = dict(instance=str(instance), unit=unit, properties=fields)
    return result


if __name__ == '__main__':
    print(json.dumps(observe(json.load(sys.stdin))))
