//! Validated tool requests built from model arguments.

use selai_domain::HostId;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    diagnostics::{diagnostic_command, DiagnosticRequest},
    MAX_COMMAND_BYTES, MAX_COMMAND_TIMEOUT_SECS,
};

/// A refused model argument. The message is safe to return to the model.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ModelCallError(String);

impl ModelCallError {
    pub(crate) fn invalid(message: &str) -> Self {
        Self(message.to_owned())
    }
}

/// A free-form command. Length and timeout are bounded; content is not judged,
/// because command classification is not a security boundary (spec, section 6.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRequest {
    command: String,
    timeout_secs: u32,
    /// Run through `sudo`, wrapped by local code. The model never writes the
    /// sudo invocation and never sees a password.
    as_root: bool,
}

impl CommandRequest {
    pub fn parse(command: &str, timeout_secs: u32) -> Result<Self, ModelCallError> {
        if command.trim().is_empty() {
            return Err(ModelCallError::invalid("command must not be empty"));
        }
        if command.len() > MAX_COMMAND_BYTES {
            return Err(ModelCallError::invalid("command is too long"));
        }
        if command.contains('\0') {
            return Err(ModelCallError::invalid("command must not contain NUL"));
        }
        if !(1..=MAX_COMMAND_TIMEOUT_SECS).contains(&timeout_secs) {
            return Err(ModelCallError::invalid(
                "timeout_secs must be between 1 and 300",
            ));
        }
        Ok(Self {
            command: command.to_owned(),
            timeout_secs,
            as_root: false,
        })
    }

    /// The same command, to be run as root.
    pub fn as_root(mut self) -> Self {
        self.as_root = true;
        self
    }

    pub fn runs_as_root(&self) -> bool {
        self.as_root
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn timeout_secs(&self) -> u32 {
        self.timeout_secs
    }
}

/// One tool request, already bound to a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRequest {
    Diagnostic {
        host: HostId,
        request: DiagnosticRequest,
    },
    Command {
        host: HostId,
        request: CommandRequest,
    },
}

impl ToolRequest {
    pub fn host(&self) -> HostId {
        match self {
            Self::Diagnostic { host, .. } | Self::Command { host, .. } => *host,
        }
    }

    /// Name stored in `command_executions.tool_name`.
    pub fn tool_name(&self) -> &'static str {
        match self {
            Self::Diagnostic { .. } => "ssh.run_diagnostic",
            Self::Command { .. } => "ssh.run_command",
        }
    }

    /// The command as stored, shown and approved. A root command reads
    /// `sudo <command>`, so an approval for the plain command can never match it.
    pub fn command_text(&self) -> String {
        match self {
            Self::Diagnostic { request, .. } => diagnostic_command(request),
            Self::Command { request, .. } if request.runs_as_root() => {
                format!("sudo {}", request.command())
            }
            Self::Command { request, .. } => request.command().to_owned(),
        }
    }

    /// The text handed to the shell. For a root command this is the inner
    /// command; the executor adds the sudo wrapper itself.
    pub fn shell_command(&self) -> String {
        match self {
            Self::Diagnostic { request, .. } => diagnostic_command(request),
            Self::Command { request, .. } => request.command().to_owned(),
        }
    }

    pub fn runs_as_root(&self) -> bool {
        matches!(self, Self::Command { request, .. } if request.runs_as_root())
    }

    /// SHA-256 of the command text, lowercase hex. Approval binds to this value.
    pub fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.command_text().as_bytes());
        hex(&hasher.finalize())
    }

    pub fn timeout_secs(&self) -> u32 {
        match self {
            Self::Diagnostic { .. } => 30,
            Self::Command { request, .. } => request.timeout_secs(),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Turns a model tool call into a validated request for the task's host.
///
/// `host` is the task's bound host, never taken from the arguments. Any property
/// not in the schema is refused, so a `host`, `host_id` or `hostname` argument
/// cannot change where the command runs.
pub fn parse_model_call(
    name: &str,
    arguments: &Value,
    host: HostId,
) -> Result<ToolRequest, ModelCallError> {
    let object = arguments
        .as_object()
        .ok_or_else(|| ModelCallError::invalid("arguments must be a JSON object"))?;
    match name {
        "ssh_run_diagnostic" => {
            reject_unknown(object, &["operation", "service", "lines"])?;
            let operation = object
                .get("operation")
                .and_then(Value::as_str)
                .ok_or_else(|| ModelCallError::invalid("operation is required"))?;
            let service = match object.get("service") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    v.as_str()
                        .ok_or_else(|| ModelCallError::invalid("service must be a string"))?,
                ),
            };
            let lines = match object.get("lines") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    v.as_u64()
                        .ok_or_else(|| ModelCallError::invalid("lines must be an integer"))?,
                ),
            };
            let request = DiagnosticRequest::parse(operation, service, lines)?;
            Ok(ToolRequest::Diagnostic { host, request })
        }
        "ssh_run_command" => {
            reject_unknown(object, &["command", "timeout_secs", "as_root"])?;
            let command = object
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| ModelCallError::invalid("command is required"))?;
            let timeout =
                match object.get("timeout_secs") {
                    None | Some(Value::Null) => MAX_COMMAND_TIMEOUT_SECS.min(30),
                    Some(v) => u32::try_from(v.as_u64().ok_or_else(|| {
                        ModelCallError::invalid("timeout_secs must be an integer")
                    })?)
                    .map_err(|_| ModelCallError::invalid("timeout_secs is out of range"))?,
                };
            let as_root = match object.get("as_root") {
                None | Some(Value::Null) => false,
                Some(v) => v
                    .as_bool()
                    .ok_or_else(|| ModelCallError::invalid("as_root must be true or false"))?,
            };
            let request = CommandRequest::parse(command, timeout)?;
            let request = if as_root { request.as_root() } else { request };
            Ok(ToolRequest::Command { host, request })
        }
        _ => Err(ModelCallError::invalid("unknown tool")),
    }
}

