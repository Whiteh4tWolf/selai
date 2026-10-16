//! Hash chain over audit events. Each `event_hash` covers the previous hash and
//! the event's canonical form, so editing any stored row breaks every later link.
//! This is tamper-evident against accidental change only (ADR-0005).

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// What the chain covers for one event.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainEvent {
    pub seq: i64,
    pub event_type: String,
    pub task_id: Option<String>,
    pub execution_id: Option<String>,
    pub payload: Value,
    pub created_at_ms: i64,
}

/// A stored link: the hash recorded for an event and the hash before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainLink {
    pub prior_hash: Option<String>,
    pub event_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChainError {
    #[error("audit chain is broken at sequence {seq}")]
    Broken { seq: i64 },
    #[error("audit sequence has a gap or repeat at {seq}")]
    Sequence { seq: i64 },
}

/// Serializes JSON with object keys sorted at every level, so the hash does not
/// depend on the order keys were inserted or on serde_json feature flags.
pub fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut sorted: Vec<(&String, &Value)> = map.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(b.0));
            let body: Vec<String> = sorted
                .into_iter()
                .map(|(k, v)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap_or_default(),
                        canonical_json(v)
                    )
                })
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(items) => {
            let body: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", body.join(","))
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Hash for one event given the hash before it.
pub fn event_hash(prior: Option<&str>, event: &ChainEvent) -> String {
    let mut canonical = Map::new();
    canonical.insert("seq".into(), Value::from(event.seq));
    canonical.insert("type".into(), Value::from(event.event_type.clone()));
    canonical.insert(
        "task_id".into(),
        event.task_id.clone().map_or(Value::Null, Value::from),
    );
    canonical.insert(
        "execution_id".into(),
        event.execution_id.clone().map_or(Value::Null, Value::from),
    );
    canonical.insert("payload".into(), event.payload.clone());
    canonical.insert("created_at_ms".into(), Value::from(event.created_at_ms));
    let material = format!(
        "{}\n{}",
        prior.unwrap_or(""),
        canonical_json(&Value::Object(canonical))
    );
    let mut hasher = Sha256::new();
    hasher.update(material.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Checks a whole chain, in sequence order, starting from the first event.
/// Returns the first broken link.
pub fn verify_chain(events: &[(ChainEvent, ChainLink)]) -> Result<(), ChainError> {
    let mut expected_prior: Option<String> = None;
    let mut expected_seq: Option<i64> = None;
    for (event, link) in events {
        if let Some(seq) = expected_seq {
            if event.seq != seq {
                return Err(ChainError::Sequence { seq: event.seq });
            }
        }
        if link.prior_hash != expected_prior {
            return Err(ChainError::Broken { seq: event.seq });
        }
        if event_hash(link.prior_hash.as_deref(), event) != link.event_hash {
            return Err(ChainError::Broken { seq: event.seq });
        }
        expected_prior = Some(link.event_hash.clone());
        expected_seq = Some(event.seq + 1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(seq: i64, payload: Value) -> ChainEvent {
        ChainEvent {
            seq,
            event_type: "tool_proposed".into(),
            task_id: Some("t".into()),
            execution_id: None,
            payload,
            created_at_ms: 1000 + seq,
        }
    }

    fn build(events: &[ChainEvent]) -> Vec<(ChainEvent, ChainLink)> {
        let mut prior: Option<String> = None;
        let mut out = Vec::new();
        for e in events {
            let hash = event_hash(prior.as_deref(), e);
            out.push((
                e.clone(),
                ChainLink {
                    prior_hash: prior.clone(),
                    event_hash: hash.clone(),
                },
            ));
            prior = Some(hash);
        }
        out
    }

    #[test]
    fn canonical_form_ignores_key_order() {
        let a = json!({"b": 1, "a": {"y": 2, "x": [3]}});
        let b = json!({"a": {"x": [3], "y": 2}, "b": 1});
        assert_eq!(canonical_json(&a), canonical_json(&b));
    }

    #[test]
    fn an_intact_chain_verifies() {
        let chain = build(&[
            ev(1, json!({"k": 1})),
            ev(2, json!({"k": 2})),
            ev(3, json!({})),
        ]);
        assert_eq!(verify_chain(&chain), Ok(()));
    }

    #[test]
    fn editing_any_row_breaks_it_at_that_row() {
        let mut chain = build(&[
            ev(1, json!({"cmd": "df"})),
            ev(2, json!({"exit": 0})),
            ev(3, json!({})),
        ]);
        chain[1].0.payload = json!({"exit": 1});
        assert_eq!(verify_chain(&chain), Err(ChainError::Broken { seq: 2 }));
    }

    #[test]
    fn a_deleted_row_is_a_sequence_error() {
        let mut chain = build(&[ev(1, json!({})), ev(2, json!({})), ev(3, json!({}))]);
        chain.remove(1);
        assert_eq!(verify_chain(&chain), Err(ChainError::Sequence { seq: 3 }));
    }
}
