# Steam physical-device visibility — 2026-11-05 UTC

## Result

After a targeted Steam configuration change and restart, the owner reported:
“Only the DualSense Edge Wireless Controller is present now.”

This confirms Steam visibility on the tested system. Buttons, gyro and rumble
were not separately retested after this configuration change. The running
driver binary and Bluetooth connection were not intentionally changed.

Reusable instructions: [Hide the physical controller from Steam](../steam-controller-visibility.md).

## Tested environment and driver

- Legion Go running Bazzite, reached as `deck` through SSH alias `legion`.
- Kernel reported after verification: `7.2.11-ogc4.1.fc44.x86_64`.
- Repository branch: `feat/uhid-emulation`, base HEAD
  `cb7895a2b8b32f8ee7c2851833d7de500e944aff` plus the existing uncommitted UHID
  implementation. No application-code change was needed for this result.
- Running transient unit: `vader3p-uhid-test-v4.service`.
- Main PID before and after Steam restart: `60434`.
- Executable SHA-256 verified after restart:
  `616d9e1b5f2e808561ea2213271e0c5db6136525cbce031caffd6a501da4c715`.
- Physical Bluetooth identity: `04b4:2412`, HID bus `0005`.
- Emulated identity: `054c:0df2`, HID bus `0003`. The USB-looking transport belongs
  to the virtual controller and does not mean the physical controller uses USB.

## Diagnosis

Read-only inspection found `suppress_physical = true` in the running config.
The daemon logged a successful exclusive grab on `/dev/input/event16` and held
that node plus `/dev/hidraw9` open. Steam also held the physical evdev node open.

The existing `EVIOCGRAB` suppression blocks physical evdev event delivery to
other readers. It does not remove the device from enumeration or block hidraw
access. Therefore a successful grab alone does not establish that Steam lists
only one controller.

## Applied change

The owner fully quit Steam and authorized the Steam-only change. A process
check confirmed zero processes named `steam` before editing.

Configuration file:
`/var/home/deck/.local/share/Steam/config/config.vdf`.

Previous file had no `controller_blacklist` entry. Added exactly one entry,
directly inside the root `InstallConfigStore` block:

```vdf
"controller_blacklist"    "04b4/2412"
```

The virtual controller's `054c/0df2` pair was not added. No other Steam setting
was changed by the edit. Existing unrelated controller identities were not
added or removed.

Before replacement, the script created a mode-0600 backup:
`/var/home/deck/.local/share/Steam/config/config.vdf.flydigi-hide-20261105T000425Z.bak`.

The script verified that Steam remained closed and the source file had not
changed concurrently. It used an atomic replacement and verified that removing
the inserted line reproduced the original bytes exactly.

| Artifact at edit time | SHA-256 |
| --- | --- |
| Original config / backup bytes | `9ae82559442c12728058854fbedafba59895ce5ad8c43d96bad3ff84a57feec3` |
| Config immediately after insertion | `ff1fdb7da400f82130f30a1bead2982bde08e7c13c93c5171ba09eefd97abd0b` |

Steam can rewrite its configuration after restarting. These hashes identify the
edit-time versions, not an assertion that the entire file remains unchanged.
Neither the Steam configuration nor its private backup is included in Git.

## Verification

| Criterion | Result | Observation |
| --- | --- | --- |
| Only the physical pair added | Pass | Exact single-entry byte comparison before Steam restart |
| Entry retained after Steam restart | Pass | Read-back found `controller_blacklist = 04b4/2412` |
| Same running driver | Pass | PID `60434`, executable hash unchanged |
| Steam no longer holds physical nodes | Observed in one snapshot | Restarted Steam PID `65317` had no open descriptor matching the current physical hidraw/evdev nodes |
| One Flydigi controller visible | Owner-confirmed pass | Only “DualSense Edge Wireless Controller” present |
| Inputs after this edit | Not separately retested | No input regression claim is made from visibility alone |

## Limits and undo

This is Steam-only hiding, not a driver-level takeover or kernel-device removal.
The setting applies to all devices with the matching VID/PID. It persists after
the daemon stops or crashes. Other Linux applications are not hidden from the
physical device by this setting.

To undo, fully quit Steam, remove only `04b4/2412` from `controller_blacklist`,
preserve any other IDs, save, and restart Steam. Remove the added key if its list
is empty. Restoring the whole backup could discard later unrelated Steam changes.
Do not unbind a kernel HID driver or change Bluetooth pairing for this fix.
