//! System panel (bottom of the left sidebar): CPU/RAM gauges, session
//! stats, performance profile and agent mode. Every value is real state
//! fed by `apps/darb` (throttled `refresh_system`); unknown numbers are
//! skipped, never faked (Contribuição §41).

use darb_core::i18n::global_text;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{status_key, App};
use crate::theme::Theme;
use crate::widgets;

/// How many rows the panel needs besides the border (keeps the sidebar
/// split honest — the explorer gets what is left).
pub const SYSTEM_MIN_ROWS: u16 = 6;

pub fn render_system(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mut lines: Vec<ratatui::text::Line> = Vec::with_capacity(area.height as usize);

    // Telemetry: drawn only when a measurement exists (cpu_percent >= 0).
    if app.cpu_percent >= 0.0 {
        lines.push(widgets::gauge("CPU", app.cpu_percent / 100.0, 16, theme));
    }
    if app.mem_percent >= 0.0 {
        lines.push(widgets::gauge("RAM", app.mem_percent / 100.0, 16, theme));
        if !app.mem_total.is_empty() {
            lines.push(widgets::kv_line(
                "mem",
                format!("{} / {}", app.mem_used, app.mem_total),
                theme,
            ));
        }
    }
    if !app.profile.is_empty() {
        lines.push(widgets::kv_line(
            &global_text("app.profile_key"),
            format!("[{}]", app.profile),
            theme,
        ));
    }
    if app.tool_calls > 0 {
        lines.push(widgets::kv_line(
            &global_text("app.tools_key"),
            app.tool_calls.to_string(),
            theme,
        ));
    }
    lines.push(widgets::kv_line(
        "mode",
        format!("[{}]", app.agent_mode.name()),
        theme,
    ));
    lines.push(widgets::kv_line(
        &global_text("app.state_key"),
        global_text(status_key(&app.agent_state)),
        theme,
    ));

    frame.render_widget(
        Paragraph::new(lines).block(widgets::panel(global_text("panel.system"), false, theme)),
        area,
    );
}
