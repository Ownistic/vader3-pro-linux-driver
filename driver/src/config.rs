//! Configuration.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::device;
use crate::identity::MacAddress;

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
    /// Forward host lightbar colors to the physical controller.
    pub led: bool,
    /// Optional Bluetooth address filter; absent means all eligible controllers.
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
            led: true,
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
        if self.virtual_name.is_empty()
            || self.virtual_name.len() > 127
            || self.virtual_name.as_bytes().contains(&0)
        {
            bail!("virtual_name must be 1..=127 bytes without NUL");
        }
        if !(1..=1000).contains(&self.motion.stale_ms) {
            bail!("motion.stale_ms must be in 1..=1000");
        }
        if self.motion.enabled && self.motion.scale == 0 {
            bail!("motion.scale must be nonzero when motion is enabled");
        }
        if let Some(address) = &self.bluetooth_address {
            address
                .parse::<MacAddress>()
                .context("invalid bluetooth_address filter")?;
        }
        Ok(())
    }

    /// Load an explicit path, or the conventional path if present.
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
            Some(p) => Self::load(p),
            None => match std::fs::symlink_metadata(default) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
                _ => Self::load(default),
            },
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
        assert!(c.validate().is_ok());
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
        assert!(c.validate().is_ok());
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
        assert!(c.validate().is_err());
    }

    #[test]
    fn load_rejects_invalid_cross_field_configuration() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "[motion]\nenabled = true\nscale = 0\n").unwrap();
        assert!(Config::load(file.path()).is_err());
    }

    #[test]
    fn auto_filter_name_and_missing_config_validation() {
        let mut cfg = Config::default();
        assert!(cfg.validate().is_ok());
        cfg.motion.enabled = true;
        assert!(cfg.validate().is_ok());
        for value in ["12:34:56:78:9A:BC", "12:34:56:78:9a:bc"] {
            cfg.bluetooth_address = Some(value.into());
            assert!(cfg.validate().is_ok());
        }
        cfg.rumble = false;
        cfg.motion.enabled = false;
        for value in [
            "bad",
            "00:00:00:00:00:00",
            "FF:FF:FF:FF:FF:FF",
            " 12:34:56:78:9A:BC",
        ] {
            cfg.bluetooth_address = Some(value.into());
            assert!(cfg.validate().is_err());
        }
        cfg.bluetooth_address = None;
        for name in [
            String::new(),
            "x".repeat(128),
            "bad\0name".into(),
            "é".repeat(64),
        ] {
            cfg.virtual_name = name;
            assert!(cfg.validate().is_err());
        }
        cfg.virtual_name = "x".repeat(127);
        assert!(cfg.validate().is_ok());
        let temp = tempfile::tempdir().unwrap();
        let absent = temp.path().join("absent");
        assert!(Config::load_resolved(None, &absent)
            .unwrap()
            .validate()
            .is_ok());
        let missing_error = Config::load_or_default(Some(&absent)).unwrap_err();
        assert!(format!("{missing_error:#}").contains(&absent.display().to_string()));
        assert!(Config::load_resolved(Some(temp.path()), &absent).is_err()); // directory, not a file
        std::fs::write(&absent, "[invalid TOML").unwrap();
        assert!(Config::load_resolved(Some(&absent), &absent).is_err());
        assert!(Config::load_resolved(None, &absent).is_err());
        std::fs::write(&absent, "bluetooth_address = '12:34:56:78:9a:bc'\n").unwrap();
        assert!(Config::load_resolved(Some(&absent), &absent)
            .unwrap()
            .validate()
            .is_ok());
        assert!(Config::load_resolved(None, &absent)
            .unwrap()
            .validate()
            .is_ok());
    }

    #[test]
    fn default_config_symlink_must_resolve_and_is_loaded() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let default = temp.path().join("config.toml");
        let target = temp.path().join("target.toml");
        assert!(Config::load_resolved(None, &default)
            .unwrap()
            .bluetooth_address
            .is_none());
        symlink(&target, &default).unwrap();
        let error = Config::load_resolved(None, &default).unwrap_err();
        assert!(format!("{error:#}").contains(&default.display().to_string()));
        assert!(Config::load_resolved(Some(&default), &default).is_err());
        std::fs::write(&target, "bluetooth_address = '12:34:56:78:9A:BC'\n").unwrap();
        assert_eq!(
            Config::load_resolved(None, &default)
                .unwrap()
                .bluetooth_address
                .as_deref(),
            Some("12:34:56:78:9A:BC")
        );
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
