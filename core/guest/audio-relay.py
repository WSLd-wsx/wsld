"""Per-session Pulse relay running in a dedicated, WSLg-enabled provider."""

import asyncio
import contextlib
import fcntl
import os
from pathlib import Path
import signal
import socket
import sys

UPSTREAM = "/mnt/wslg/PulseServer"
MAX_CONNECTIONS = 16


async def pump(reader, writer):
    while data := await reader.read(16384):
        writer.write(data)
        await writer.drain()
    writer.write_eof()


async def relay(directory):
    if os.getuid() == 0:
        raise RuntimeError("WSX-Service requires a non-root default user")
    connections = set()
    stop = asyncio.Event()

    async def forward(reader, writer):
        if len(connections) >= MAX_CONNECTIONS:
            writer.close()
            return
        task = asyncio.current_task()
        connections.add(task)
        upstream_writer = None
        pumps = []
        try:
            upstream_reader, upstream_writer = await asyncio.wait_for(
                asyncio.open_unix_connection(UPSTREAM), 2
            )
            pumps = [
                asyncio.create_task(pump(reader, upstream_writer)),
                asyncio.create_task(pump(upstream_reader, writer)),
            ]
            await asyncio.gather(*pumps)
        except (OSError, asyncio.TimeoutError) as error:
            print(f"Pulse connection closed: {error}", file=sys.stderr)
        finally:
            for transfer in pumps:
                transfer.cancel()
            await asyncio.gather(*pumps, return_exceptions=True)
            if upstream_writer is not None:
                upstream_writer.close()
            writer.close()
            connections.remove(task)

    async def monitor_lease(lease):
        # EOF before handoff means Core aborted startup. After handoff the
        # Linux session's lease, rather than the Core process, owns the relay.
        loop = asyncio.get_running_loop()
        handoff = loop.create_future()
        fd = sys.stdin.fileno()

        def read_handoff():
            loop.remove_reader(fd)
            try:
                handoff.set_result(os.read(fd, 1))
            except OSError as error:
                handoff.set_exception(error)

        loop.add_reader(fd, read_handoff)
        try:
            accepted = await asyncio.wait_for(handoff, 240) == b"1"
        except (OSError, asyncio.TimeoutError):
            accepted = False
        finally:
            loop.remove_reader(fd)
        if not accepted:
            stop.set()
            return

        deadline = loop.time() + 30
        while not (directory / "attached").exists():
            if loop.time() >= deadline:
                stop.set()
                return
            await asyncio.sleep(0.25)
        while True:
            try:
                fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                await asyncio.sleep(0.25)
            else:
                stop.set()
                return

    loop = asyncio.get_running_loop()
    for sig in (signal.SIGTERM, signal.SIGINT):
        loop.add_signal_handler(sig, stop.set)
    deadline = loop.time() + 12
    while not stop.is_set():
        try:
            _, writer = await asyncio.wait_for(asyncio.open_unix_connection(UPSTREAM), 1)
            writer.close()
            await writer.wait_closed()
            break
        except (OSError, asyncio.TimeoutError):
            if loop.time() >= deadline:
                raise RuntimeError("WSLg audio upstream did not become ready")
            await asyncio.sleep(0.1)
    if stop.is_set():
        return
    directory.mkdir(mode=0o700)
    try:
        (directory / "client.conf").write_text("enable-shm = no\n", encoding="ascii")
        with (directory / "lease").open("xb") as lease, socket.socket(socket.AF_UNIX) as listener:
            path = directory / "pulse"
            listener.bind(str(path))
            path.chmod(0o600)
            async with await asyncio.start_unix_server(forward, sock=listener):
                watcher = asyncio.create_task(monitor_lease(lease))
                try:
                    await stop.wait()
                finally:
                    watcher.cancel()
                    await asyncio.gather(watcher, return_exceptions=True)
                    for connection in list(connections):
                        connection.cancel()
                    await asyncio.gather(*connections, return_exceptions=True)
    finally:
        for name in ("pulse", "client.conf", "lease", "attached"):
            with contextlib.suppress(FileNotFoundError):
                (directory / name).unlink()
        directory.rmdir()


os.umask(0o077)
if os.getuid() == 0:
    sys.exit("The audio provider must use a non-root default user")
asyncio.run(relay(Path(sys.argv[1])))