fn reject_unknown(
    object: &serde_json::Map<String, Value>,
    allowed: &[&str],
) -> Result<(), ModelCallError> {
    match object.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(_) => Err(ModelCallError::invalid(
            "the call contains an argument that is not allowed",
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_host_argument_is_refused_not_obeyed() {
        let host = HostId::new();
        for smuggled in ["host", "host_id", "hostname", "target"] {
            let args = json!({"operation": "uptime", smuggled: "other-box"});
            assert!(
                parse_model_call("ssh_run_diagnostic", &args, host).is_err(),
                "{smuggled}"
            );
        }
    }

    #[test]
    fn a_valid_call_binds_to_the_task_host() {
        let host = HostId::new();
        let req = parse_model_call(
            "ssh_run_command",
            &json!({"command": "id", "timeout_secs": 5}),
            host,
        )
        .expect("valid");
        assert_eq!(req.host(), host);
        assert_eq!(req.tool_name(), "ssh.run_command");
    }

    #[test]
    fn unknown_tools_and_non_object_arguments_are_refused() {
        let host = HostId::new();
        assert!(parse_model_call("ssh.run_command", &json!({"command": "id"}), host).is_err());
        assert!(parse_model_call("ssh_run_command", &json!("id"), host).is_err());
    }

    #[test]
    fn digest_changes_with_a_single_byte() {
        let host = HostId::new();
        let a = parse_model_call("ssh_run_command", &json!({"command": "ls"}), host).expect("a");
        let b = parse_model_call("ssh_run_command", &json!({"command": "ls "}), host).expect("b");
        assert_ne!(a.digest(), b.digest());
        assert_eq!(a.digest().len(), 64);
    }

    #[test]
    fn commands_are_bounded() {
        assert!(CommandRequest::parse("", 5).is_err());
        assert!(CommandRequest::parse("a\0b", 5).is_err());
        assert!(CommandRequest::parse(&"x".repeat(MAX_COMMAND_BYTES + 1), 5).is_err());
        assert!(CommandRequest::parse("id", 0).is_err());
        assert!(CommandRequest::parse("id", 1).is_ok());
        assert!(CommandRequest::parse("id", MAX_COMMAND_TIMEOUT_SECS).is_ok());
        assert!(CommandRequest::parse("id", MAX_COMMAND_TIMEOUT_SECS + 1).is_err());
    }

    #[test]
    fn a_root_command_is_bound_to_its_own_digest_and_runs_the_inner_text() {
        let host = HostId::new();
        let plain = parse_model_call(
            "ssh_run_command",
            &json!({"command": "systemctl restart nginx"}),
            host,
        )
        .expect("plain");
        let root = parse_model_call(
            "ssh_run_command",
            &json!({"command": "systemctl restart nginx", "as_root": true}),
            host,
        )
        .expect("root");
        assert!(!plain.runs_as_root());
        assert!(root.runs_as_root());
        assert_eq!(root.command_text(), "sudo systemctl restart nginx");
        assert_eq!(root.shell_command(), "systemctl restart nginx");
        assert_ne!(
            plain.digest(),
            root.digest(),
            "approving one must not approve the other"
        );
        assert!(parse_model_call(
            "ssh_run_command",
            &json!({"command": "id", "as_root": "yes"}),
            host
        )
        .is_err());
        assert!(
            parse_model_call(
                "ssh_run_diagnostic",
                &json!({"operation": "uptime", "as_root": true}),
                host
            )
            .is_err(),
            "diagnostics never run as root"
        );
    }
}
