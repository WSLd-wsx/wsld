"""Check the service image and the WSLg audio endpoint as the default user."""

import json
import os
import shutil
import socket
import sys
import time

if sys.version_info < (3, 9) or os.getuid() == 0:
    raise RuntimeError("WSX-Service requires Python 3.9+ and a non-root default user")
if not shutil.which("apt-get") or not shutil.which("dpkg"):
    raise RuntimeError("WSX-Service requires an APT-managed image")

deadline = time.monotonic() + 12
while True:
    try:
        with socket.socket(socket.AF_UNIX) as pulse:
            pulse.settimeout(1)
            pulse.connect("/mnt/wslg/PulseServer")
        break
    except OSError:
        if time.monotonic() >= deadline:
            raise
        time.sleep(0.25)

print(json.dumps({"uid": os.getuid(), "python": sys.version.split()[0], "pulse": True}))
