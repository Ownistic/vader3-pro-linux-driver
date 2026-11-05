//! Bluetooth Nordic UART rumble frames; see docs/evidence/bluetooth-rumble-solved.md.

pub const BT_FRAME_LEN: usize = 4;
pub const BT_DURATION_MS: u8 = 200;
pub const BT_RESEND_MS: u64 = 100;

#[must_use]
pub fn bt_frame(left: u8, right: u8) -> [u8; BT_FRAME_LEN] {
    [0x11, left.min(250), right.min(250), BT_DURATION_MS]
}

#[must_use]
pub fn bt_stop_frame() -> [u8; BT_FRAME_LEN] {
    bt_frame(0, 0)
}

pub mod bluez {
    pub const NUS_RX_UUID: &str = "6e400002-b5a3-f393-e0a9-e50e24dcca9e";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_frame_clamps_and_stops() {
        assert_eq!(bt_frame(255, 48), [0x11, 250, 48, BT_DURATION_MS]);
        assert_eq!(bt_stop_frame(), [0x11, 0, 0, BT_DURATION_MS]);
        assert!(BT_RESEND_MS < u64::from(BT_DURATION_MS));
    }
}
