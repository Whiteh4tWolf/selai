//! The webview is presentation only (T-SEC threat: compromised UI). These checks
//! read the shipped configuration so a loosened CSP or an added plugin permission
//! fails the build instead of shipping quietly.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{fs, path::Path};

use serde_json::Value;

fn read_json(relative: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    let text = fs::read_to_string(&path).expect("read config");
    serde_json::from_str(&text).expect("valid json")
}

fn csp_directives() -> Vec<(String, Vec<String>)> {
    let config = read_json("tauri.conf.json");
    let csp = config["app"]["security"]["csp"]
        .as_str()
        .expect("csp is set");
    csp.split(';')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| {
            let mut parts = d.split_whitespace().map(str::to_owned);
            let name = parts.next().unwrap_or_default();
            (name, parts.collect())
        })
        .collect()
}

#[test]
fn csp_is_set_and_refuses_remote_code_and_plugins() {
    let directives = csp_directives();
    let get = |name: &str| {
        directives
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("CSP has no {name} directive"))
    };
    assert_eq!(get("default-src"), vec!["'self'"]);
    assert_eq!(
        get("script-src"),
        vec!["'self'"],
        "no inline or remote script"
    );
    assert_eq!(get("object-src"), vec!["'none'"]);
    assert_eq!(get("frame-ancestors"), vec!["'none'"]);
    assert_eq!(get("base-uri"), vec!["'none'"]);
    for (name, sources) in &directives {
        assert!(
            !sources.iter().any(|s| s == "'unsafe-eval'"),
            "{name} allows eval"
        );
        assert!(
            !sources.iter().any(|s| s == "*"),
            "{name} allows any origin"
        );
    }
}

#[test]
fn default_capability_grants_only_core_apis() {
    let capability = read_json("capabilities/default.json");
    let permissions: Vec<String> = capability["permissions"]
        .as_array()
        .expect("permissions array")
        .iter()
        .map(|p| p.as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(permissions, vec!["core:default".to_owned()]);
    for forbidden in ["shell", "fs", "http", "dialog", "process", "opener"] {
        assert!(
            !permissions.iter().any(|p| p.starts_with(forbidden)),
            "{forbidden} permission must not be granted to the webview"
        );
    }
}

#[test]
fn only_the_main_window_has_the_default_capability() {
    let capability = read_json("capabilities/default.json");
    let windows: Vec<&str> = capability["windows"]
        .as_array()
        .expect("windows array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(windows, vec!["main"]);
}
