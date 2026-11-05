# Synthetic DualSense Edge UHID spike

This is a disposable experiment, not production integration. It presents the
Sony USB VID and DualSense Edge PID to trigger existing host identification;
it is not a Sony controller. Its HID descriptor is a small independently
authored descriptor, not a dump of a real device. No physical controller is
read or written. Its sensor samples and calibration are fabricated.

## Protocol basis and assumptions

- Linux [UHID API](https://docs.kernel.org/hid/uhid.html) and
  [`include/uapi/linux/uhid.h`](https://github.com/torvalds/linux/blob/master/include/uapi/linux/uhid.h)
  define the native-endian packed event ABI and reply IDs. Short events are
  zero-extended. Feature and input payloads instead use little-endian values.
- Linux [`hid-ids.h`](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-ids.h)
  identifies USB vendor `054c`, Edge product `0df2`. Linux
  [`hid-playstation.c`](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-playstation.c)
  documents the report IDs, lengths, motion and control offsets, calibration
  interpretation, inactive touch markers, and three sensor-clock ticks per
  microsecond. These are public interface facts, not copied implementation.
- [USB HID 1.11](https://www.usb.org/document-library/device-class-definition-hid-111)
  defines descriptor items. This descriptor advertises six axes and opaque
  vendor input bytes plus the required numbered feature/output reports. It
  does not reproduce a real Edge descriptor, so generic HID consumers or
  Steam may reject it or expose different capabilities.
- GET feature 05 returns invented nonzero symmetric calibration, 09 returns
  a fixed locally administered virtual address, and 20 returns zero version
  information. Other GETs and all SETs receive an explicit error reply with
  the request ID. Output is logged, not acted on. One instance at a time is
  assumed because the virtual pairing address is fixed.
- Input report 01 is 64 bytes at a nominal 250 Hz, with centered sticks,
  released controls, inactive contacts, +1g on Z and zero angular velocity.
  The sensor timestamp follows elapsed monotonic time and wraps at 32 bits.
  `--synthetic-gyro` alternates a fabricated X angular velocity, not measured
  motion. The calibration and axes are only a host compatibility hypothesis.

## Owner-run gating experiment

Do not run concurrently with another copy. From the repository root, after
the owner authorizes the UI test:

```sh
cargo run --manifest-path driver/Cargo.toml --example uhid_dualsense_probe -- --synthetic-gyro
```

It runs for at most 600 seconds; Ctrl-C or SIGTERM tears it down. Use
`--seconds N` to choose 1–86400 seconds and `--help` for usage. Record the
kernel version, logs, and whether Steam actually recognizes the virtual pad
and offers gyro. A successful CREATE2/write or START is not proof of Steam
recognition or usable gyro. If device-node permissions prevent the experiment,
ask the owner to grant access; do not run sudo or change ACLs automatically.

## Status and limitations

### First host run — 2026-11-04

Tested revision: `ab522e8` on `feat/uhid-emulation`, clean tree at launch.
Artifact: `driver/target/debug/examples/uhid_dualsense_probe`, built by:

```sh
cargo run --manifest-path driver/Cargo.toml --example uhid_dualsense_probe -- --synthetic-gyro --seconds 1200
```

`cargo test --example uhid_dualsense_probe`: 3 passed. The existing driver
baseline `cargo test` passed 109 tests before the spike was added. Kernel:
`7.3.1-1-cachyos`.

A read-only sysfs inspection while the probe was running observed:

```text
HID: 0003:054C:0DF2.002A
driver: playstation
hidraw: /dev/hidraw10 readable: True writable: True
input60 Virtual DualSense Edge probe Touchpad
input59 Virtual DualSense Edge probe Motion Sensors
input61 Virtual DualSense Edge probe Headset Jack
input58 Virtual DualSense Edge probe
```

This establishes host kernel binding and node creation, not Steam support or
physical sensor delivery. Owner-confirmed Steam recognition and gyro UI are
pending. The process has a 1,200-second lifetime; it must be running during
the UI observation. No physical controller command was sent.

Steam recognition, sensor delivery, and any effect of fabricated gyro remain
unverified. No real input,
button mapping, paddles, rumble, lightbar, USB transport, Bluetooth, or
production daemon changes are implemented. This identity may collide with a
real device or another instance; the synthetic MAC is not unique per process.

### Legion Go / Bazzite host run — 2026-11-04

The owner clarified that Steam is installed on the Legion Go, not the initial
development host. The local probe exited normally after its bounded lifetime.
The remote host is accessed as `ssh legion`, account `deck`, kernel
`7.2.11-ogc4.1.fc44.x86_64`, x86-64, glibc 2.43. The account already had
read/write access to `/dev/uhid`; no permissions or installed services changed.

Copied the standalone binary, its source and this note to
`/tmp/flydigi-uhid-spike-ab522e8/`. The binary and source match revision
`ab522e8`; only evidence/progress documents changed locally after that commit.
SHA-256 verified on both hosts:

```text
a7be75374361ca87036bc1cfa2552f15abcbac81135433bc145bbbe4ad856fb7  uhid_dualsense_probe
a923e3cf4ba0316b96713ac98dace062afea3313ae74e50eeebffcc6cfe7cf9a  uhid_dualsense_probe.rs
```

Remote command (through SSH):

```sh
/tmp/flydigi-uhid-spike-ab522e8/uhid_dualsense_probe --synthetic-gyro --seconds 1200 > /tmp/flydigi-uhid-spike-ab522e8/run.log 2>&1
```

Read-only remote inspection observed HID `0003:054C:0DF2.0014` bound to
`playstation`, readable/writable `/dev/hidraw9`, gamepad `input86`, Motion
Sensors `input87`, Touchpad `input88`, and Headset Jack `input89`.
The log records `UHID START flags=0x7`, `UHID OPEN`, answered GET feature
requests for 09/20/05, and host output reports of 63 and 48 bytes. These are
observed host interactions, not proof that Steam exposes gyro or that any
physical motor moved. No physical device is read or written by the probe.
The owner then confirmed **"Recognized; gyro available"** in response to the
Legion Go Steam UI check: Steam recognizes the emulated supported controller
and exposes gyro configuration. This passes the spike gate, not physical
motion/rumble/button validation. The synthetic probe was stopped with SIGTERM;
its SSH task exited 0 and a subsequent sysfs inspection found no Edge probe.

Next gate: AC persistence across a physical power cycle. A remote inspection
found no `d7d7:0041` HID node; physical Bluetooth DInput connection on the
Legion Go must be established with the owner before continuing.
