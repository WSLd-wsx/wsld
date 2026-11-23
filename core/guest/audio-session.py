"""Probe or hold the optional relay lease around an existing Guest session."""

import fcntl
import json
import os
from pathlib import Path
import pwd
import socket
import stat
import sys
import time


def probe(directory):
    state = json.loads(Path("/var/lib/wsx/initialized.json").read_text())
    account = pwd.getpwnam(state["user"])
    if not account.pw_uid:
        raise ValueError("Expected an unprivileged desktop account")
    os.setgroups([])
    os.setgid(account.pw_gid)
    os.setuid(account.pw_uid)
    deadline = time.monotonic() + 12
    while True:
        try:
            info = directory.lstat()
            if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or stat.S_IMODE(info.st_mode) != 0o700:
                raise ValueError("Audio relay directory is not private to the desktop UID")
            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(0.25)
                client.connect(str(directory / "pulse"))
            return
        except (FileNotFoundError, ConnectionRefusedError, TimeoutError):
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.1)


def launch(directory, args):
    # The launcher owns this descriptor until it exits. Its systemd session
    # children are managed by the existing Guest lifecycle.
    fd = None
    try:
        fd = os.open(directory / "lease", os.O_RDONLY | os.O_NOFOLLOW)
        fcntl.flock(fd, fcntl.LOCK_SH | fcntl.LOCK_NB)
        os.set_inheritable(fd, True)
        marker = os.open(directory / "attached", os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        os.close(marker)
    except OSError as error:
        if fd is not None:
            os.close(fd)
        print(f"WSX audio lease unavailable: {error}", file=sys.stderr)
    os.execv(args[0], args)


mode, path, *command = sys.argv[1:]
if mode == "probe":
    probe(Path(path))
else:
    launch(Path(path), command)
