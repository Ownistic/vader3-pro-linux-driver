# Device access

The daemon runs as your normal user. It needs read/write access to `/dev/uhid`,
read access to the selected physical controller's hidraw node, and access to its
associated evdev nodes for optional event grabs. The installer does not grant
these permissions. A user service does not grant them either.

Access to `/dev/uhid` lets a user create virtual HID devices. Access to evdev
nodes can expose input events. Only grant these permissions to trusted users and
devices, and follow your system's security policy. Do not make all hidraw or input
nodes world-readable or writable.

## Bazzite

On my Legion Go running Bazzite, I observed this existing rule in
`/usr/lib/udev/rules.d/60-sunshine-ublue.rules`:

```udev
KERNEL=="uhid", TAG+="uaccess"
```

The active local `deck` session had read/write access to `/dev/uhid`. This is
an observation of that installation, not a guarantee for every Bazzite image,
version or session type. The rule tags `/dev/uhid` for active-seat access; the
resulting ACL is managed dynamically, rather than being proof of a manually
granted permanent ACL. I have not verified the access after a reboot or in
Game Mode with this driver. The UHID rule alone does not grant access to the
physical controller's hidraw or evdev nodes.

Check the rule and current access on your own system:

```sh
udevadm info --query=property --name=/dev/uhid
getfacl /dev/uhid
test -r /dev/uhid && test -w /dev/uhid && echo 'UHID access available'
```

The `uaccess` tag alone is not proof that your user has access. The `test`
command produces no success message if either permission is missing. If
`/dev/uhid` does not exist, first check that your kernel provides UHID. Depending
on the distribution, an administrator can load the `uhid` module.

## Other distributions: active-seat access with udev

The following example is for distributions using udev and systemd-logind's
`uaccess` mechanism. It requires an active local seat; it does not grant access
to arbitrary SSH sessions or guarantee access in every Game Mode environment.
Use your distribution's equivalent access controls if `uaccess` is unavailable.

1. Check whether an existing rule already provides the required access. Do not
   add a duplicate rule just because a device has a dynamically assigned ACL.
2. Connect the controller in Bluetooth DInput mode. Identify its hidraw node and
   the related evdev nodes. From a source checkout,
   `python3 tools/inventory.py --descriptor` inventories candidates. Inspect the
   ancestry of each verified node, replacing `N` with its number:

   ```sh
   udevadm info --attribute-walk --name=/dev/hidrawN
   udevadm info --attribute-walk --name=/dev/input/eventN
   ```

3. If access is missing, ask an administrator to review the following example
   and adapt it to the observed device and local policy. Create
   `/etc/udev/rules.d/69-vader3p-steam.rules` only if that path is not already in
   use. Do not overwrite an existing rule file.

   ```udev
   # Virtual HID creation for the active local seat.
   SUBSYSTEM=="misc", KERNEL=="uhid", TAG+="uaccess"

   # Example: Bluetooth HID parent observed as 0005:04B4:2412.*.
   SUBSYSTEM=="hidraw", KERNELS=="0005:04B4:2412.*", TAG+="uaccess"
   SUBSYSTEM=="input", KERNEL=="event*", KERNELS=="0005:04B4:2412.*", TAG+="uaccess"
   ```

   The example matches **every** node under a Bluetooth HID parent with that
   identity, not one Bluetooth address. Verify that all affected devices are
   appropriate for active-seat access. The `uaccess` mechanism can grant
   read/write access, including to evdev nodes; it is not a read-only grant.
   Another supported controller can have a different HID identity, including
   `0005:D7D7:0041.*`; inspect its actual ancestry before adapting the example.
   Descriptor matching can also identify a device with another identity, which
   the example does not cover. A bus-5 identity alone does not prove physical
   Bluetooth transport or add dongle support.

4. Reload the rules and apply the UHID rule to the existing node:

   ```sh
   sudo udevadm control --reload-rules
   sudo udevadm trigger --action=change --subsystem-match=misc --sysname-match=uhid
   ```

5. Reconnect the physical controller so its device nodes receive the new rules.
   Confirm the active local session and inspect the tags and ACLs on the newly
   identified nodes:

   ```sh
   loginctl session-status
   udevadm info --query=property --name=/dev/uhid
   udevadm info --query=property --name=/dev/hidrawN
   udevadm info --query=property --name=/dev/input/eventN
   getfacl /dev/uhid /dev/hidrawN /dev/input/eventN
   test -r /dev/uhid && test -w /dev/uhid && echo 'UHID access available'
   test -r /dev/hidrawN && echo 'Physical HID read access available'
   test -r /dev/input/eventN && echo 'Physical event read access available'
   ```

   Replace `N` with the verified current node numbers. If access is still
   missing, inspect the session, applicable rules and daemon logs before making
   further changes. An ACL or successful device open does not prove controller or
   Steam behavior.

To undo a rule you created, have an administrator inspect and remove only that
file, then reload the udev rules. Existing device ACLs might remain until the
session or device is refreshed; verify access again. Do not remove distribution
rules or unrelated local rules.
