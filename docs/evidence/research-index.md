# Vader 3 Pro static research index

Working coverage ledger, updated 2026-11-04. **Partial** means that at least
one relevant entry point or artifact has been examined, not that all code in
the subsystem has been reviewed. Existing hardware observations are historical;
the current investigation does not operate the controller.

## Artifact inventory

Hashes identify bytes, not authenticity. Vendor artifacts and captures are
private under `local/research/`; do not add them to Git. The installed device's
firmware identity is not independently established by the files below.

| Artifact | Version / SHA-256 | Coverage |
| --- | --- | --- |
| Main MCU image | `6.7.5.1`, `6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a` | Partial: 780-function saved Ghidra export; important paths reviewed, many function roles and indirect targets unknown. |
| Main image ZIP | `6.7.5.1`, `95fb4298f27874305250b8d178e26ce98d0eb93e6a61f215d30f90d6ba8e9266` | Acquired; see [acquisition](vader3pro-firmware-acquisition.md). |
| SI image | `3.0.2`, `e6dc2f68540d3a31b39b6250492702b2566a6a97da7d04fce6895ba040722c14` | REA Binwalk 3.1.0 found no signature regions (`ev_20c9ddeae58a0e7bac22532adcb161756a0d5ba02947d6d791d2f62100570763`); ZIP consistency, bounded cross-image block/XOR relationships and saved Windows SI dispatch checked. Format and CPU remain unknown. |
| Dongle image | `0.4.1.7`, `af74da8b80e60aa21f49d46805f89dff9f2d6834a5d2c063fde0097b240a8a60` | Partial: selected TC32-hypothesis code paths documented in [secondary firmware triage](secondary-firmware-static.md). REA Binwalk returned unvalidated `gpg_signed` candidates (`ev_e616e3983c2d0c5d751fb1745ed6c7b81e53b2311d5909a6f550f810e95c2b70`), not proof of GPG content or authenticity. |
| SI archive | `3.0.2`, `7b76dea5f8a092d063c744ec768f17f0ad748ae4c36aab7f35f128190365d291` | Acquired and extracted. |
| Dongle archives | `f3dongle.zip`: `6f2ced723dd828e87ef1e71aa9a569027d502e5b48172c4f109f327f6f9537e5`; `f3dongle_v0.4.1.7.zip`: `0427a56a81b8f6811a37033491b6a7e3f911aabd899172c184148844256afacb` | Both retained; different hashes do not establish equivalent contents. |
| Space Station installer | `3.4.4.3`, `f6e480b75016288797080ba5257aa8cbc46ebc724401ee841680f8c755ca0d50` | Historical acquisition record; signature unverified. Installer availability in temporary storage is not guaranteed. |
| Space Station managed assemblies | See hashes in [static analysis](space-station-static-analysis.md) | Partial: saved 720-file C# decompilation covers selected services, command builders and update code. The historical `/tmp/opencode/flydigi` extraction was absent on 2026-11-04; no other original assembly location was established. |
| Android base APK | `7.2.10.5`, `0af8e56fd094b94bed1d4340ab02f995cf118b363e6aa321dc601f336d6ba7a6` | Partial: REA summary `ev_ca8b8577c4928550522184cd9d9813b83e5b4685f1950b48525dcbc25b99fd7b` reports 14,574 classes and 3,024 resources. Package signature and native libraries were not verified. |
| APK native `libapp.so` | ARM64 `436a2edfc75f8556b3dfd46c2d5645df7c342ea9e5d6568a62b29e0c4060db13`; x86-64 `4e49431e94a7bbe2652fa5fb40c0211d3f4df975bb4e1905875b401b08319f72` | `file` identifies stripped ELF shared objects. REA/Ghidra 12.1.2 overview of x86-64 found zero defined procedures and 45 defined strings (`ev_2b0876dab40a67f613aa10740e167dc4dbc8718203f14db5b29b08508db1f911`); searches for `flydigi`, `bluetooth`, and `firmware` found none among those defined strings. This does not search undefined or encoded data or prove code absence. Function and protocol relevance remain unknown. REA's separate ELF layout provider was unavailable because `REA_PWNTOOLS_PYTHON` is not configured. |
| Extracted `fztool/classes.dex` | SHA-256 `dfc12dcd41a3900174bf162fed154c234d36fe429e2aae71530ce5fa2fc13d10` | Retained; relation to all five APK DEX files and code coverage not established. |
| Bootloader | No independently identified artifact | Unavailable for verification; updater code is not bootloader code. |
| Descriptors and captures | See [DInput](bluetooth-dinput-mode.md), [Switch](switch-mode.md), [USB](vendor-report-usb.md), and local private captures | Selected descriptors and captures examined. They do not cover all personalities, controls or firmware revisions. |

REA Android inventory used JADX headless `0.7.1`, engine artifact SHA-256
`6e5eacf500b64292bfb73c49797c1958f6ee44646e43e868039ae7feb573ff75`.
Manifest parsing and class counts are static metadata, not runtime coverage.
The REA session bundle is retained privately; its contents are not distributable.

