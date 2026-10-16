//! Host label and port validation (engineering spec section 3.1).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LabelError {
    #[error("label must be 2 to 48 characters, got {0}")]
    Length(usize),
    #[error("label must start with a letter or digit")]
    BadStart,
    #[error("label may only contain letters, digits, spaces, '-', '_' and '.'")]
    BadCharacter,
}

impl crate::SelaiError for LabelError {
    fn code(&self) -> crate::ErrorCode {
        crate::ErrorCode::InvalidInput
    }
}

/// A host name for people, such as `Production Web` or `db-01`.
///
/// Leading and trailing whitespace is removed and inner runs of whitespace become
/// one space. The case the user typed is kept for display, but two labels that
/// differ only in case are the same label (ADR-0001, amended): equality and
/// hashing ignore ASCII case, and the database index is `COLLATE NOCASE`.
#[derive(Debug, Clone)]
pub struct HostLabel(String);

impl PartialEq for HostLabel {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for HostLabel {}

impl std::hash::Hash for HostLabel {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_ascii_lowercase().hash(state);
    }
}

impl HostLabel {
    pub const MIN_LEN: usize = 2;
    pub const MAX_LEN: usize = 48;

    pub fn parse(input: &str) -> Result<Self, LabelError> {
        let normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");
        let len = normalized.chars().count();
        if !(Self::MIN_LEN..=Self::MAX_LEN).contains(&len) {
            return Err(LabelError::Length(len));
        }
        if !normalized.starts_with(|c: char| c.is_ascii_alphanumeric()) {
            return Err(LabelError::BadStart);
        }
        if !normalized
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
        {
            return Err(LabelError::BadCharacter);
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HostLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for HostLabel {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for HostLabel {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("port must be between 1 and 65535")]
pub struct PortError;

impl crate::SelaiError for PortError {
    fn code(&self) -> crate::ErrorCode {
        crate::ErrorCode::InvalidInput
    }
}

/// A TCP port in `1..=65535`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Port(u16);

impl Port {
    pub const SSH_DEFAULT: Port = Port(22);

    pub fn new(value: u32) -> Result<Self, PortError> {
        match u16::try_from(value) {
            Ok(p) if p != 0 => Ok(Self(p)),
            _ => Err(PortError),
        }
    }

    pub fn get(self) -> u16 {
        self.0
    }
}

impl Serialize for Port {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u16(self.0)
    }
}

impl<'de> Deserialize<'de> for Port {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = u32::deserialize(d)?;
        Self::new(v).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_labels_with_spaces_and_keeps_their_case() {
        let label = HostLabel::parse("Production Web").expect("valid");
        assert_eq!(label.as_str(), "Production Web");
        let tidy = HostLabel::parse("  Production \t  Web  ").expect("valid");
        assert_eq!(
            tidy.as_str(),
            "Production Web",
            "whitespace is trimmed and collapsed"
        );
        for ok in [
            "ab",
            "db_1",
            "db 01",
            "10.0.0.5 web",
            "web-01.prod",
            &"a".repeat(48),
        ] {
            assert!(HostLabel::parse(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn case_variants_are_the_same_label() {
        let a = HostLabel::parse("Production Web").expect("a");
        let b = HostLabel::parse("PRODUCTION WEB").expect("b");
        assert_eq!(a, b);
        let hash = |l: &HostLabel| {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            l.hash(&mut h);
            h.finish()
        };
        assert_eq!(hash(&a), hash(&b), "equal labels must hash alike");
    }

    #[test]
    fn rejects_invalid_labels() {
        assert_eq!(HostLabel::parse("a"), Err(LabelError::Length(1)));
        assert_eq!(HostLabel::parse(""), Err(LabelError::Length(0)));
        assert_eq!(HostLabel::parse("   "), Err(LabelError::Length(0)));
        assert_eq!(
            HostLabel::parse(&"a".repeat(49)),
            Err(LabelError::Length(49))
        );
        assert_eq!(HostLabel::parse("-abc"), Err(LabelError::BadStart));
        assert_eq!(HostLabel::parse(".abc"), Err(LabelError::BadStart));
        for bad in ["a/b", "web\u{e9}", "ab@c", "a\u{0}b", "a;b", "a$b"] {
            assert_eq!(
                HostLabel::parse(bad),
                Err(LabelError::BadCharacter),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn port_bounds() {
        assert!(Port::new(0).is_err());
        assert!(Port::new(65536).is_err());
        assert_eq!(Port::new(1).expect("1").get(), 1);
        assert_eq!(Port::new(65535).expect("max").get(), 65535);
        assert_eq!(Port::SSH_DEFAULT.get(), 22);
        assert!(serde_json::from_str::<Port>("70000").is_err());
        assert!(serde_json::from_str::<Port>("-1").is_err());
    }
}
