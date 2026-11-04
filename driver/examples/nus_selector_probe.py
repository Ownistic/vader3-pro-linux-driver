#!/usr/bin/env python3
"""Observe NUS notifications; selector writes require an explicit option."""

import argparse
import collections
import importlib
import sys


def positive_seconds(value):
    try:
        seconds = int(value)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("seconds must be a positive integer") from exc
    if seconds <= 0:
        raise argparse.ArgumentTypeError("seconds must be a positive integer")
    return seconds


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seconds", type=positive_seconds, default=15,
                        help="observation time in seconds (default: 15)")
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--query-selector", action="store_true",
                        help="send BA 40 00 00 after three passive seconds")
    action.add_argument("--enable-stream", action="store_true",
                        help="send AC 01 after three passive seconds (changes selector)")
    parser.add_argument("--device-name", default="Flydigi VADER3P",
                        help="exact BlueZ device name to match (default: Flydigi VADER3P)")
    args = parser.parse_args()
    if (args.query_selector or args.enable_stream) and args.seconds <= 3:
        parser.error("a command requires --seconds greater than 3")

    try:
        import dbus
        import dbus.mainloop.glib
        GLib = importlib.import_module("gi.repository.GLib")
    except ImportError:
        print("Error: Python dbus and gi/GLib are required.", file=sys.stderr)
        return 1

    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    service = "org.bluez"
    device_iface = "org.bluez.Device1"
    service_iface = "org.bluez.GattService1"
    char_iface = "org.bluez.GattCharacteristic1"
    properties_iface = "org.freedesktop.DBus.Properties"
    tx_uuid = "6e400003-b5a3-f393-e0a9-e50e24dcca9e"
    rx_uuid = "6e400002-b5a3-f393-e0a9-e50e24dcca9e"
    started = False
    failed = False
    bus = None
    tx = None
    receiver = None
    lengths = collections.Counter()
    candidates = 0
    selectors = []

    try:
        bus = dbus.SystemBus()
        manager = dbus.Interface(bus.get_object(service, "/"),
                                 "org.freedesktop.DBus.ObjectManager")
        objects = manager.GetManagedObjects()
        eligible = {path for path, interfaces in objects.items()
                     if device_iface in interfaces
                     and str(interfaces[device_iface].get("Name", "")) == args.device_name
                    and interfaces[device_iface].get("Connected", False)
                    and interfaces[device_iface].get("ServicesResolved", False)}
        matches = []
        for device in eligible:
            services = {path for path, interfaces in objects.items()
                        if service_iface in interfaces
                        and interfaces[service_iface].get("Device") == device}
            chars = collections.defaultdict(list)
            for path, interfaces in objects.items():
                props = interfaces.get(char_iface, {})
                if props.get("Service") in services:
                    chars[str(props.get("UUID", "")).lower()].append(path)
            if chars[tx_uuid] and chars[rx_uuid]:
                matches.append((chars[tx_uuid], chars[rx_uuid]))
        if len(matches) != 1 or any(len(paths) != 1 for paths in matches[0]):
            print("Error: expected one connected, resolved NUS device with unique TX and RX characteristics.",
                  file=sys.stderr)
            return 1

        tx_path, rx_path = (paths[0] for paths in matches[0])
        print("Device connected: yes; services resolved: yes")
        tx = dbus.Interface(bus.get_object(service, tx_path), char_iface)
        rx = dbus.Interface(bus.get_object(service, rx_path), char_iface)
        loop = GLib.MainLoop()

        def on_changed(interface, changed, invalidated):
            nonlocal candidates
            if interface != char_iface or "Value" not in changed:
                return
            data = bytes(changed["Value"])
            lengths[len(data)] += 1
            if len(data) == 20:
                if data[:2] == b"\xac\xc0":
                    selectors.append(data[10])
                    print(f"AC C0 status observed; byte 10 selector: {data[10]}", flush=True)
                if data[-2:] == b"\xfe\x00":
                    candidates += 1

        receiver = bus.add_signal_receiver(on_changed, signal_name="PropertiesChanged",
                                           dbus_interface=properties_iface,
                                           bus_name=service, path=tx_path)
        tx.StartNotify()
        started = True
        print("StartNotify accepted (notification delivery not yet verified)", flush=True)

        def query():
            nonlocal failed
            try:
                frame = b"\xac\x01" if args.enable_stream else b"\xba\x40\x00\x00"
                rx.WriteValue(dbus.ByteArray(frame), {})
                label = "AC 01" if args.enable_stream else "Status query"
                print(f"{label} write accepted (device response not verified)", flush=True)
            except dbus.DBusException:
                print("Error: command write failed.", file=sys.stderr)
                failed = True
                loop.quit()
            return False

        if args.query_selector or args.enable_stream:
            GLib.timeout_add(3000, query)
        GLib.timeout_add(args.seconds * 1000, lambda: (loop.quit(), False)[1])
        loop.run()
    except (dbus.DBusException, OSError, ValueError, OverflowError):
        print("Error: BlueZ discovery or notification operation failed.", file=sys.stderr)
        failed = True
    except KeyboardInterrupt:
        print("Observation interrupted.", file=sys.stderr)
        failed = True
    finally:
        if started and tx is not None:
            try:
                tx.StopNotify()
            except dbus.DBusException:
                print("Error: StopNotify failed.", file=sys.stderr)
                failed = True
        if receiver is not None:
            receiver.remove()
        if started:
            print(f"Candidate 20-byte motion-shaped notifications: {candidates}")
            print(f"AC C0 status notifications: {len(selectors)}")
            print("Notification length counts: " +
                  (", ".join(f"{length}: {count}" for length, count in sorted(lengths.items()))
                   or "none"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
