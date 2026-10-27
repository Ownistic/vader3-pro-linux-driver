//! Rumble delivery to the physical controller.
//!
//! Two transports, two mechanisms. Choosing the wrong one fails silently,
//! because both "succeed" at the syscall level.
//!
//! # Bluetooth
//!
//! Verified on hardware: a **4-byte** frame written to the Nordic UART RX
//! characteristic (`6e400002`) produces real vibration.
//!
//! ```text
//! 11 <left> <right> <duration_ms>
//! ```
//!
//! `FUN_00010980` in firmware 6.7.5.1 dispatches on the payload length and only
//! accepts `4` or `20`; every other length is silently dropped. For length 4 it
//! calls `FUN_0000b830`, which routes `buf[0] == 0x11` to
//! `FUN_0000b62c(buf[3], buf[1], buf[2])` — that is
//! `rumble(duration_ms, left, right)`, so **byte 3 is the duration** and bytes
//! 1-2 are the levels. Swapping them yields one short buzz instead of a
//! sustained one.
//!
//! There is no start/stop latch: the duration is a hard firmware timeout. To
//! sustain vibration, re-send faster than the duration expires.
//!
//! # USB
//!
//! `05 0F <L> <R>` written to the `0xFFA0` vendor interface (`mi_02`) for
//! 500 ms, then re-sent. Writing to the wrong interface reports success while
//! doing nothing, so the interface must be selected by number.

use anyhow::Result;

/// Bluetooth subcommand byte for rumble.
const BT_SUBCMD_RUMBLE: u8 = 0x11;

/// Length of the Bluetooth rumble frame. The firmware accepts only this length
/// or 20 bytes; anything else is dropped without error.
pub const BT_FRAME_LEN: usize = 4;

/// Duration carried in each Bluetooth frame, in milliseconds.
///
/// Re-send interval must be shorter than this. 200 ms with a 100 ms interval
/// was verified to produce continuous vibration.
pub const BT_DURATION_MS: u8 = 200;

/// Re-send interval for sustained Bluetooth rumble.
pub const BT_RESEND_MS: u64 = 100;

/// Firmware clamps rumble levels at 250, not 255.
const MAX_LEVEL: u8 = 250;

/// Build the 4-byte Bluetooth rumble frame.
#[must_use]
pub fn bt_frame(left: u8, right: u8) -> [u8; BT_FRAME_LEN] {
    [
        BT_SUBCMD_RUMBLE,
        left.min(MAX_LEVEL),
        right.min(MAX_LEVEL),
        BT_DURATION_MS,
    ]
}

/// Build the Bluetooth "stop" frame, which also uses the same timeout path.
#[must_use]
pub fn bt_stop_frame() -> [u8; BT_FRAME_LEN] {
    [BT_SUBCMD_RUMBLE, 0, 0, BT_DURATION_MS]
}

/// Build the USB `05 0F` rumble command for the vendor interface.
///
/// 31 bytes: report ID 5, command 0x0F, two levels, then padding. The report
/// length matches the `0xFFA0` descriptor's output report.
#[must_use]
pub fn usb_frame(left: u8, right: u8) -> Vec<u8> {
    let mut frame = vec![0u8; 31];
    frame[0] = 0x05;
    frame[1] = 0x0f;
    frame[2] = left.min(MAX_LEVEL);
    frame[3] = right.min(MAX_LEVEL);
    frame
}

/// BlueZ objects and a GATT write path for the Nordic UART characteristic.
pub mod bluez {
    use super::Result;
    use anyhow::Context;
    use dbus::arg::PropMap;
    use dbus::blocking::{Connection, Proxy};
    use std::collections::HashMap;
    use std::time::Duration;

    /// Nordic UART RX characteristic UUID.
    pub const NUS_RX_UUID: &str = "6e400002-b5a3-f393-e0a9-e50e24dcca9e";

    /// D-Bus call timeout. GATT writes are quick; a long timeout would block
    /// the main loop on a wedged controller.
    const CALL_TIMEOUT: Duration = Duration::from_millis(500);

    /// A connected handle to the controller's GATT write path.
    ///
    /// The object path is resolved once at connect time rather than searched on
    /// every frame: `WriteValue` runs at 10 Hz during rumble, and interface
    /// enumeration per write would dominate the cost.
    pub struct NusWriter {
        conn: Connection,
        path: dbus::Path<'static>,
    }

