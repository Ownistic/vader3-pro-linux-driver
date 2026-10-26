# Vader 3 Pro firmware acquisition record

Yes — official firmware is downloadable. Space Station fetches it from a public
FlyDigi JSON endpoint rather than bundling it, so the installer contains no
firmware payloads (`innoextract --list` shows only `images.zip` and driver
files). Nothing here was flashed; these are downloaded archives only.

## How the download works

Space Station calls, from `FlydigiSpaceStation.Common.Services.Api.FirmwareService`
(`GetFirmware` → `FirmwareUpgrade.getReqUrl`):

```
http://api.flydigi.com//android/v3/Firmwares/pcNew
  ?type=<firmName>&cpuName=<cpuType>&lang=<lang>
  &appVersion=3.4.4.3&currentVersion=<installed>
```

`type` comes from the device table in `LibCommon.Config.GameHandleListDic`. For
the Vader 3 Pro family the entries are device IDs 80/81/88 with
`FirmwareName = "f3"`, `SIFirmwareVersion = "F3SI"`,
`DONGLEFirmwareName = "F3DONGLE"`. The endpoint needs no authentication and
returns `{"status":0,"message":"OK","data":{...}}` with a `url` to a zip.

`cpuName` accepts `ch573`, `ch571`, or `nordic`; `ch571` returns an empty
version for these firmwares, so the meaningful queries are `ch573`/`nordic`.

## Acquired artifacts

| Target | Version | Archive SHA-256 | Inner binary | Inner SHA-256 | Size |
| --- | --- | --- | --- | --- | --- |
| Vader 3 Pro main MCU (`f3`) | 6.7.5.1 | `95fb4298f27874305250b8d178e26ce98d0eb93e6a61f215d30f90d6ba8e9266` | `Telink87_For_F3_Ver6.7.5.1.bin` | `6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a` | 152068 B |
| Vader 3 Pro SI/ext chip (`F3SI`) | 3.0.2 | `7b76dea5f8a092d063c744ec768f17f0ad748ae4c36aab7f35f128190365d291` | `db_update_GDF-G559301(G_For_F3_V3.0.2)_A2EC_V1.0_230510.bin` | `e6dc2f68540d3a31b39b6250492702b2566a6a97da7d04fce6895ba040722c14` | 296400 B |
| Vader 3 dongle (`F3DONGLE`) | 0.4.1.7 | `6f2ced723dd828e87ef1e71aa9a569027d502e5b48172c4f109f327f6f9537e5` | `F3_Telink80_For_Dongle_Ver0.4.1.7_release_01.bin` | — | 22852 B |

Source URLs, as returned by the API on 2026-10-25 (CDN, HTTPS):

```
https://tencent-android.cdn.flydigi.com/firmware/f3/Telink87_For_F3_Ver6.7.5.1.zip
http://api-web.cdn.flydigi.com/firmwaresnew/2023/05/22/db_update_GDF-G559301(G_For_F3_V3.0.2)_A2EC_V1.0_230510.zip
http://api-web.cdn.flydigi.com/firmwaresnew/2024/06/25/F3_Telink80_For_Dongle_Ver0.4.1.7_release_01.zip
```

Stored outside Git in `/tmp/opencode/flydigi/firmware/`. The API response for
`f3` reports `version 6.7.5.1`, `date 2023-06-13`, description
"1. Switch somatosensory experience fixed", with a later `2024.09.14` stamp.

Reproduce without flashing anything:

```sh
curl -s 'http://api.flydigi.com//android/v3/Firmwares/pcNew?type=f3&cpuName=ch573&lang=en&appVersion=3.4.4.3&currentVersion=0'
curl -sfL -o f3.zip 'https://tencent-android.cdn.flydigi.com/firmware/f3/Telink87_For_F3_Ver6.7.5.1.zip'
sha256sum f3.zip
unzip -l f3.zip
```

## Why this firmware matters for the project

The image contains the controller's **actual HID report descriptors as compiled
into the device firmware**, not as inferred from a host enumeration:

- At `0x23764` there is a 163-byte descriptor that is **byte-for-byte identical**
  to the descriptor this project captured from the live Bluetooth
  `d7d7:0041` device — same bytes, same SHA-256
  `227924e94560bea5149d6a75a8bb225143a1e4b0685075dea729a30649bc1f22`.
  That independently confirms the captured fixture is genuine device firmware
  output rather than a host-side synthesis.
- Adjacent to it at `0x236ec` / `0x2371e` are the GATT/GAP name entries
  `Flydigi VADER3P` (9 bytes, handle context) and `Flydigi VADER3`.
- At `0x23744` is the firmware version string `6.7.5.1`.
- At `0x239d8` the names appear again next to an `OTA` marker and what look like
  OTA partitioning offsets.
- `0x23fdc` holds a 70-byte mouse descriptor; `0x24098` begins an
  Xbox-style descriptor declaring input, output and feature reports (the
  XInput/rumble-capable personality), which is why the same hardware can appear
  as `045e:02e0`.

The BLE-side descriptors are inside this image, so **gyro and vendor-mode
behaviour can be researched statically without touching the device**, which is
what you asked for.

## Descriptor decoded (the DInput personality)

Parsed from the firmware copy; report-relative bits mapped to absolute wire
bytes by adding one for the report ID:

| abs bytes | bits | page | contents |
| --- | --- | --- | --- |
| 1–4 | 32 | Generic Desktop | 4 × 8-bit: X, Y, Z, Rz (the four sticks) |
| 5–8 | 32 | Generic Desktop | 4 × 8-bit: Vx, Vy, Vbrx, Vbry |
| 9 | 4 | Generic Desktop | hat switch (D-pad) |
| 9–10 | 12 | Button | buttons 1–12 |
| 11–12 | 15 | Consumer | 15 consumer-page bits |
| 12 | 1 | Consumer | one further consumer bit |
| 13–14 | 16 | Simulation | 2 × 8-bit: Brake, Accelerator |

Total 112 input bits = 14 bytes + report ID = 15 bytes declared, while the
device actually sends 21. In every one of the seven captures bytes 5–8 (the
`Vx/Vy/Vbrx/Vbry` group) were **always `0x00`**, so those declared axes are
present in the descriptor but unused in this personality. That is consistent
with the observed absence of motion in the passive stream and reinforces that
gyro needs the vendor-mode report, not this one.

## Caveats

- The device's installed firmware version has never been read, so it is unknown
  whether this unit already runs 6.7.5.1. Reading it needs the vendor-mode
  identity query, which has not been run.
- The image is a Telink TC32/TL87-class build: a small header, a large
  near-zero region around `0x4000`, then high-entropy code with a separate data
  region near the end. It is **not** plain ARM/x86, so Ghidra needs a TC32
  loader or manual architecture setup; `binwalk` finds no container signatures.
- Descriptor contents are read directly from the image; no disassembly of the
  protocol handler has been attempted yet. The command semantics recorded in
  `docs/evidence/space-station-static-analysis.md` still come from the Windows
  application, not from this firmware.
- Downloading firmware is not flashing it. Writing this image to the device
  would be an irreversible, high-risk operation and is out of scope here.
