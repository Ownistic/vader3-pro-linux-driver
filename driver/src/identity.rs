//! Canonical Bluetooth addresses and deterministic virtual controller identities.

use std::fmt;
use std::str::FromStr;

/// Six address octets in display order. This identifies a device, not a credential.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MacAddress([u8; 6]);

/// The supplied address is not a supported full Bluetooth address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidMacAddress;

impl fmt::Display for InvalidMacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected a nonzero, non-broadcast Bluetooth address in six hex pairs")
    }
}

impl std::error::Error for InvalidMacAddress {}

impl FromStr for MacAddress {
    type Err = InvalidMacAddress;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let input = s.as_bytes();
        if input.len() != 17 {
            return Err(InvalidMacAddress);
        }
        let mut address = [0; 6];
        for (i, octet) in address.iter_mut().enumerate() {
            let start = i * 3;
            if i != 5 && input[start + 2] != b':' {
                return Err(InvalidMacAddress);
            }
            let digit = |b: u8| match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                b'A'..=b'F' => Some(b - b'A' + 10),
                _ => None,
            };
            *octet = digit(input[start]).ok_or(InvalidMacAddress)? * 16
                + digit(input[start + 1]).ok_or(InvalidMacAddress)?;
        }
        if address == [0; 6] || address == [0xff; 6] {
            return Err(InvalidMacAddress);
        }
        Ok(Self(address))
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d, e, g] = self.0;
        write!(f, "{a:02X}:{b:02X}:{c:02X}:{d:02X}:{e:02X}:{g:02X}")
    }
}

impl MacAddress {
    /// Return octets in the same order as the displayed address.
    #[must_use]
    pub const fn bytes(self) -> [u8; 6] {
        self.0
    }

    /// Return the reverse order used by the virtual pairing feature report.
    #[must_use]
    pub fn reversed_pairing_bytes(self) -> [u8; 6] {
        let mut bytes = self.0;
        bytes.reverse();
        bytes
    }

    /// A filesystem-safe representation, without separators.
    #[must_use]
    pub fn compact(self) -> String {
        self.0.iter().map(|byte| format!("{byte:02X}")).collect()
    }

    /// Derive a stable local-unicast virtual identifier from the complete address.
    ///
    /// This truncated digest is not collision-free. The caller must reject
    /// collisions among concurrently discovered, active and retiring devices.
    #[must_use]
    pub fn virtual_address(self) -> Self {
        const DOMAIN: &[u8] = b"vader3p-steam/virtual-edge/bt/v1\0";
        let mut input = Vec::with_capacity(DOMAIN.len() + self.0.len());
        input.extend_from_slice(DOMAIN);
        input.extend_from_slice(&self.0);
        let digest = crate::device::sha256_hex(&input);
        let mut bytes = [0; 6];
        for (i, byte) in bytes.iter_mut().enumerate() {
            // sha256_hex always produces ASCII hexadecimal pairs.
            *byte = u8::from_str_radix(&digest[2 * i..2 * i + 2], 16).expect("SHA-256 hex output");
        }
        bytes[0] = (bytes[0] & 0xfc) | 0x02;
        Self(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, HashSet};

    #[test]
    fn identity_parses_and_formats_full_address() {
        let mac: MacAddress = "aB:01:23:45:67:89".parse().unwrap();
        assert_eq!(mac.to_string(), "AB:01:23:45:67:89");
        assert_eq!(mac.bytes(), [0xab, 1, 0x23, 0x45, 0x67, 0x89]);
        assert_eq!(
            mac.reversed_pairing_bytes(),
            [0x89, 0x67, 0x45, 0x23, 1, 0xab]
        );
        assert_eq!(mac.compact(), "AB0123456789");
        assert_eq!(mac, "AB:01:23:45:67:89".parse().unwrap());
        assert!("01:23:45:67:89:AB".parse::<MacAddress>().is_ok());
    }

    #[test]
    fn identity_rejects_invalid_and_reserved_addresses() {
        for value in [
            "",
            "00:00:00:00:00:00",
            "ff:ff:ff:ff:ff:ff",
            "1:23:45:67:89:AB",
            "01-23-45-67-89-AB",
            "01:23:45:67:89:AG",
            " 01:23:45:67:89:AB",
            "01:23:45:67:89:AB\n",
            "０1:23:45:67:89:AB",
            "01:23:45:67:89:AB:CD",
        ] {
            assert!(value.parse::<MacAddress>().is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn identity_virtual_vectors_are_stable_and_order_independent() {
        // Independently generated using Python hashlib.sha256, not this implementation.
        let vectors = [
            ("12:34:56:78:9A:BC", "D2:00:76:FC:83:78"),
            ("01:23:45:67:89:AB", "DA:21:3B:7F:A1:04"),
            ("DE:AD:BE:EF:00:01", "9E:A0:81:BD:B7:6D"),
        ];
        let mut forward = BTreeMap::new();
        let mut backward = BTreeMap::new();
        for (physical, expected) in vectors {
            let physical: MacAddress = physical.parse().unwrap();
            let virtual_mac = physical.virtual_address();
            assert_eq!(virtual_mac.to_string(), expected);
            assert_eq!(virtual_mac.bytes()[0] & 3, 2);
            forward.insert(physical, virtual_mac);
        }
        for (physical, _) in vectors.into_iter().rev() {
            let physical: MacAddress = physical.parse().unwrap();
            backward.insert(physical, physical.virtual_address());
        }
        assert_eq!(forward, backward);
        assert_eq!(
            forward.values().collect::<HashSet<_>>().len(),
            vectors.len()
        );
    }
}
