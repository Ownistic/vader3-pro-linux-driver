# F3 Windows updater: static analysis

Source: saved Space Station `3.4.4.3` decompilation of
`FlydigiSpaceStation.dll`, SHA-256
`a72e3cc035f0660e60d45c5d8e5ddbb1ef44077ee50c47321f43fe32d14d636d`.
No updater was run and no device was contacted. `GameHandleListDic` identifies
the F3 Pro family as `f3p`, with firmware name `f3`; the application contains
`FmUpgradeSwViewF3Model` and a corresponding F3-specific view.
`MainWindowViewModel.ShowForceFMUpgrade` routes devices satisfying
`GamePadVersionUtils.IsF3Serial` to `ShowF3ForceFMUpgrade`, which constructs
this model. The saved `F3Serial` list includes device IDs 28, 80, 81 and 88;
the device table identifies IDs 80, 81 and 88 as F3 Pro variants. This traces
the forced-upgrade UI route, not necessarily every manual upgrade route or
every type of F3-family image.

## Image handling

`ExecDownloadFile` downloads a ZIP, extracts it, and selects the first `.bin`
returned by directory enumeration. It reads the proposed image length as a
little-endian 32-bit value at offset `0x18`, reads a 32-bit `bin_crc` from
`length - 4`, copies that many image bytes, and appends 16 bytes of `0xff` to
the staging buffer. It logs a version word read at offset 2.
`check_bin_is_ok` rejects `bin_crc` equal to zero or `0xffffffff`; the examined
method does not recalculate or compare that image value. This says nothing
about checks performed elsewhere or by the device.

Applied as a read-only metadata check to the retained F3 main image
`6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`,
the length word is 152,068, equal to the file size. The updater-selected
tail word is `0xd26ba3af`, and the offset-2 version word is `0x01020306`.
These values match the client's field locations; they do not validate the
tail word as a correct checksum or show that this file was installed.
As a separate calculation, Python `zlib.crc32` of the bytes preceding that word
is `0x2d945c50`, whose bitwise complement is the stored word. The CRC over the
entire file is `0xffffffff`. This arithmetic relationship is an observation
for these bytes, not evidence that the bootloader uses or trusts this check.

## Device discovery and transfer

`USB_SearchDevice` searches HID devices for VID:PID `04b4:2412` and a
usage page. The model constructor selects `ffef` by default and `ffee` when
`FirmwareModel.DeviceType` is `Dongle`. It creates an asynchronous read/write stream.
That VID:PID alone does not distinguish a bootloader from another personality,
nor prove that a particular device accepts this updater's frames.

`ota_write` constructs a start frame, then groups up to three indexed chunks
per outgoing report. Each chunk consists of a two-byte little-endian index,
16 image bytes and a two-byte little-endian CRC. `FirmwareUpgradeUtils.crc16`
uses initial value `0xffff`, a reflected `0xa001` polynomial and no final XOR
in the inspected implementation. This is a per-chunk checksum over the index
and 16 payload bytes, not proof of the complete image verification policy.
After the data loop, the client constructs a terminal frame containing an
index and its two's-complement value. `user_sendDataToDevice` uses a HID
stream write in this flow. These observations describe the host, not the
receiver's implementation or flash behavior.

`Read_File_Data_Process_YKQ` recognizes an error-shaped reply and stops
advancing when its status byte is nonzero. Other reads can advance the transfer
when `start_ota_flag` is nonzero. Notably, the end-of-transfer branch sets its
success UI state **before** the subsequent terminal-frame write in
`ota_write`. A success display is therefore not evidence that the controller
accepted the terminal frame, booted the new image, or authenticated it.

## Remaining work

- Trace the remaining manual update routes and identify the actual enumerated interface.
- Trace the device-side receiver, acknowledgments, terminal command and image
  verification. The bootloader has not been independently identified.
- Determine whether the apparent image CRC is validated by another routine or
  by the controller, and whether other authentication data is present.
- Establish recovery separately. No modified-image or other hardware test is
  authorized by this static investigation.
