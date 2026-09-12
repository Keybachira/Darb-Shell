//! darb-agent — planner, executor, reviewer, loop, tool calling, state.
//! Depends on darb-core + darb-tools + darb-context + darb-memory (§17, §24).

pub mod executor;
pub mod loop_;
pub mod planner;
pub mod reviewer;
pub mod state;
pub mod tool_calling;
