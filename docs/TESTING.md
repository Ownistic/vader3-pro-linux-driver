# Testing and validation

## Current setup results

| Check | Status | Evidence |
| --- | --- | --- |
| Diagnostic, descriptor/input fixture and documentation offline tests | Verified locally | `python3 -m unittest discover -s tests -v`: 19 tests passed on Python 3.14.8 |
| CLI help | Verified locally | `python3 tools/inventory.py --help`: exit 0 |
| Initial local sysfs probe | Verified for tool execution only | `python3 tools/inventory.py --descriptor`: exit 0, `devices=[]`, `errors=[]` before connection; subsequent DInput discovery documented below; no hardware support claim |
| Connected physical metadata | Partially verified | Owner-confirmed Vader as Xbox-compatible `045e:02e0` (306-byte descriptor), then DInput `d7d7:0041` (163-byte descriptor) with HID-over-GATT/vendor objects; [DInput evidence](evidence/bluetooth-dinput-mode.md) |
| Passive DInput inputs | Partial raw observation | Owner-authorized read-only ACL; 14 coordinated 21-byte reports with A/B/C/Z press/release fields; no output writes or Steam experiment; [capture evidence](evidence/passive-dinput-abcz.md) |
| Static diagnostics | Verified for inspected files | Scoped `aft_inspect`: 0 errors/warnings, authoritative reports for all 4 Python files after descriptor-fixture addition |
| CI workflow | Untested remotely | `.github/workflows/check.yml` configured for Python 3.12; not run on GitHub here |
| Bazzite runtime | Untested | No runtime translator/service/udev configuration exists yet |

The diagnostic tests use temporary synthetic sysfs metadata. A separate fixture
tests check both real descriptors' length/hash and the recorded A/B/C/Z sample's
shape and press/release sequence, not a production decoder. Tests cover filtering,
identity redaction, digest/descriptor opt-in, malformed/oversized/missing files,
permission/disconnect races, symlinks, CLI validation/exit codes, and no false
hardware-support success. They do **not** validate controller report decoding,
UHID enumeration, Steam, IMU, rumble, reconnection, or actual Bazzite deployment.

## First Legion Go evidence step

Pair the Vader 3 Pro normally; record mode, firmware, Bazzite version and Steam
build. Run from this checkout in Desktop Mode:

```sh
python3 tools/inventory.py --descriptor
```

Review the JSON manually before sharing. The default vendor is only an upstream
lead; if there are no matches, identify the actual vendor through Bazzite device
diagnostics and supply `--vid`/`--pid`. No matches do not establish unsupported
hardware. Repeat in each relevant mode without flashing/configuration writes.

### Optional private capture, when ready for protocol work

BlueZ device metadata inspection and an owner-authorized, read-only hidraw
capture have now run; **btmon has not**. See the evidence records for exact
boundaries. Keep raw output
outside Git with private permissions; Bluetooth tools can reveal unrelated
device identities. Do not upload their output without manual sanitization.

```sh
umask 077
mkdir -p "$HOME/.local/share/flydigi-research/captures"
bluetoothctl show
bluetoothctl devices Connected
```

Use `bluetoothctl info <controller-address>` privately for the identified target.
Record advertised HID/HOGP UUIDs, but corroborate the actual transport; bus 0005
and an advertised UUID alone may not settle which link is carrying input.
For a bounded connect/disconnect capture, if a suitable privilege policy exists:

```sh
sudo btmon -w "$HOME/.local/share/flydigi-research/captures/session.btsnoop"
```

Stop with Ctrl-C after the single experiment. This is a diagnostic command, not
an instruction to grant a daemon root privileges. Be aware btmon can capture
unrelated adapter traffic. If installed and the exact matched node is readable,
`hid-recorder /dev/hidrawN` can record passive reports; replace N only after
verifying its identity. Keep output private, press each control individually and
in combinations, include neutral/release states, and record actions/timestamps.
Do not send guessed rumble/feature reports or broadly chmod device nodes.

## Product acceptance matrix

Metadata and a small owner-coordinated raw A/B/C/Z input experiment have been
observed. No output or Steam experiments have been performed. These are separate
checks for each mode; evtest or UHID enumeration alone cannot mark them passed.

| PRD requirement | Gaming Mode | Desktop Mode | Evidence needed |
| --- | --- | --- | --- |
| F1 / A1,A3: recognition + standard controls | Untested | Untested | Steam identity, full button/axis/range test |
| F2 / A4: M1–M4 independently assignable | Untested | Untested | Steam editor bindings, singles and simultaneous presses/releases |
| F2 / A5: C/Z independently assignable | Untested | Untested | Distinct editor bindings, no duplicate standard control |
| F3 / A6: native gyro/accelerometer | Untested | Untested | Native sensor configuration, orientation, scale, bias/calibration and aiming |
| F4 / A7: real Bluetooth rumble | Untested | Untested | Steam feedback, transport capture, physical motor observation |
| F5 / A2: no duplicate physical controller | Untested | Untested | Steam enumeration/hidraw access before/after translator |
| A8: per-game configuration | Untested | Untested | Distinct saved mappings observed in two games |
| F5 / A9: reconnect and sleep, no stuck input | Untested | Untested | Repeated held-button disconnect/wake tests without service restart |
| A10: built-in/unrelated controllers unaffected | Untested | Untested | Before/after independent controller test |
| A11: Bazzite permissions/startup/reboot/update/uninstall | Untested | Untested | Actual immutable-host deployment logs and recovery |

Hardware-dependent implementation needs more live inputs/combined presses and
real Steam experiments; temporary read-only node access enabled this first
capture but is not a deployment permission policy. Product criteria remain untested, not
failed. Bluetooth rumble is unresolved, not declared impossible.

## Later automated test ladder

After real protocol evidence exists, add fixtures/tests for every regular and
extra button, simultaneous extras, stick/trigger endpoints and neutral, sensor
signed/endian parsing, gyro orientation/calibration, report ordering, malformed
length/IDs, disconnect neutralization/reconnect, and feedback duration/channel
translation. Label fixtures synthetic versus recorded. Test UHID control replies
and feedback separately from input generation; keep real Steam gates independent.
