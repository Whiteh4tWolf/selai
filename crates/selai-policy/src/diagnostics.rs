//! Fixed diagnostic recipes. The only place diagnostic commands are built.
//! Model input never reaches the command text except through validated values.

use crate::ModelCallError;

/// A systemd unit or service name: ASCII letters, digits and `@ . _ -`, 1 to 64.
/// Anything else (quotes, spaces, `;`, `$`, `/`) is refused before a command exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceName(String);

impl ServiceName {
    pub fn parse(input: &str) -> Result<Self, ModelCallError> {
        let ok = (1..=64).contains(&input.len())
            && input
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'@' | b'.' | b'_' | b'-'));
        if ok {
            Ok(Self(input.to_owned()))
        } else {
            Err(ModelCallError::invalid(
                "service name must be 1 to 64 letters, digits or @._-",
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Number of journal lines, 1 to 200.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogLines(u32);

impl LogLines {
    pub fn parse(value: u64) -> Result<Self, ModelCallError> {
        match u32::try_from(value) {
            Ok(v @ 1..=200) => Ok(Self(v)),
            _ => Err(ModelCallError::invalid("lines must be between 1 and 200")),
        }
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticRequest {
    Uptime,
    DiskUsage,
    MemorySummary,
    OsInfo,
    ServiceStatus {
        service: ServiceName,
    },
    RecentServiceLogs {
        service: ServiceName,
        lines: LogLines,
    },
}

impl DiagnosticRequest {
    /// Parses `operation` and its arguments from model input. Unknown operations
    /// and arguments that do not belong to the operation are refused.
    pub fn parse(
        operation: &str,
        service: Option<&str>,
        lines: Option<u64>,
    ) -> Result<Self, ModelCallError> {
        let needs_service = matches!(operation, "service_status" | "recent_service_logs");
        if !needs_service && service.is_some() {
            return Err(ModelCallError::invalid(
                "service is only valid for service operations",
            ));
        }
        if operation != "recent_service_logs" && lines.is_some() {
            return Err(ModelCallError::invalid(
                "lines is only valid for recent_service_logs",
            ));
        }
        match operation {
            "uptime" => Ok(Self::Uptime),
            "disk_usage" => Ok(Self::DiskUsage),
            "memory_summary" => Ok(Self::MemorySummary),
            "os_info" => Ok(Self::OsInfo),
            "service_status" => Ok(Self::ServiceStatus {
                service: ServiceName::parse(
                    service.ok_or_else(|| ModelCallError::invalid("service is required"))?,
                )?,
            }),
            "recent_service_logs" => Ok(Self::RecentServiceLogs {
                service: ServiceName::parse(
                    service.ok_or_else(|| ModelCallError::invalid("service is required"))?,
                )?,
                lines: LogLines::parse(lines.unwrap_or(50))?,
            }),
            _ => Err(ModelCallError::invalid("unknown diagnostic operation")),
        }
    }
}

/// The exact command text for a diagnostic. It is what the audit records and
/// what the remote shell runs.
pub fn diagnostic_command(request: &DiagnosticRequest) -> String {
    match request {
        DiagnosticRequest::Uptime => "uptime".to_owned(),
        DiagnosticRequest::DiskUsage => "df -hP".to_owned(),
        DiagnosticRequest::MemorySummary => "free -h".to_owned(),
        DiagnosticRequest::OsInfo => "uname -sr; head -n 3 /etc/os-release 2>/dev/null".to_owned(),
        DiagnosticRequest::ServiceStatus { service } => {
            format!(
                "systemctl --no-pager status -- '{}' 2>&1 | head -n 20",
                service.as_str()
            )
        }
        DiagnosticRequest::RecentServiceLogs { service, lines } => format!(
            "journalctl --no-pager -n {} -u '{}' 2>&1",
            lines.get(),
            service.as_str()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_names_refuse_shell_syntax() {
        for bad in [
            "nginx;rm",
            "a b",
            "x$(id)",
            "it's",
            "a/b",
            "",
            &"a".repeat(65),
        ] {
            assert!(ServiceName::parse(bad).is_err(), "{bad:?}");
        }
        assert!(ServiceName::parse("nginx.service").is_ok());
        assert!(ServiceName::parse("getty@tty1").is_ok());
    }

    #[test]
    fn line_counts_are_bounded() {
        assert!(LogLines::parse(0).is_err());
        assert!(LogLines::parse(201).is_err());
        assert_eq!(LogLines::parse(200).expect("max").get(), 200);
    }

    #[test]
    fn arguments_must_belong_to_the_operation() {
        assert!(DiagnosticRequest::parse("disk_usage", Some("nginx"), None).is_err());
        assert!(DiagnosticRequest::parse("service_status", None, None).is_err());
        assert!(DiagnosticRequest::parse("uptime", None, Some(5)).is_err());
        assert!(DiagnosticRequest::parse("rm_everything", None, None).is_err());
    }

    #[test]
    fn commands_are_fixed_templates() {
        let svc = ServiceName::parse("nginx").expect("name");
        assert_eq!(diagnostic_command(&DiagnosticRequest::DiskUsage), "df -hP");
        assert_eq!(
            diagnostic_command(&DiagnosticRequest::RecentServiceLogs {
                service: svc,
                lines: LogLines::parse(20).expect("lines"),
            }),
            "journalctl --no-pager -n 20 -u 'nginx' 2>&1"
        );
    }
}
