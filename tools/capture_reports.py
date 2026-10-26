#!/usr/bin/env python3
"""Read-only acquisition; requires owner-authorized scoped read access to the node."""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import select
import signal
import socket
import struct
import tempfile
import time
import zlib


SYSFS = Path("/sys/class/hidraw")
HIDIOCGRAWINFO = 0x80084803
NODE_NAME = re.compile(r"hidraw[0-9]+\Z")


def identifier(value):
    try:
        number = int(value, 16 if value.lower().startswith("0x") else 10)
    except ValueError as error:
        raise argparse.ArgumentTypeError("expected a decimal or 0x-prefixed integer") from error
    if not 0 <= number <= 0xFFFF:
        raise argparse.ArgumentTypeError("identifier must be between 0 and 65535")
    return number


def positive_int(value):
    try:
        number = int(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("expected a positive integer") from error
    if number <= 0:
        raise argparse.ArgumentTypeError("value must be positive")
    return number


def positive_seconds(value):
    try:
        number = float(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("expected positive seconds") from error
    if not 0 < number < float("inf"):
        raise argparse.ArgumentTypeError("seconds must be positive and finite")
    return number


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--vid", required=True, type=identifier)
    result.add_argument("--pid", required=True, type=identifier)
    result.add_argument("--node")
    result.add_argument("--max-seconds", type=positive_seconds, default=300)
    result.add_argument("--max-reports", type=positive_int, default=20000)
    result.add_argument("--out")
    result.add_argument("--correlation-id")
    return result


def sysfs_identity(path):
    for line in (path / "device" / "uevent").read_text().splitlines():
        if line.startswith("HID_ID="):
            bus, vendor, product = (int(item, 16) for item in line[7:].split(":"))
            return bus, vendor, product
    raise ValueError("missing HID_ID")


def resolve_node(node, vid, pid):
    if node:
        return node
    matches = []
    for entry in SYSFS.iterdir():
        if NODE_NAME.fullmatch(entry.name):
            try:
                if sysfs_identity(entry) == (0x0005, vid, pid):
                    matches.append(f"/dev/{entry.name}")
            except (OSError, ValueError):
                continue
    if len(matches) != 1:
        raise ValueError(f"expected exactly one Bluetooth match; found {len(matches)}")
    return matches[0]


def raw_identity(fd):
    data = bytearray(struct.pack("=Ihh", 0, 0, 0))
    fcntl.ioctl(fd, HIDIOCGRAWINFO, data, True)
    bus, vendor, product = struct.unpack("=Ihh", data)
    return bus, vendor & 0xFFFF, product & 0xFFFF


def descriptor_digest(node):
    try:
        data = Path("/sys/class/hidraw", Path(node).name, "device", "report_descriptor").read_bytes()
        return hashlib.sha256(data).hexdigest()
    except OSError:
        return None


def capture_fd(fd, output, node, vid, pid, correlation_id, max_seconds, max_reports,
               poller=None, should_stop=lambda: False, reader=os.read):
    start = time.monotonic_ns()
    output.write(json.dumps({
        "kind": "capture-start", "schema_version": 1, "node": node,
        "bus": "0005", "vendor": f"{vid:04x}", "product": f"{pid:04x}",
        "descriptor_sha256": descriptor_digest(node), "correlation_id": correlation_id,
        "actions_confirmed": False, "output_commands_sent": False,
    }) + "\n")
    output.flush()
    poller = poller or select.poll()
    poller.register(fd, select.POLLIN | select.POLLERR | select.POLLHUP | select.POLLNVAL)
    reports, lengths, reason = 0, {}, "end-of-stream"
    while True:
        elapsed = (time.monotonic_ns() - start) / 1_000_000_000
        if should_stop():
            reason = "requested-stop"
            break
        if elapsed >= max_seconds:
            reason = "duration"
            break
        if reports >= max_reports:
            reason = "report-limit"
            break
        try:
            events = poller.poll(max(1, int(min(max_seconds - elapsed, 1) * 1000)))
        except InterruptedError:
            continue
        if not events:
            continue
        flags = events[0][1]
        if flags & (select.POLLERR | select.POLLHUP | select.POLLNVAL):
            reason = "disconnect-or-error"
            break
        try:
            packet = reader(fd, 4096)
        except BlockingIOError:
            continue
        except OSError:
            reason = "disconnect-or-error"
            break
        if not packet:
            reason = "end-of-stream"
            break
        output.write(json.dumps({"kind": "input", "elapsed_ns": time.monotonic_ns() - start,
                                 "hex": packet.hex()}) + "\n")
        output.flush()
        reports += 1
        lengths[str(len(packet))] = lengths.get(str(len(packet)), 0) + 1
    output.write(json.dumps({"kind": "capture-end", "reports": reports,
                             "lengths": lengths, "reason": reason}) + "\n")
    output.flush()


def main(argv=None):
    args = parser().parse_args(argv)
    if args.correlation_id is None:
        host_id = zlib.crc32(socket.gethostname().encode()) % 65536
        raw_correlation = f"{host_id}-{os.getpid()}-{int(time.monotonic())}"
    else:
        raw_correlation = args.correlation_id
    correlation = re.sub(r"[^A-Za-z0-9_-]", "", raw_correlation)[:63] or None
    try:
        node = resolve_node(args.node, args.vid, args.pid)
        fd = os.open(node, os.O_RDONLY | os.O_NONBLOCK | os.O_CLOEXEC)
    except (OSError, ValueError) as error:
        parser().error(str(error))
    try:
        if raw_identity(fd) != (5, args.vid, args.pid):
            parser().error("opened node identity does not match requested Bluetooth VID/PID")
        if args.out:
            saved = args.out
            output = open(saved, "w", encoding="utf-8")
        else:
            directory = Path("/tmp/opencode")
            directory.mkdir(parents=True, exist_ok=True)
            descriptor, saved = tempfile.mkstemp(prefix="hidraw-capture-", suffix=".jsonl", dir=directory)
            output = os.fdopen(descriptor, "w", encoding="utf-8")
        os.chmod(saved, 0o600)
        print(json.dumps({"state": "capturing", "pid": os.getpid(), "saved": saved,
                          "max_seconds": args.max_seconds}), flush=True)
        stop = [False]
        previous = {}
        for signum in (signal.SIGINT, signal.SIGTERM):
            previous[signum] = signal.signal(signum, lambda _sig, _frame: stop.__setitem__(0, True))
        try:
            capture_fd(fd, output, node, args.vid, args.pid, correlation,
                       args.max_seconds, args.max_reports, should_stop=lambda: stop[0])
        finally:
            for signum, handler in previous.items():
                signal.signal(signum, handler)
            output.close()
    finally:
        os.close(fd)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
