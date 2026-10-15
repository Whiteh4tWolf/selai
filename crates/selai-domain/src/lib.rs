//! Core IDs, state machines, error codes and shared contracts for Selai.
//!
//! This crate has no I/O. Everything here is pure and transport-neutral so a
//! future gateway can reuse it.

pub mod error;
pub mod ids;
pub mod label;
pub mod page;
pub mod state;

pub use error::{ErrorCode, SelaiError};
pub use ids::{
    ApprovalId, ConversationId, ExecutionId, HostGroupId, HostId, IdError, ProviderId, TaskId,
};
pub use label::{HostLabel, LabelError, Port, PortError};
pub use page::{Page, PageRequest};
pub use state::{ApprovalState, ExecStatus, TaskState, TransitionError};
