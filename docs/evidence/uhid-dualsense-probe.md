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

No probe or hardware test was run as part of this implementation. Kernel
binding, Edge support on the installed kernel, Steam recognition, sensor
delivery, and any effect of fabricated gyro remain unverified. No real input,
button mapping, paddles, rumble, lightbar, USB transport, Bluetooth, or
production daemon changes are implemented. This identity may collide with a
real device or another instance; the synthetic MAC is not unique per process.
