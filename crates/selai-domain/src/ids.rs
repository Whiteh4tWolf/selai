//! Strongly typed identifiers. All IDs are UUIDs stored as lowercase hyphenated
//! text; parsing is strict so a malformed or forged ID never reaches storage.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {kind} id: expected a lowercase hyphenated UUID")]
pub struct IdError {
    kind: &'static str,
}

impl crate::SelaiError for IdError {
    fn code(&self) -> crate::ErrorCode {
        crate::ErrorCode::InvalidInput
    }
}

fn parse_strict(kind: &'static str, s: &str) -> Result<Uuid, IdError> {
    // Uuid::parse_str also accepts braced/simple/urn forms and upper case;
    // identity must have exactly one textual form.
    let uuid = Uuid::parse_str(s).map_err(|_| IdError { kind })?;
    if uuid.hyphenated().to_string() != s {
        return Err(IdError { kind });
    }
    Ok(uuid)
}

macro_rules! typed_id {
    ($(#[$doc:meta])* $name:ident, $kind:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a fresh random (v4) identifier.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0.hyphenated())
            }
        }

        impl FromStr for $name {
            type Err = IdError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                parse_strict($kind, s).map(Self)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

typed_id!(
    /// Immutable identity of a saved SSH host. Labels may change; this never does.
    HostId, "host"
);
typed_id!(HostGroupId, "host group");
typed_id!(ProviderId, "provider");
typed_id!(ConversationId, "conversation");
typed_id!(TaskId, "task");
typed_id!(ExecutionId, "execution");
typed_id!(ApprovalId, "approval");
typed_id!(KnownKeyId, "known key");
typed_id!(AuditEventId, "audit event");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_text_and_json() {
        let id = HostId::new();
        let text = id.to_string();
        assert_eq!(text.parse::<HostId>().expect("parse"), id);
        let json = serde_json::to_string(&id).expect("ser");
        assert_eq!(json, format!("\"{text}\""));
        assert_eq!(serde_json::from_str::<HostId>(&json).expect("de"), id);
    }

    #[test]
    fn rejects_non_canonical_forms() {
        let id = Uuid::new_v4();
        let upper = id.hyphenated().to_string().to_uppercase();
        for bad in [
            upper,
            id.simple().to_string(),
            format!("{{{}}}", id.hyphenated()),
            id.urn().to_string(),
            String::new(),
            "not-a-uuid".to_owned(),
            "'; DROP TABLE hosts; --".to_owned(),
        ] {
            assert!(bad.parse::<HostId>().is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn json_rejects_garbage() {
        assert!(serde_json::from_str::<TaskId>("\"x\"").is_err());
        assert!(serde_json::from_str::<TaskId>("42").is_err());
    }
}
