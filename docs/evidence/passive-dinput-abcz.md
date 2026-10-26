# First coordinated Bluetooth DInput report capture

## Scope and acquisition

This is an input-research checkpoint, **not Phase 01 completion** or a Steam
acceptance test. The owner confirmed the Vader 3 Pro and reconnected it in
Bluetooth DInput (`0005:d7d7:0041`). Firmware remains unknown.

The owner executed a temporary per-node ACL grant for account `work` after the
agent rediscovered the target node. The agent checked readable=true,
writable=false. The ACL is a user-authorized local change, not an installed
udev policy; it must not be treated as reboot/reconnect persistence. Reidentify
the node before any later grant/revocation because node numbers can be reused.

The diagnostic opened only the matched node with `O_RDONLY | O_NONBLOCK |
O_CLOEXEC`, verified bus/vendor/product using read-only `HIDIOCGRAWINFO`
(`0x80084803`), and used `poll`/`read` for bounded input acquisition. It did not
grab the device, issue feature/output reports, subscribe to GATT, change the
controller configuration, or run as root.

1. Five-second untouched request: zero reports. This does not establish a
   permanent report rate or broken connection.
2. First 30-second A/B/C/Z request: 14 reports, but the owner confirmed they
   **missed the window**. Only small axis-byte changes were observed; do not
   label these packets as button evidence.
3. Coordinated recording: up to five minutes, stopped after owner replied
   **done** to requested press/release **A → B → C → Z**, about two seconds
   apart. Four clear press/release pairs occur in that order. Acquisition
   flushed JSONL records as received; SIGTERM requested a graceful stop and
   the final record confirms `requested-stop`, 14 reports, all 21 bytes.

The recorder was a one-off inline Python acquisition script, not a saved runtime
or reusable capture CLI. Capture limits were 300 seconds / 20,000 reports /
4,096 bytes per read, with identity refusal and disconnect/error termination.
The exact acquired bytes/times are retained for independent offline comparison.
Per-packet video/action annotations are absent; button associations rely on the
owner-completed sequence. Repeat/combined-input tests are still required.

## Observed report facts

All offsets count the report ID as byte 0. All 14 coordinated packets have ID
`01` and **21 bytes**, although the descriptor declares 112 payload bits
(14 bytes), i.e. **15 bytes including the ID**. The six trailing bytes are zero
in these samples; their general meaning is unknown. Do not truncate future
captures to the descriptor length or claim these bytes are motion data.

| Requested button | Offset / mask | Evidence in coordinated capture |
| --- | --- | --- |
| A | byte 9 / `10` | set then clear at 17.509–17.660 seconds |
| B | byte 9 / `20` | set then clear at 19.738–19.933 seconds |
| C | byte 11 / `01` | set then clear at 21.778–21.919 seconds |
| Z | byte 11 / `02` | set then clear at 23.908–24.043 seconds |

For these samples C/Z change distinct fields without setting the observed A/B
bits. This is not proof of M1–M4 mapping, independent combinations with all
standard inputs, persistent profiles, or separate Steam Input editor controls.
Minor byte-2/3 changes outside those transitions are recorded, not interpreted
as sensor output.

The pinned `ahungry/vader3` raw-event comments also place C/Z at byte 11 masks
`0x01`/`0x02`; see the [source ledger](../RESEARCH.md). That is corroborating external
commentary, not copied implementation or evidence that its FF claim is universal.

## Retention and checks

- Coordinated private source:
  `/tmp/opencode/flydigi/passive-dinput-coordinated-dqsg8uzu.jsonl`.
- Source size: **1,802 bytes**; SHA-256
  `2ad2b739ef459c92117cc2ea3b50a6710cec5e3bd0b9bec6773761496f6e27fe`.
- Sanitized public fixture:
  `../../tests/fixtures/vader3pro-bluetooth-dinput-abcz.json`, preserving every
  acquired packet and relative monotonic timestamp. No addresses, serials,
  node/host paths, wall-clock times, or unrelated device identities included.
- The source's start record says `actions_confirmed=false` because it was
  emitted before the owner acted. The later owner reply and analyst sequence
  annotation are separate evidence; the raw log was not retroactively rewritten.
- Offline fixture tests check shape, ordering, and observed press/release bits;
  they are not a general parser, UHID test, Steam test, or full device validation.

Next capture: M1/M2/M3/M4 individually, then C+Z and all four rear buttons held
together with releases. Coordinate the window first and preserve all bytes.
Gyro enablement/scale/orientation, rumble commands, virtual identity, suppression,
and Gaming/Desktop Steam acceptance remain unresolved.
