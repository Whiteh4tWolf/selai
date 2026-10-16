//! Core IDs, state machines, error codes and shared contracts for Selai.
//!
//! This crate has no I/O. Everything here is pure and transport-neutral so a
//! future gateway can reuse it.

pub mod error;
pub mod host;
pub mod ids;
pub mod label;
pub mod page;
pub mod provider;
pub mod state;

pub use error::{ErrorCode, SelaiError};
pub use host::{
    AuthMethod, ConnectTimeout, GroupName, HostFieldError, HostSettings, Hostname, Keepalive,
    KeyPath, Notes, TerminalType, Username,
};
pub use ids::{
    ApprovalId, AuditEventId, ConversationId, ExecutionId, HostGroupId, HostId, IdError,
    KnownKeyId, ProviderId, TaskId,
};
pub use label::{HostLabel, LabelError, Port, PortError};
pub use page::{Page, PageRequest};
pub use provider::{
    BaseUrl, MaxOutputTokens, ModelId, ProviderFieldError, ProviderName, ProviderProtocol,
    ProviderSettings, TimeoutSecs,
};
pub use state::{ApprovalState, ExecStatus, TaskState, TransitionError};
