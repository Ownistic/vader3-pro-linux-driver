# Changelog

## 0.2.0 — 2026-11-05

### Added

- Forward DualSense lightbar colors from Steam to the physical controller over
  Bluetooth. I observed the controller's LEDs follow Steam color changes on a
  Legion Go. Color forwarding is experimental and enabled by default. Set
  `led = false` in the configuration to disable it.
- Coalesce color updates per controller and limit color writes to one every
  50 ms. Rumble and color can be updated independently or in the same host
  report.

### Documentation

- Clarify installation, device permissions, compatibility and Bluetooth risks.
- Document color forwarding, its configuration, and an attended test procedure.

### Limitations

- Host color updates can replace the controller's configured lighting effect.
  The daemon cannot restore it on exit. A full power cycle restored the original
  button-pulse effect on my controller; other firmware might behave differently.
- Color forwarding with multiple physical controllers and all brightness values
  has not been verified on hardware. Battery reporting and charging state remain
  unresolved.

## 0.1.0 — 2026-11-05

- Initial published release of the Bluetooth DInput to virtual DualSense Edge
  driver, with automatic controller discovery, rumble, experimental motion,
  and the FN Quick Access shortcut.
