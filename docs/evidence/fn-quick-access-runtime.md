# FN quick-access runtime tests

## First candidate: simultaneous chord failed

Baseline: `main` at `a0b40bf` plus the saved FN changes. Independent code preflight
passed 92 Rust tests, 38 Python tests, formatting, strict Clippy, Cargo check and
release build. Driver-input manifest SHA-256:
`cce98d329e110c314ca6547f911921ebefe02101a55a2e43b39cc2264d791952`.

The first candidate encoded PS/Guide and Cross/A simultaneously while FN was
held. Executable SHA-256:
`0a73dcc52b3a03a8f5a98dfb65006bace681a1e97038ddc8060e9bd6ccad2e8d`.
That hash was checked locally, on the remote copy, and through the live
`/proc/92872/exe` after the UI test.

I authorized replacing only the temporary v4 test instance for this test. The
candidate ran as `vader3p-uhid-test-fn.service` from
`/tmp/flydigi-fn-a0b40bf/vader3p-steam-fn`, with a private copy of the existing
configuration. Both configuration files had SHA-256
`e2f46c62484adbdc759d0000fec1637950a7bf292aae0c03b3d65cf6a4a4934c`.
The service was active, received NUS input and logged `NUS state: fn`. The
physical controller remained connected after the first check.

I initially confirmed that FN opened the right-hand sidebar. I then reported
that it also sends an A-button action and launches whichever game is selected.
**The overall UI acceptance result is therefore Fail**, not Pass. Recognition of
the chord does not establish that Steam suppresses its constituent A action.

The failing candidate was stopped. The preserved v4 executable was restarted
as `vader3p-uhid-test-v4.service` and checked active/running (PID 96112 at that
check). Its executable SHA-256 remains
`616d9e1b5f2e808561ea2213271e0c5db6136525cbce031caffd6a501da4c715`.
No installed binary, boot-enabled service or Steam setting was changed. v4 has no
FN shortcut; it is the rollback while the next candidate is prepared.

## Corrective approach

A focused review recommends a bounded sequence: Guide only for about 100 ms,
Guide + A for about 60 ms, then Guide only for about 40 ms before release. These
timings were initially a hypothesis, not an established Steam fix. The bounded
candidate was tested separately below.

Each phase must be emitted rather than skipped after a scheduling delay. A
valid short FN tap should complete one pulse; a long hold must not repeat it.
Stale/missing selected NUS input, disconnect, host STOP/CLOSE and shutdown must
cancel it. Invalid neutral input must not be mistaken for a valid FN release.
Physical Home, A and other controls must retain their independent contributions.

### Preflight repair

Candidate `d1790d39564e232e62c8eb2e4e705668ab46e74e0e61b75d0371f3029f61587e`
passed its implementation tests, but independent preflight found that fabricated
neutral input could arm FN before a real release after startup or reconnect.
It was not deployed. The repair requires an observed valid input state before
HID state caching is considered valid. Added selector-to-report regressions cover
initial attach, reconnect, first-held FN and an actual release before a new press.

## Sequenced candidate: short tap and hold passed

- Baseline: `main` at `a0b40bf`, plus the saved FN changes.
- Release SHA-256:
  `7ec608e76c8c66a8f97a334b88b24372ea1360be1a85038c970ac34f6ba31c56`.
- Driver-input manifest SHA-256:
  `1ee4574e63a3534f9b9c872f476111bbd81b3d7ae80f859a5fb41dac052d45d0`
  over 25 sorted driver files, including the new helper, excluding `target`;
  each manifest entry contains its path, NUL, and raw file SHA-256 digest.
- Independent reverification: 102 Rust and 38 Python tests passed, along with
  formatting, strict Clippy, Cargo check for all targets, release build and diff
  check. A bounded implementation review found no blocker. Rust analyzer was
  unavailable; Cargo supplied compiler validation.

After I confirmed readiness with a game highlighted on Game Mode Home, the
temporary v4 service was stopped and the repaired candidate started as
`vader3p-uhid-test-fn-sequenced.service`, using:

```text
/tmp/flydigi-fn-a0b40bf/vader3p-steam-fn-sequenced
--config /tmp/flydigi-fn-a0b40bf/config.toml
```

The executable hash matched the reviewed build both on upload and through live
`/proc/102713/exe` after the tests. The configuration copy still matched the
preserved original hash recorded above. Startup logs showed NUS input, and the
physical controller remained connected through both tests.

| Check | Result | My observation |
| --- | --- | --- |
| Quick FN tap and release with a game highlighted, then wait one second | Pass | Only the right-hand sidebar opened. The game stayed closed and the main Steam menu did not open. |
| Press and release B to close the sidebar; hold FN alone for about two seconds, then release | Pass | The sidebar opened once and stayed open, with no repeat, game launch or main-menu popup. |

This verifies the requested QAM action for the identified candidate and test
setup. It does not force selection of the Decky tab, establish all firmware or
Steam versions, verify ordinary HID FN on hardware, or close unrelated motion,
rumble, reconnect and latency acceptance gaps. Preserving physical A/Home during
overlap is code-tested; it cannot guarantee a new A edge when physical A is
already held.

The successful service remains active with no runtime cutoff. It is transient,
not boot-enabled: reboot or stopping it ends this instance. The installed binary,
boot configuration and Steam settings were not modified. The original v4 binary
and configuration remain available for rollback; the failing simultaneous
candidate is stopped. See the [phase record](../phases/03-fn-quick-access.md) for the completed
acceptance and Git-packaging record.
