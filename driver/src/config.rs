//! Configuration.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
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
    /// Compatibility-only setting; the UHID runtime does not use this value.
    pub poll_ms: u64,
    /// Grab physical evdev nodes to block their event delivery to other readers.
    ///
    /// Uses `EVIOCGRAB`, which the kernel releases automatically when this
    /// process exits, so a crash cannot leave the controller unavailable. It
    /// still reads the controller through hidraw, which is a separate interface.
    /// The grab does not prevent device enumeration or hidraw access.
    pub suppress_physical: bool,
    /// Log level: `error`, `warn`, `info`, `debug` or `trace`.
    pub log_level: String,
    pub pad: PadConfig,
    pub motion: MotionConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    Uhid,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PadConfig {
    pub backend: Backend,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionConfig {
    pub enabled: bool,
    pub scale: i16,
    pub stale_ms: u64,
    pub swap_xy: bool,
    pub invert_x: bool,
    pub invert_y: bool,
}

impl MotionConfig {
    /// Multipliers applied after axis selection, with axis inversion in i32.
    #[must_use]
    pub fn axis_scales(&self) -> [i32; 2] {
        let scale = i32::from(self.scale);
        [
            if self.invert_x { -scale } else { scale },
            if self.invert_y { -scale } else { scale },
        ]
    }
}

impl Default for MotionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            scale: 1,
            stale_ms: 100,
            swap_xy: true,
            invert_x: true,
            invert_y: true,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vendor_id: device::VENDOR_ID,
            product_id: device::PRODUCT_ID,
            virtual_name: "Flydigi Vader 3 Pro (UHID DualSense Edge)".to_string(),
            rumble: true,
            bluetooth_address: None,
            suppress_physical: true,
            poll_ms: 4,
            log_level: "info".to_string(),
            pad: PadConfig::default(),
            motion: MotionConfig::default(),
        }
    }
}

impl Config {
    /// Load from a TOML file, falling back to defaults for missing keys.
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let cfg: Self =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Check cross-field constraints before opening devices or starting workers.
    pub fn validate(&self) -> Result<()> {
        if !(1..=1000).contains(&self.motion.stale_ms) {
            bail!("motion.stale_ms must be in 1..=1000");
        }
        if self.motion.enabled && self.motion.scale == 0 {
            bail!("motion.scale must be nonzero when motion is enabled");
        }
        if self.rumble || self.motion.enabled {
            let valid = self.bluetooth_address.as_deref().is_some_and(|addr| {
                let parts: Vec<_> = addr.split(':').collect();
                parts.len() == 6
                    && parts
                        .iter()
                        .all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))
            });
            if !valid {
                bail!("uhid motion or rumble requires a valid bluetooth_address");
            }
        }
        Ok(())
    }

    /// Load the given path if it exists, else return defaults.
    ///
    /// With no path given, the conventional location is tried before falling back
    /// to defaults. Without this, a config written where the documentation says to
    /// put it would be silently ignored and rumble would quietly stay off.
    pub fn load_or_default(path: Option<&Path>) -> Result<Self> {
        let cfg = Self::load_resolved(path, &Self::default_path())?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn load_resolved(explicit: Option<&Path>, default: &Path) -> Result<Self> {
        match explicit {
            Some(p) if p.exists() => Self::load(p),
            Some(_) => Ok(Self::default()),
            None if default.exists() => Self::load(default),
            None => Ok(Self::default()),
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
        assert_eq!(c.pad.backend, Backend::Uhid);
        assert!(!c.motion.enabled);
        assert_eq!((c.motion.scale, c.motion.stale_ms), (1, 100));
        assert!(c.motion.swap_xy);
        assert!(c.motion.invert_x);
        assert!(c.motion.invert_y);
        assert_eq!(c.motion.axis_scales(), [-1, -1]);
        assert!(c.validate().is_err()); // Rumble requires an address.
    }

    #[test]
    fn backend_and_motion_validation() {
        assert!(toml::from_str::<Config>("[pad]\nbackend = 'unknown'").is_err());
        assert!(toml::from_str::<Config>("[pad]\nbackend = 'uinput'").is_err());
        let compatible: Config = toml::from_str("rumble = false\n[pad]\nbackend = 'uhid'").unwrap();
        assert_eq!(compatible.pad.backend, Backend::Uhid);
        assert!(compatible.validate().is_ok());
        let mut c: Config =
            toml::from_str("rumble = false\npoll_ms = 8\n[motion]\nenabled = true").unwrap();
        assert_eq!(c.poll_ms, 8);
        assert!(!c.rumble);
        assert!(c.motion.swap_xy);
        assert!(c.motion.invert_x);
        let override_cfg: Config = toml::from_str("[motion]\nswap_xy = false").unwrap();
        assert!(!override_cfg.motion.swap_xy);
        assert!(override_cfg.motion.invert_x);
        let mut motion = MotionConfig {
            scale: i16::MIN,
            ..MotionConfig::default()
        };
        assert_eq!(motion.axis_scales(), [32768, 32768]);
        motion.invert_y = false;
        assert_eq!(motion.axis_scales(), [32768, -32768]);
        motion.invert_x = false;
        assert_eq!(motion.axis_scales(), [-32768, -32768]);
        assert!(c.validate().is_err());
        c.bluetooth_address = Some("AA:BB:CC:DD:EE:FF".into());
        assert!(c.validate().is_ok());
        c.motion.scale = 0;
        assert!(c.validate().is_err());
        c.motion.scale = -1;
        for value in [0, 1001] {
            c.motion.stale_ms = value;
            assert!(c.validate().is_err());
        }
        for value in [1, 1000] {
            c.motion.stale_ms = value;
            assert!(c.validate().is_ok());
        }
        c.bluetooth_address = Some("AA:BB:CC:DD:EE:GG".into());
        assert!(c.validate().is_err());
        c.motion.enabled = false;
        c.rumble = false;
        assert!(c.validate().is_ok());
    }

    #[test]
    fn load_rejects_invalid_cross_field_configuration() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "[motion]\nenabled = true\n").unwrap();
        assert!(Config::load(file.path()).is_err());
    }

    #[test]
    fn load_accepts_uhid_and_rejects_removed_backend() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "rumble = false\n[pad]\nbackend = 'uhid'\n").unwrap();
        assert_eq!(
            Config::load(file.path()).unwrap().pad.backend,
            Backend::Uhid
        );
        std::fs::write(file.path(), "rumble = false\n[pad]\nbackend = 'uinput'\n").unwrap();
        assert!(Config::load(file.path()).is_err());
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
    fn default_path_is_loaded_when_no_path_is_given() {
        // Regression: the conventional config path was computed but never read, so
        // following the README produced a working pad with rumble silently off.
        let dir = std::env::temp_dir().join(format!("vader3p-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "log_level = \"debug\"\nbluetooth_address = \"AA:BB:CC:DD:EE:FF\"\n",
        )
        .unwrap();

        let loaded = Config::load_resolved(None, &path).unwrap();
        assert_eq!(loaded.log_level, "debug");
        assert_eq!(
            loaded.bluetooth_address.as_deref(),
            Some("AA:BB:CC:DD:EE:FF")
        );

        std::fs::remove_file(&path).unwrap();
        let missing = Config::load_resolved(None, &path).unwrap();
        assert_eq!(missing.log_level, Config::default().log_level);
        std::fs::remove_dir_all(&dir).unwrap();
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
