#!/usr/bin/env python3
"""Read-only, identity-redacted Linux HID inventory; never opens device nodes."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys


DEFAULT_VENDOR = 0xD7D7  # Research lead, not automatic runtime matching policy.
MAX_METADATA = 16384
MAX_DESCRIPTOR = 4096
NODE_NAME = re.compile(r"hidraw[0-9]+\Z")
HID_ID = re.compile(r"([0-9a-fA-F]{4,8}):([0-9a-fA-F]{4,8}):([0-9a-fA-F]{4,8})\Z")


def read_bounded(path, limit):
    with path.open("rb") as source:
        data = source.read(limit + 1)
    if len(data) > limit:
        raise ValueError("metadata exceeds size limit")
    return data


def parse_identity(data):
    """Allowlist only HID_ID; never publish names, serials, addresses or paths."""
    fields = {}
    for line in data.decode("utf-8", errors="strict").splitlines():
        key, separator, value = line.partition("=")
        if separator and key == "HID_ID":
            if key in fields:
                raise ValueError("duplicate HID_ID")
            fields[key] = value
    match = HID_ID.fullmatch(fields.get("HID_ID", ""))
    if not match:
        raise ValueError("missing or malformed HID_ID")
    bus, vendor, product = (int(part, 16) for part in match.groups())
    if bus > 0xFFFF or vendor > 0xFFFF or product > 0xFFFF:
        raise ValueError("HID identifier out of range")
    return bus, vendor, product


def problem(node, field, error):
    # Do not serialize exception messages, which can contain private paths.
    return {"node": node, "field": field, "error": type(error).__name__}


def inventory(root, vendor=DEFAULT_VENDOR, product=None, include_descriptor=False):
    """Inspect sysfs metadata only; supports an artificial root for offline tests."""
    report = {
        "schema_version": 1,
        "scope": "matching HID metadata only; no device node access",
        "filter": {"vendor": f"{vendor:04x}", "product": None if product is None else f"{product:04x}"},
        "identity_fields_omitted": ["HID_NAME", "HID_UNIQ", "HID_PHYS", "sysfs_path"],
        "devices": [],
        "errors": [],
        "complete": True,
        "hardware_support_verified": False,
    }
    try:
        nodes = sorted(root.iterdir(), key=lambda item: item.name)
    except OSError as error:
        report["errors"].append(problem(None, "hidraw_class", error))
        report["complete"] = False
        return report

    for node in nodes:
        if not NODE_NAME.fullmatch(node.name):
            continue
        try:
            identity = parse_identity(read_bounded(node / "device" / "uevent", MAX_METADATA))
        except (OSError, ValueError) as error:
            report["errors"].append(problem(node.name, "identity", error))
            continue
        bus, vid, pid = identity
        if vid != vendor or (product is not None and pid != product):
            continue
        device = {
            "node": f"/dev/{node.name}",
            "bus": f"{bus:04x}",
            "bus_name": {0x03: "USB", 0x05: "Bluetooth"}.get(bus, "other"),
            "vendor": f"{vid:04x}",
            "product": f"{pid:04x}",
            "bluetooth_transport": "unknown (Classic versus BLE needs BlueZ evidence)" if bus == 0x05 else "not established",
            "descriptor": None,
        }
        try:
            descriptor = read_bounded(node / "device" / "report_descriptor", MAX_DESCRIPTOR)
            if not descriptor:
                raise ValueError("empty descriptor")
            device["descriptor"] = {
                "bytes": len(descriptor),
                "sha256": hashlib.sha256(descriptor).hexdigest(),
            }
            if include_descriptor:
                device["descriptor"]["hex"] = descriptor.hex()
        except (OSError, ValueError) as error:
            report["errors"].append(problem(node.name, "report_descriptor", error))
        report["devices"].append(device)
    report["complete"] = not report["errors"]
    return report


def hex_id(value):
    try:
        parsed = int(value, 16)
    except ValueError as error:
        raise argparse.ArgumentTypeError("expected a hexadecimal 16-bit identifier") from error
    if not 0 <= parsed <= 0xFFFF:
        raise argparse.ArgumentTypeError("identifier must be between 0000 and ffff")
    return parsed


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sysfs-root", type=Path, default=Path("/sys/class/hidraw"), help="hidraw class directory (or offline fixture root)")
    parser.add_argument("--vid", type=hex_id, default=DEFAULT_VENDOR, help="hex vendor ID; default d7d7 is a research lead, not a driver match")
    parser.add_argument("--pid", type=hex_id, help="optional exact hex product ID")
    parser.add_argument("--descriptor", action="store_true", help="include matched descriptor bytes; manually review before sharing")
    args = parser.parse_args(argv)
    result = inventory(args.sysfs_root, args.vid, args.pid, args.descriptor)
    json.dump(result, sys.stdout, indent=2)
    print()
    return 0 if result["complete"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