## Subsystem coverage

| Subsystem | Status and evidence | Principal gap / next static step |
| --- | --- | --- |
| Startup, scheduler, power and reset | Partial: [disassembly](vader3pro-firmware-disassembly.md), [command reference](ble-command-reference.md) | Map boot entry and dispatch scheduling; identify reset register effects. |
| USB HID reports and buttons | Partial: [USB motion routing and response arbitration](usb-motion-routing-static.md), [USB vendor report](vendor-report-usb.md), [follow-up](firmware-followup-2026-11-03.md) | Resolve input paths, buffer synchronization and endpoint register semantics; identify the separate `FUN_0001899c` staging transport. Verify physical axes later. |
| Bluetooth DInput | Partial: [RF-shaped ring, attribute dispatch and payload callback](ble-attribute-callback-static.md), [two-word Nordic UART output, selector and outbound queue](static-subsystems-2026-11-04.md#sensor-state-words-to-nordic-uart-notification-shaped-output), [outbound transform and candidate initializer](static-subsystems-2026-11-04.md#optional-outbound-transform-and-candidate-ring-initializer), [registered transform callback and seed initialization](static-subsystems-2026-11-04.md#registered-callback-and-generator-initialization), [other RF-shaped receive queue](staged-frame-transport-static.md), [write paths](bluetooth-write-paths.md), [command reference](ble-command-reference.md) | Identify actual ring loader and descriptor aliases, complete callback/input validation and seed-source identification, inspect remaining helper branches, other selector writers and radio delivery. Negative HID captures do not prove every potential path impossible. |
| Motion / IMU | Partial: [selector and USB serialization](usb-motion-routing-static.md), [two-word Nordic UART output and sensor-map comparison](static-subsystems-2026-11-04.md#sensor-register-map-comparison), [staged-frame handoff](staged-frame-transport-static.md), [gyro investigation](gyro-deferred.md), [Switch](switch-mode.md) | Identify the active sensor and selector-3 path, units, axis signs and remaining sample words; validate delivery and calibration across modes and firmware versions. |
| Analog inputs / calibration | Partial: [analog-input trace](static-subsystems-2026-11-04.md) | Identify six channel sources and mode state from raw instructions and producers. |
| Rumble and feedback | Partial: [Bluetooth](bluetooth-rumble-solved.md), [USB firmware](vader3pro-firmware-rumble.md), [trigger path](vader3pro-firmware-trigger-rumble.md) | Verify additional motor channels and indirect peripheral targets. |
| LEDs and settings | Partial: [LED paths](led-control-paths.md), [follow-up](firmware-followup-2026-11-03.md) | Determine storage address space and complete indirect writes; USB `0x33` Bluetooth reachability is not established. |
| Modes and descriptors | Partial: [Switch](switch-mode.md), [DInput](bluetooth-dinput-mode.md), [Xbox](bluetooth-xbox-mode.md), [toggle correction](fe-report-toggle.md) | Map the chord's configuration byte and distinct `0xEE` flag, selector state, and descriptor construction, including the observed transient digitizer. |
| Configuration, remapping, persistence | Partial: [command reference](ble-command-reference.md), [LED paths](led-control-paths.md) | Resolve all tables, persistence triggers and indirect calls. |
| Update and recovery | Partial: [F3 Windows updater](f3-windows-updater-static.md), [acquisition](vader3pro-firmware-acquisition.md), [bootloader](bootloader-verification.md), [Android OTA](static-subsystems-2026-11-04.md) | Trace device-side update and bootloader if an authorized artifact becomes available. No signature or safe recovery conclusion is proven. |
| Polling and timing | Partial: historical [USB capture](vendor-report-usb.md), [Switch capture](switch-mode.md) | Trace scheduler and clocks. Captured rates are observations, not verified configurable rates. |
| Secondary MCU and dongle | Partial: [secondary firmware triage](secondary-firmware-static.md); selected descriptors, register-fed input, computed endpoint-control reset loop and selected generic writers, cross-model USB-core comparison, queues, selector state and SPI-NOR-shaped storage decoded under a TC32 hypothesis; SI architecture unresolved. | Trace remaining endpoint configuration and max-size writes, actual count bounds and endpoint-6 reachability, saved-selector restoration, callback initialization, integrity and aliases; identify SI updater/container, architecture and interprocessor communication. |
| Native application components | Unresolved: APK native libraries and the Windows `flydigimapclient.dll` / `hidvirtualdriver.sys` boundary | Locate authorized original Windows binaries and trace the native motion interface. Analyze APK native code where relevant; the `libapp.so` layout provider currently lacks configuration. |

## Method limits

The saved Ghidra export is derived from a community TC32 module with local
address-space and `tnegs` corrections; see [disassembly](vader3pro-firmware-disassembly.md).
Literal-pool resolution, calling convention, indirect calls and unrecognized
code may make its call graph incomplete. Recheck important instructions and
constants before using a finding for firmware changes. Existing reports are
historical documents; their correction notices take precedence over conflicting
claims in their bodies. This ledger is not a claim of complete firmware analysis.
