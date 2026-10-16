//! Redaction of text before it is stored or shown to a model (ADR-0005).
//!
//! Best effort. It catches common secret shapes and any literal secret the
//! task holds. It cannot find every secret; the audit documentation says so.

use regex::Regex;
use std::sync::OnceLock;

/// Result of redaction: the cleaned text and how many replacements were made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redacted {
    pub text: String,
    pub replacements: usize,
}

// The patterns below are literals checked by the unit tests in this file, so an
// invalid one would fail there, not at runtime.
#[allow(clippy::expect_used)]
fn key_value() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // The key name may carry a prefix such as DB_ or APP-, so no leading word boundary.
        Regex::new(r#"(?i)([A-Za-z0-9_.-]*(?:password|passwd|pwd|passphrase|token|secret|api[_-]?key|authorization))(\s*[:=]\s*|\s+)(?:bearer\s+)?[^\s'",;]+"#)
            .expect("static pattern")
    })
}

#[allow(clippy::expect_used)]
fn private_key_block() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
        )
        .expect("static pattern")
    })
}

#[allow(clippy::expect_used)]
fn bearer() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)bearer\s+[A-Za-z0-9._~+/=-]{8,}").expect("static pattern"))
}

/// Redacts `text`. `literals` are exact secret values the task holds (for
/// example a password or passphrase). They are replaced everywhere.
pub fn redact(text: &str, literals: &[&str]) -> Redacted {
    let mut replacements = 0usize;
    let mut out = text.to_owned();

    for literal in literals.iter().filter(|l| l.len() >= 4) {
        let count = out.matches(literal).count();
        if count > 0 {
            replacements += count;
            out = out.replace(literal, "[redacted]");
        }
    }

    let count = private_key_block().find_iter(&out).count();
    if count > 0 {
        replacements += count;
        out = private_key_block()
            .replace_all(&out, "[redacted private key]")
            .into_owned();
    }

    let count = bearer().find_iter(&out).count();
    if count > 0 {
        replacements += count;
        out = bearer().replace_all(&out, "Bearer [redacted]").into_owned();
    }

    let count = key_value().find_iter(&out).count();
    if count > 0 {
        replacements += count;
        out = key_value()
            .replace_all(&out, |caps: &regex::Captures<'_>| {
                format!("{}=[redacted]", &caps[1])
            })
            .into_owned();
    }

    Redacted {
        text: out,
        replacements,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_secret_shapes_are_removed() {
        let r = redact("db password=hunter2 and token: abc123 end", &[]);
        assert!(!r.text.contains("hunter2"));
        assert!(!r.text.contains("abc123"));
        assert!(r.replacements >= 2);
    }

    #[test]
    fn bearer_tokens_and_private_keys_are_removed() {
        let r = redact("Authorization: Bearer sk-live-abcdefghijk", &[]);
        assert!(!r.text.contains("sk-live"));
        let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n-----END OPENSSH PRIVATE KEY-----";
        assert_eq!(redact(key, &[]).text, "[redacted private key]");
    }

    #[test]
    fn task_literals_are_removed_wherever_they_appear() {
        let r = redact(
            "echo correct-horse; printf correct-horse",
            &["correct-horse"],
        );
        assert!(!r.text.contains("correct-horse"));
        assert_eq!(r.replacements, 2);
    }

    #[test]
    fn prefixed_key_names_are_redacted_and_the_name_is_kept() {
        let r = redact(
            "DB_PASSWORD=hunter2-very-secret\nAPP-API-KEY: abc123xyz\nPORT=8080",
            &[],
        );
        assert!(!r.text.contains("hunter2"), "{}", r.text);
        assert!(!r.text.contains("abc123xyz"), "{}", r.text);
        assert!(r.text.contains("DB_PASSWORD=[redacted]"), "{}", r.text);
        assert!(r.text.contains("PORT=8080"));
    }

    #[test]
    fn ordinary_output_is_untouched() {
        let text = "Filesystem Size Used Avail Use% Mounted on\n/dev/vda 252G 25G 15G 63% /";
        let r = redact(text, &[]);
        assert_eq!(r.text, text);
        assert_eq!(r.replacements, 0);
    }
}
