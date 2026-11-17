# SPDX-License-Identifier: MIT
"""Retire a session through the negotiated public Guest protocol only."""

def stop_protocol(instance):
    import json
    request = dict(protocol='wsx-guest', major=1, minor=0, id=instance.hex,
                   operation='session.stop', params=dict(instance=str(instance)))
    # Normal logout is allowed to wait for its safe point; never kill it on a
    # timeout or silently invoke a legacy desktop implementation after failure.
    result = subprocess.run(['/usr/bin/wsx-guest', 'host-call'], input=json.dumps(request),
                            check=True, capture_output=True, text=True)
    if len(result.stdout) > 1024 * 1024:
        raise RuntimeError('Guest retirement response exceeds protocol limit')
    reply = json.loads(result.stdout)
    if (reply.get('protocol') == 'wsx-guest' and reply.get('major') == 1
            and reply.get('id') == instance.hex and reply.get('ok') is False):
        error = reply.get('error', {})
        raise RuntimeError('Guest retirement rejected: ' + json.dumps(
            dict(code=error.get('code'), message=str(error.get('message', ''))[:2048]), ensure_ascii=True))
    if (reply.get('protocol') != 'wsx-guest' or reply.get('major') != 1 or
            reply.get('id') != instance.hex or reply.get('ok') is not True or
            reply.get('result', {}).get('instance') != str(instance) or
            reply['result'].get('session_retired') is not True):
        raise RuntimeError('Guest did not acknowledge normal session retirement')
    print('WSX_GUEST_SESSION_RETIRED instance=' + str(instance), flush=True)
