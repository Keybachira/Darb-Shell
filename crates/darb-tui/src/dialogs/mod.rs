//! Modal dialogs. The permission dialog mirrors
//! `PermissionRequested` / `PermissionResolved`: it shows what the agent
//! wants to run and returns the answer through `KeyOutcome` — the dialog
//! itself never executes anything.

use darb_core::i18n::global_text;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::PermissionPrompt;
use crate::layout::centered_rect;
use crate::theme::Theme;

/// Draw the permission modal centered over `area`.
pub fn render_permission(frame: &mut Frame, area: Rect, prompt: &PermissionPrompt, theme: &Theme) {
    let popup = centered_rect(area, 64, 9);
    frame.render_widget(Clear, popup);
    let body = format!(
        "{}\n{}\n\n[a] {}   [d] {}",
        prompt.tool,
        prompt.target,
        global_text("dialog.allow"),
        global_text("dialog.deny"),
    );
    frame.render_widget(
        Paragraph::new(body).block(
            Block::default()
                .title(global_text("dialog.permission_title"))
                .borders(Borders::ALL)
                .border_style(theme.warning),
        ),
        popup,
    );
}
