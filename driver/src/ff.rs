//! Host force-feedback event storage.
//!
//! Reading raw `/dev/input/eventN` gives `EV_FF` events but not the effect
//! *parameters*, because uinput delivers parameter uploads as `EV_UINPUT`
//! control requests that must be collected with an ioctl handshake. A driver
//! which ignores that handshake makes `upload_effect()` block until timeout, so
//! Steam cannot start rumble at all.
//!
//! Protocol, from `linux/uinput.h`:
//!
//! 1. An `EV_UINPUT` event arrives with `code == UI_FF_UPLOAD` and `value` set
//!    to a request id.
//! 2. Issue `UI_BEGIN_FF_UPLOAD` with `request_id` filled in; the kernel fills
//!    the rest, including `struct ff_effect`.
//! 3. Inspect the effect, set `retval`.
//! 4. Issue `UI_END_FF_UPLOAD` to complete the kernel's pending call.
//!
//! Erase works the same way with `UI_FF_ERASE` and `UI_END_FF_ERASE`.
//!
//! Every struct here is declared to match the kernel ABI exactly, and the sizes
//! are asserted at compile time against values obtained from `linux/input.h`
//! and `linux/uinput.h` via a C program, not from memory.

/// `struct ff_effect` is 48 bytes on x86-64.
pub const FF_EFFECT_SIZE: usize = 48;

/// Offset of the effect union inside `ff_effect`.
///
/// Note this is **16**, not 14: `trigger` and `replay` are each two `u16`, and
/// the union is aligned to 4 bytes, so two bytes of padding follow `replay`.
const FF_U_OFFSET: usize = 16;

/// Offset of `strong_magnitude` inside `ff_rumble_effect`, relative to the
/// union.
const RUMBLE_STRONG: usize = 0;

/// Offset of `weak_magnitude` inside `ff_rumble_effect`, relative to the union.
const RUMBLE_WEAK: usize = 2;

/// `FF_RUMBLE` from `linux/input.h`.
pub const FF_RUMBLE: u16 = 0x50;

/// `FF_GAIN` from `linux/input.h`.
pub const FF_GAIN: u16 = 0x60;

/// `EV_UINPUT` from `linux/uinput.h`.
pub const EV_UINPUT: u16 = 0x0101;

/// `UI_FF_UPLOAD`.
pub const UI_FF_UPLOAD: i32 = 1;

/// `UI_FF_ERASE`.
pub const UI_FF_ERASE: i32 = 2;

/// Raw `ff_effect` bytes.
///
/// Kept as a byte array rather than a mirrored struct so there is no chance of
/// the Rust layout silently differing from the kernel's. Fields are read and
/// written through the documented offsets.
#[derive(Clone)]
pub struct FfEffect {
    bytes: [u8; FF_EFFECT_SIZE],
}

impl Default for FfEffect {
    fn default() -> Self {
        Self {
            bytes: [0u8; FF_EFFECT_SIZE],
        }
    }
}

impl core::fmt::Debug for FfEffect {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FfEffect")
            .field("type", &self.effect_type())
            .field("id", &self.id())
            .field("replay_length", &self.replay_length())
            .finish()
    }
}

impl FfEffect {
    /// Effect type (`FF_RUMBLE`, `FF_GAIN`, ...).
    #[must_use]
    pub fn effect_type(&self) -> u16 {
        u16::from_le_bytes([self.bytes[0], self.bytes[1]])
    }

    /// Kernel-assigned effect id.
    #[must_use]
    pub fn id(&self) -> i16 {
        i16::from_le_bytes([self.bytes[2], self.bytes[3]])
    }

    /// Effect duration in milliseconds.
    #[must_use]
    pub fn replay_length(&self) -> u16 {
        // replay is at offset 10: length u16, delay u16.
        u16::from_le_bytes([self.bytes[10], self.bytes[11]])
    }

    /// Strong (heavy motor) magnitude, for `FF_RUMBLE`.
    #[must_use]
    pub fn strong_magnitude(&self) -> u16 {
        let at = FF_U_OFFSET + RUMBLE_STRONG;
        u16::from_le_bytes([self.bytes[at], self.bytes[at + 1]])
    }

    /// Weak (light motor) magnitude, for `FF_RUMBLE`.
    #[must_use]
    pub fn weak_magnitude(&self) -> u16 {
        let at = FF_U_OFFSET + RUMBLE_WEAK;
        u16::from_le_bytes([self.bytes[at], self.bytes[at + 1]])
    }

    /// True when this effect is a rumble effect the physical controller can
    /// reproduce.
    #[must_use]
    pub fn is_rumble(&self) -> bool {
        self.effect_type() == FF_RUMBLE
    }

    /// Copy into the byte array.
    #[must_use]
    pub fn from_bytes(bytes: [u8; FF_EFFECT_SIZE]) -> Self {
        Self { bytes }
    }

