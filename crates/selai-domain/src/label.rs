//! Host label and port validation (engineering spec section 3.1).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LabelError {
    #[error("label must be 2 to 48 characters, got {0}")]
    Length(usize),
    #[error("label must start with an ASCII letter")]
    BadStart,
    #[error("label may only contain ASCII letters, digits, '-' and '_'")]
    BadCharacter,
}

impl crate::SelaiError for LabelError {
    fn code(&self) -> crate::ErrorCode {
        crate::ErrorCode::InvalidInput
    }
}

/// A host alias such as `production-web`, matching the spec slug
/// `[a-z][a-z0-9_-]{1,47}`.
///
/// Input is ASCII case-folded to lower case on parse (ADR-0001), so
/// `PRODUCTION-WEB` and `production-web` are the same label and conflict.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HostLabel(String);

impl HostLabel {
    pub const MIN_LEN: usize = 2;
    pub const MAX_LEN: usize = 48;

    pub fn parse(input: &str) -> Result<Self, LabelError> {
        let len = input.chars().count();
        if !(Self::MIN_LEN..=Self::MAX_LEN).contains(&len) {
            return Err(LabelError::Length(len));
        }
        if !input.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return Err(LabelError::BadStart);
        }
        if !input
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(LabelError::BadCharacter);
        }
        Ok(Self(input.to_ascii_lowercase()))
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
    fn accepts_valid_labels_and_lowercases_them() {
        let label = HostLabel::parse("Production-Web").expect("valid");
        assert_eq!(label.as_str(), "production-web");
        for ok in ["ab", "db_1", &"a".repeat(48)] {
            assert!(HostLabel::parse(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn case_variants_are_the_same_label() {
        let a = HostLabel::parse("production-web").expect("a");
        let b = HostLabel::parse("PRODUCTION-WEB").expect("b");
        assert_eq!(a, b);
    }

    #[test]
    fn rejects_invalid_labels() {
        assert_eq!(HostLabel::parse("a"), Err(LabelError::Length(1)));
        assert_eq!(HostLabel::parse(""), Err(LabelError::Length(0)));
        assert_eq!(
            HostLabel::parse(&"a".repeat(49)),
            Err(LabelError::Length(49))
        );
        assert_eq!(HostLabel::parse("1abc"), Err(LabelError::BadStart));
        assert_eq!(HostLabel::parse("-abc"), Err(LabelError::BadStart));
        for bad in ["a b", "a.b", "a/b", "web\u{e9}", "ab\n", "ab@c", "a\u{0}b"] {
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
