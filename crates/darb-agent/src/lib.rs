//! darb-agent — planner, executor, reviewer, loop, tool calling, state.
//! Depends on darb-core + darb-tools + darb-context + darb-memory + darb-providers (§17, §24).
//! The Agent only sees `darb_providers::interface::Provider`, never a concrete API.

pub mod executor;
pub mod loop_;
pub mod planner;
pub mod reviewer;
pub mod state;
pub mod tool_calling;

pub use loop_::{Agent, FinalResult, PermissionResponder};
