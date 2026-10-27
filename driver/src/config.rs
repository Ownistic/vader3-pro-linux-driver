//! Configuration.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::device;

/// Runtime configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Vendor ID of the physical controller.
    pub vendor_id: u16,
    /// Product ID of the physical controller.
    pub product_id: u16,
    /// Name presented to the kernel for the virtual pad.
    pub virtual_name: String,
    /// Enable rumble forwarding from host feedback to the controller.
    pub rumble: bool,
    /// Bluetooth address of the controller, needed for GATT writes.
    ///
    /// Required for Bluetooth rumble: the NUS characteristic is reached through
    /// BlueZ by device path, and there is no hidraw route.
    pub bluetooth_address: Option<String>,
    /// Poll interval in milliseconds when idle.
    ///
    /// The controller is edge-triggered, so an idle poll costs one non-blocking
    /// read. 4 ms keeps latency low without busy-waiting.
    pub poll_ms: u64,
    /// Hide the physical controller's evdev nodes so it does not appear as a
    /// duplicate gamepad alongside the virtual one.
    ///
    /// Uses `EVIOCGRAB`, which the kernel releases automatically when this
    /// process exits, so a crash cannot leave the controller unavailable. It
    /// still reads the controller through hidraw, which is a separate interface.
    pub suppress_physical: bool,
    /// Log level: `error`, `warn`, `info`, `debug` or `trace`.
    pub log_level: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vendor_id: device::VENDOR_ID,
            product_id: device::PRODUCT_ID,
            virtual_name: "Flydigi Vader 3 Pro (translated)".to_string(),
            rumble: true,
            bluetooth_address: None,
            suppress_physical: true,
            poll_ms: 4,
            log_level: "info".to_string(),
        }
    }
}

impl Config {
    /// Load from a TOML file, falling back to defaults for missing keys.
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Load the given path if it exists, else return defaults.
    pub fn load_or_default(path: Option<&Path>) -> Result<Self> {
        match path {
            Some(p) if p.exists() => Self::load(p),
            _ => Ok(Self::default()),
        }
    }

    /// Conventional configuration path.
    #[must_use]
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("/etc"));
        base.join("vader3p-steam").join("config.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_verified_identity() {
        let c = Config::default();
        assert_eq!(c.vendor_id, 0xd7d7);
        assert_eq!(c.product_id, 0x0041);
        assert!(c.rumble);
    }

    #[test]
    fn idle_poll_is_not_a_busy_loop() {
        // A zero or sub-millisecond interval would spin a core for nothing;
        // the device sends nothing while untouched.
        assert!(Config::default().poll_ms >= 1);
    }

    #[test]
    fn parses_partial_config() {
        let c: Config = toml::from_str("rumble = false\n").unwrap();
        assert!(!c.rumble);
        // Unspecified keys keep defaults.
        assert_eq!(c.vendor_id, 0xd7d7);
    }

    #[test]
    fn round_trips_through_toml() {
        let original = Config::default();
        let text = toml::to_string(&original).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed.vendor_id, original.vendor_id);
        assert_eq!(parsed.virtual_name, original.virtual_name);
    }
}
