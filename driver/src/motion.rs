//! Experimental decoding of the Bluetooth Nordic UART motion-shaped stream.
//!
//! These two values are uncalibrated. Their physical axes, sign and units are
//! not established; they are not yet a native three-axis motion interface.

/// Two packed values from an observed 20-byte motion-shaped notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawMotion {
    pub first: i16,
    pub second: i16,
}

fn signed_12(value: u16) -> i16 {
    ((value << 4) as i16) >> 4
}

/// Decode a candidate data notification, excluding status and other frames.
///
/// The suffix is observed on this controller and matches the saved firmware;
/// another firmware version may use a different layout.
#[must_use]
pub fn parse_notification(data: &[u8]) -> Option<RawMotion> {
    if data.len() != 20 || data[18..20] != [0xfe, 0x00] {
        return None;
    }
    let first = u16::from(data[11]) | (u16::from(data[12] & 0x0f) << 8);
    let second = u16::from(data[12] >> 4) | (u16::from(data[13]) << 4);
    Some(RawMotion {
        first: signed_12(first),
        second: signed_12(second),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(a: u16, b: u16) -> [u8; 20] {
        let mut data = [0; 20];
        data[11] = a as u8;
        data[12] = ((a >> 8) as u8 & 0x0f) | ((b as u8 & 0x0f) << 4);
        data[13] = (b >> 4) as u8;
        data[18] = 0xfe;
        data
    }

    #[test]
    fn decodes_signed_values_and_boundaries() {
        assert_eq!(
            parse_notification(&frame(0x7ff, 0x800)),
            Some(RawMotion {
                first: 2047,
                second: -2048
            })
        );
        assert_eq!(
            parse_notification(&frame(0xfff, 0x001)),
            Some(RawMotion {
                first: -1,
                second: 1
            })
        );
    }

    #[test]
    fn rejects_short_other_and_status_notifications() {
        assert_eq!(parse_notification(&[0; 19]), None);
        assert_eq!(parse_notification(&[0; 20]), None);
        let mut status = frame(0, 0);
        status[..2].copy_from_slice(&[0xac, 0xc0]);
        status[18..].copy_from_slice(&[0xfd, 0x10]);
        assert_eq!(parse_notification(&status), None);
    }
}