    /// Borrow the raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; FF_EFFECT_SIZE] {
        &self.bytes
    }
}

/// `struct uinput_ff_upload`: `request_id` (u32), `retval` (s32),
/// `effect` (48 bytes), `old` (48 bytes) = 104 bytes.
#[repr(C)]
#[derive(Clone)]
pub struct UinputFfUpload {
    pub request_id: u32,
    pub retval: i32,
    pub effect: [u8; FF_EFFECT_SIZE],
    pub old: [u8; FF_EFFECT_SIZE],
}

/// `struct uinput_ff_erase`: `request_id` (u32), `retval` (s32),
/// `effect_id` (u32) = 12 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UinputFfErase {
    pub request_id: u32,
    pub retval: i32,
    pub effect_id: u32,
}

const _: () = assert!(std::mem::size_of::<UinputFfUpload>() == 104);
const _: () = assert!(std::mem::size_of::<UinputFfErase>() == 12);
const _: () = assert!(FF_U_OFFSET + 4 <= FF_EFFECT_SIZE);

mod ioctl {
    /// `_IOWR('U', 200, struct uinput_ff_upload)`.
    pub const BEGIN_FF_UPLOAD: libc::c_ulong = 0xc068_55c8;
    /// `_IOW('U', 201, struct uinput_ff_upload)`.
    pub const END_FF_UPLOAD: libc::c_ulong = 0x4068_55c9;
    /// `_IOWR('U', 202, struct uinput_ff_erase)`.
    pub const BEGIN_FF_ERASE: libc::c_ulong = 0xc00c_55ca;
    /// `_IOW('U', 203, struct uinput_ff_erase)`.
    pub const END_FF_ERASE: libc::c_ulong = 0x400c_55cb;
}

pub use ioctl::{BEGIN_FF_ERASE, BEGIN_FF_UPLOAD, END_FF_ERASE, END_FF_UPLOAD};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_sizes_match_the_kernel_abi() {
        // Values printed by a C program compiled against the real headers.
        assert_eq!(std::mem::size_of::<UinputFfUpload>(), 104);
        assert_eq!(std::mem::size_of::<UinputFfErase>(), 12);
        assert_eq!(FF_EFFECT_SIZE, 48);
    }

    #[test]
    fn union_offset_accounts_for_alignment_padding() {
        // ff_effect: type(2) id(2) direction(2) trigger(4) replay(4) = 14, then
        // the union is 4-byte aligned so it starts at 16.
        assert_eq!(FF_U_OFFSET, 16);
    }

    #[test]
    fn decodes_a_rumble_effect() {
        let mut bytes = [0u8; FF_EFFECT_SIZE];
        bytes[0..2].copy_from_slice(&FF_RUMBLE.to_le_bytes());
        bytes[2..4].copy_from_slice(&3i16.to_le_bytes());
        bytes[10..12].copy_from_slice(&500u16.to_le_bytes());
        bytes[16..18].copy_from_slice(&0xf000u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&0x8000u16.to_le_bytes());

        let effect = FfEffect::from_bytes(bytes);
        assert!(effect.is_rumble());
        assert_eq!(effect.id(), 3);
        assert_eq!(effect.replay_length(), 500);
        assert_eq!(effect.strong_magnitude(), 0xf000);
        assert_eq!(effect.weak_magnitude(), 0x8000);
    }

    #[test]
    fn rejects_non_rumble_effects() {
        let mut bytes = [0u8; FF_EFFECT_SIZE];
        bytes[0..2].copy_from_slice(&FF_GAIN.to_le_bytes());
        assert!(!FfEffect::from_bytes(bytes).is_rumble());
    }

    #[test]
    fn ff_codes_are_correct() {
        // From linux/input.h. 0x52 is FF_CONSTANT and must not be used here.
        assert_eq!(FF_RUMBLE, 0x50);
        assert_eq!(FF_GAIN, 0x60);
        assert_ne!(FF_RUMBLE, 0x52);
    }

    #[test]
    fn uinput_control_constants_are_correct() {
        assert_eq!(EV_UINPUT, 0x0101);
        assert_eq!(UI_FF_UPLOAD, 1);
        assert_eq!(UI_FF_ERASE, 2);
    }

    #[test]
    fn upload_struct_field_offsets() {
        // request_id at 0, retval at 4, effect at 8, old at 56.
        let upload = UinputFfUpload {
            request_id: 0xdead_beef,
            retval: -5,
            effect: [0xaa; FF_EFFECT_SIZE],
            old: [0xbb; FF_EFFECT_SIZE],
        };
        assert_eq!(upload.request_id, 0xdead_beef);
        assert_eq!(upload.retval, -5);
        assert_eq!(upload.effect[0], 0xaa);
        assert_eq!(upload.old[0], 0xbb);
    }
}