    impl NusWriter {
        /// Locate the Nordic UART RX characteristic for a device address.
        ///
        /// `address` is the Bluetooth address as BlueZ spells it, e.g.
        /// `AA:BB:CC:DD:EE:FF`.
        pub fn connect(address: &str) -> Result<Self> {
            let conn = Connection::new_system().context("connecting to the system bus")?;
            let path = Self::find_nus_rx(&conn, address)?;
            Ok(Self { conn, path })
        }

        /// Find the NUS RX characteristic object for one device.
        ///
        /// BlueZ exposes `GetManagedObjects` on the D-Bus root (`/`), not on the
        /// adapter or device path, and the reply needs the single-output-argument
        /// tuple wrapper to deserialize. Both were confirmed against a live
        /// BlueZ instance.
        fn find_nus_rx(conn: &Connection, address: &str) -> Result<dbus::Path<'static>> {
            let proxy = Proxy::new("org.bluez", "/", CALL_TIMEOUT, conn);
            let (objects,): (
                HashMap<dbus::Path<'static>, HashMap<String, PropMap>>,
            ) = proxy
                .method_call(
                    "org.freedesktop.DBus.ObjectManager",
                    "GetManagedObjects",
                    (),
                )
                .map_err(|e: dbus::Error| anyhow::anyhow!("GetManagedObjects failed: {e}"))?;

            let wanted = address.to_uppercase().replace(':', "_");
            for (path, interfaces) in objects {
                let Some(props) = interfaces.get("org.bluez.GattCharacteristic1") else {
                    continue;
                };
                let Some(uuid) = props.get("UUID") else {
                    continue;
                };
                let Some(text) = uuid.0.as_str() else {
                    continue;
                };
                // Match the characteristic *and* the device, because a second
                // Flydigi controller would otherwise be selectable by accident.
                if text == NUS_RX_UUID && path.to_string().contains(&wanted) {
                    return Ok(path);
                }
            }
            anyhow::bail!("NUS RX characteristic not found for device {address}")
        }

        /// Write one rumble frame.
        pub fn write(&self, frame: &[u8]) -> Result<()> {
            let proxy = Proxy::new(
                "org.bluez",
                self.path.clone(),
                CALL_TIMEOUT,
                &self.conn,
            );
            let options: PropMap = HashMap::new();
            proxy
                .method_call("org.bluez.GattCharacteristic1", "WriteValue", (frame, options))
                .map_err(|e: dbus::Error| anyhow::anyhow!("WriteValue failed: {e}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bt_frame_layout_matches_firmware() {
        // 11 <left> <right> <duration>
        assert_eq!(bt_frame(0xfa, 0x30), [0x11, 0xfa, 0x30, BT_DURATION_MS]);
        assert_eq!(BT_FRAME_LEN, 4);
    }

    #[test]
    fn bt_frame_clamps_to_250() {
        // The firmware clamps at 250; sending 255 would be silently reduced, so
        // clamp here to keep host and device in agreement.
        assert_eq!(bt_frame(0xff, 0xff), [0x11, 250, 250, BT_DURATION_MS]);
    }

    #[test]
    fn bt_stop_is_zero_level() {
        assert_eq!(bt_stop_frame(), [0x11, 0, 0, BT_DURATION_MS]);
    }

    #[test]
    fn resend_beats_the_firmware_timeout() {
        // A re-send interval at or above the duration would let the motors stop
        // between frames, producing stutter rather than sustained rumble.
        assert!(
            BT_RESEND_MS < u64::from(BT_DURATION_MS),
            "re-send {BT_RESEND_MS}ms must be shorter than duration {BT_DURATION_MS}ms"
        );
    }

    #[test]
    fn usb_frame_layout_matches_capture() {
        let f = usb_frame(0x70, 0x70);
        assert_eq!(f.len(), 31);
        assert_eq!(&f[..4], &[0x05, 0x0f, 0x70, 0x70]);
        assert!(f[4..].iter().all(|&b| b == 0), "tail must be zero padding");
    }

    #[test]
    fn usb_frame_clamps_to_250() {
        let f = usb_frame(0xff, 0xff);
        assert_eq!(&f[2..4], &[250, 250]);
    }

    #[test]
    fn nus_uuid_is_the_nordic_uart_rx_characteristic() {
        assert!(bluez::NUS_RX_UUID.starts_with("6e400002"));
    }
}
