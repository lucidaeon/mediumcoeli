//! Chart UID generation. `derive_uid` is deterministic (stable across
//! repeated exports of the same chart); `random_uid` is a fresh UUID v4.

use std::hash::{Hash, Hasher};

/// Inputs to the deterministic [`derive_uid`] hash. Domain-neutral so any
/// consumer can populate it without depending on this crate's chart types.
#[derive(Debug, Clone, Copy)]
pub struct UidSeed<'a> {
    /// Display name.
    pub name: &'a str,
    /// Year (signed; negative = BCE), i16 to match the canonical chart year width.
    pub year: i16,
    /// Month, 1–12.
    pub month: u8,
    /// Day, 1–31.
    pub day: u8,
    /// Hour, 0–23.
    pub hour: u8,
    /// Minute, 0–59.
    pub minute: u8,
    /// Second, 0–59.
    pub second: u8,
    /// Latitude, decimal degrees (ISO 6709, North positive).
    pub latitude: f64,
    /// Longitude, decimal degrees (ISO 6709, East positive).
    pub longitude: f64,
    /// Timezone offset, decimal hours.
    pub tz_offset_hours: f64,
    /// Optional secondary/alias name.
    pub secondary_name: Option<&'a str>,
}

/// A fresh random UUID v4 string.
#[must_use]
pub fn random_uid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// A deterministic UUID-shaped identifier derived from birth data. Stable
/// across repeated exports of the same chart. Not RFC 4122 compliant, but
/// guaranteed to look like a UUID and to differ when any seed field differs.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn derive_uid(seed: &UidSeed) -> String {
    use std::collections::hash_map::DefaultHasher;
    let mut h1 = DefaultHasher::new();
    seed.name.hash(&mut h1);
    seed.year.hash(&mut h1);
    seed.month.hash(&mut h1);
    seed.day.hash(&mut h1);
    seed.hour.hash(&mut h1);
    seed.minute.hash(&mut h1);
    seed.second.hash(&mut h1);
    seed.latitude.to_bits().hash(&mut h1);
    seed.longitude.to_bits().hash(&mut h1);
    let a = h1.finish();

    let mut h2 = DefaultHasher::new();
    a.hash(&mut h2);
    seed.tz_offset_hours.to_bits().hash(&mut h2);
    seed.secondary_name.hash(&mut h2);
    let b = h2.finish();

    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (a >> 32) as u32,
        (a >> 16) as u16,
        (a & 0x0FFF) as u16,
        0x8000u16 | ((b >> 48) as u16 & 0x3FFF),
        b & 0x0000_FFFF_FFFF_FFFF_u64
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> UidSeed<'static> {
        UidSeed {
            name: "Anna Freud",
            year: 1895,
            month: 12,
            day: 3,
            hour: 15,
            minute: 15,
            second: 0,
            latitude: 48.208_333,
            longitude: 16.371_667,
            tz_offset_hours: 1.0,
            secondary_name: None,
        }
    }

    #[test]
    fn derive_is_stable() {
        assert_eq!(derive_uid(&seed()), derive_uid(&seed()));
    }

    #[test]
    fn derive_changes_with_name() {
        let mut s = seed();
        let a = derive_uid(&s);
        s.name = "Sigmund Freud";
        assert_ne!(a, derive_uid(&s));
    }

    #[test]
    fn derive_has_uuid_shape() {
        let id = derive_uid(&seed());
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
    }

    #[test]
    fn derive_changes_with_year() {
        let mut s = seed();
        let a = derive_uid(&s);
        s.year = 1896;
        assert_ne!(a, derive_uid(&s));
    }

    #[test]
    fn random_differs_each_call() {
        assert_ne!(random_uid(), random_uid());
    }
}
