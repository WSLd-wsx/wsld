# SPDX-License-Identifier: MIT
"""Use the verified rootfs Plasma logout before exact-device retirement.

Injected into the pinned Native Guest helper by prepare.py. The rootfs adapter
owns KWin-before-input/seat ordering, as verified by the CachyOS r2 trial.
"""


def stop_plasma(instance):
    unit = 'wsx-cachyos-plasma-' + instance.hex + '.service'
    state = subprocess.run(['/usr/bin/systemctl', 'show', '--property=LoadState',
                            '--value', unit], check=True, text=True, capture_output=True)
    if state.stdout.strip() == 'not-found':
        return False
    if state.stdout.strip() != 'loaded':
        raise RuntimeError('Plasma session unit is unresolved during retirement')
    # Guest validates the root-owned instance descriptor and requests normal
    # Plasma logout, then waits for the compositor, input and seat in order.
    subprocess.run(['/usr/bin/wsx-guest', 'stop', '--instance', str(instance)], check=True)
    state = subprocess.run(['/usr/bin/systemctl', 'show', unit,
                            '--property=ActiveState', '--value'],
                           check=True, text=True, capture_output=True)
    if state.stdout.strip() != 'inactive':
        raise RuntimeError('Plasma session did not stop normally; device retained')
    if (Path('/run/wsx') / (instance.hex + '.owner.json')).exists() or Path('/run/seatd.sock').exists():
        raise RuntimeError('Plasma input/seat ownership remains; device retained')
    marker = ('WSX_DESKTOP_INSTANCE=' + str(instance)).encode()
    for process in Path('/proc').glob('[0-9]*'):
        try:
            if marker in (process / 'environ').read_bytes().split(b'\0'):
                raise RuntimeError('Plasma instance process remains; device retained')
        except (FileNotFoundError, ProcessLookupError):
            continue
    print('WSX_PLASMA_SESSION_RETIRED instance=' + str(instance), flush=True)
    return True
