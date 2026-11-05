# Hide the physical controller from Steam

The driver grabs the physical controller's evdev event node when
`suppress_physical = true`. This blocks event delivery through that node, but it
does not remove the device from Linux enumeration or block access to its hidraw
node. Steam can therefore list both the physical controller and the emulated
controller.

On the tested system, Steam listed a virtual USB-looking **DualSense Edge
Wireless Controller** and the physical Bluetooth Flydigi controller. The
physical controller reported `HID_ID=0005:000004B4:00002412`; the emulated
controller reported `HID_ID=0003:0000054C:00000DF2`. Adding only the physical
vendor/product pair to Steam's controller blacklist left the emulated controller
visible. The owner confirmed that Steam then listed only the DualSense Edge
Wireless Controller. This is a Steam-only visibility change, not kernel-level
hiding.

## Before you begin

- Keep the driver running with `suppress_physical = true`.
- Identify the physical controller's vendor ID (VID) and product ID (PID) on your
  own system **before quitting Steam**. In **Settings → Controller**, select the
  physical controller and open **Test Device Inputs** to read its IDs.
  Alternatively, inspect the Linux HID device information. Device IDs can differ
  between models or revisions. Do not use the emulated controller's IDs.
- Close games and fully quit Steam. This ends any running Steam game.
- Use the configuration file for the Steam installation you are changing.

Common configuration paths are:

- Regular Steam: `~/.local/share/Steam/config/config.vdf`. The path
  `~/.steam/steam/config/config.vdf` often points to the same installation.
- Flatpak Steam:
  `~/.var/app/com.valvesoftware.Steam/.local/share/Steam/config/config.vdf`.

If you have more than one Steam installation, edit the file for the instance you
use. Do not share the file or its backup; it contains private account settings.

## Add the physical device to Steam's blacklist

1. Fully quit Steam, including its tray process. Confirm Steam is no longer
   running before editing the file.
2. Open the applicable `config.vdf` and save a private backup. Keep the backup
   outside the repository.
3. Confirm the physical VID/PID recorded before quitting Steam. Keep Steam closed
   until you finish editing. If you still need the IDs, inspect the Linux device
   information rather than reopening Steam during the edit.
4. In `config.vdf`, find the root `InstallConfigStore` block. If it already has a
   `controller_blacklist` entry, append the physical pair to its existing
   comma-separated IDs. Preserve every existing ID. Do not add a duplicate key.
5. If there is no `controller_blacklist` entry, add one directly inside the
   `InstallConfigStore` braces, outside the `Software` block. Replace the example
   IDs below with your physical device's VID and PID, in lowercase hexadecimal
   without a `0x` prefix:

   ```vdf
   "InstallConfigStore"
   {
       "controller_blacklist"    "04b4/2412"

       "Software"
       {
           ...
       }
   }
   ```

   This is only a small example of the relevant structure. Do not replace your
   full configuration with this snippet. If an entry already exists, for
   example, `045e/028e`, keep it and add a comma followed by the physical pair:
   `045e/028e,04b4/2412`.
6. Save the file, then start Steam.
7. Open Steam's controller settings and confirm that the physical Flydigi entry
   is gone and the emulated **DualSense Edge Wireless Controller** remains. Use
   Steam's input test to check the emulated controller. The visibility change
   alone does not verify its buttons, gyro, or other input behavior.

The tested physical pair was `04b4/2412`. Do **not** blacklist `054c/0df2` or
all devices from vendor `054c`: `054c/0df2` was the emulated controller's ID on
the tested system. Confirm your own physical VID/PID before applying this
workaround. Steam's VID/PID entry affects every matching device, not one
individual controller.

## Undo the change

The blacklist is a persistent Steam setting that is independent of the driver.
The physical controller remains hidden from Steam even after the driver stops or
crashes.

To restore the physical controller in Steam:

1. Fully quit Steam.
2. Remove only the physical VID/PID pair from `controller_blacklist`, preserving
   any other IDs. If the list becomes empty, remove the added key. Do not restore
   the whole backup unless you intend to discard any Steam settings changed since
   the backup was made.
3. Save the file and restart Steam.

## Troubleshooting

- **Steam still lists both devices:** Fully quit Steam again. Check that you
  edited the configuration file for the Steam installation you use, and verify
  the physical device's VID/PID. The blacklist uses the form `vid/pid`, such as
  `04b4/2412`, without `0x` prefixes.
- **The emulated controller disappears:** Check that the blacklist contains
  only the physical controller's pair, not the emulated controller's pair, and
  confirm that the driver is running. Do not add more IDs or unbind a kernel HID
  driver to hide the physical entry.
- **Native applications no longer receive physical input:** The Steam blacklist
  does not hide devices from other Linux applications. The driver's evdev grab
  blocks their physical evdev events while the driver runs. Use the emulated
  controller, or stop the driver for native physical input. Also remove the
  blacklist entry if Steam needs to use the physical controller directly.
