//! Context panel: token/file summary lines fed by `App::set_context`.
//!
//! The lines are already localised by the caller, because only
//! `apps/darb` knows the numbers behind them.

use darb_core::i18n::global_text;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme::Theme;
use crate::widgets;

pub fn render_context(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let text = if app.context_lines.is_empty() {
        global_text("panel.empty")
    } else {
        app.context_lines.join("\n")
    };
    frame.render_widget(
        // The context panel is informational: it is never a focus target.
        Paragraph::new(text).block(widgets::panel(global_text("panel.context"), false, theme)),
        area,
    );
}
