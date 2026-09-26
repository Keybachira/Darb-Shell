//! TUI components — explorer, system, chat, context, diff, terminal, git,
//! tasks. The header is drawn by `app::render_header` (no module of its
//! own): it shares the header row helpers with the status bar.

pub mod chat;
pub mod context;
pub mod diff;
pub mod explorer;
pub mod git;
pub mod system;
pub mod tasks;
pub mod terminal;
