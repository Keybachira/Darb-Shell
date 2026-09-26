//! Shared widget helpers for the panels.
//!
//! Every panel is the same shape — titled block, content, empty marker —
//! so that shape lives here once (Contribuição §10) instead of being
//! re-typed in each component. Small stateless ornaments (gauge, toast
//! chip, span truncation, section header) live here too so components
//! stay small.

use darb_core::i18n::global_text;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};

use crate::theme::Theme;

/// Bordered panel block. The focused panel gets the accent border so the
/// user can see where keys will land.
pub fn panel(title: String, focused: bool, theme: &Theme) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(theme.primary)
        } else {
            Style::default().fg(theme.border)
        })
}

/// The marker shown by a panel that has nothing to display.
pub fn empty_line(theme: &Theme) -> Line<'static> {
    Line::styled(global_text("panel.empty"), Style::default().fg(theme.muted))
}

/// Section header inside a panel: `── LABEL ─────` in cyan. A cheap,
/// border-free way to group dense sidebar content.
pub fn section_header(title: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        format!("── {title} "),
        Style::default().fg(theme.accent),
    ))
}

/// Key/value row: muted key, normal value.
pub fn kv_line(key: &str, value: String, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key}: "), Style::default().fg(theme.muted)),
        Span::styled(value, Style::default().fg(theme.text)),
    ])
}

/// Horizontal gauge: `label ▓▓▓▓░░░░ 42%`. Fixed inner width so numbers
/// do not shift as the value changes (no layout jitter), and it degrades
/// to a plain `label value` line on very narrow sidebars.
pub fn gauge(label: &str, fraction: f32, inner: usize, theme: &Theme) -> Line<'static> {
    let fraction = fraction.clamp(0.0, 1.0);
    let percent = (fraction * 100.0).round() as u16;
    let value = Span::styled(
        format!("{percent:>3}%"),
        Style::default().fg(if fraction >= 0.9 {
            theme.danger
        } else if fraction >= 0.7 {
            theme.warning
        } else {
            theme.success
        }),
    );
    if inner < 8 {
        return Line::from(vec![
            Span::styled(format!("{label} "), Style::default().fg(theme.muted)),
            value,
        ]);
    }
    let filled = ((inner as f32) * fraction).round() as usize;
    let bar = format!(
        "{}{}",
        "▮".repeat(filled),
        "▯".repeat(inner.saturating_sub(filled))
    );
    Line::from(vec![
        Span::styled(format!("{label} "), Style::default().fg(theme.muted)),
        Span::styled(bar, Style::default().fg(theme.accent)),
        value,
    ])
}

/// Truncate a styled span to `max` columns with `…` (cell width ≈ chars
/// here; wide glyphs may leave a one-cell gap — the same documented
/// approximation the editor cursor already makes).
pub fn truncated_span(text: String, max: usize, style: Style) -> Span<'static> {
    let width: usize = text.chars().map(char_width).sum();
    if width <= max {
        return Span::styled(text, style);
    }
    let mut kept = String::new();
    let mut used = 0usize;
    for c in text.chars() {
        let w = char_width(c);
        if used + w > max.saturating_sub(1) {
            break;
        }
        kept.push(c);
        used += w;
    }
    kept.push('…');
    Span::styled(kept, style)
}

/// Terminal cell width of one char. Only CJK-wide ranges are widened; a
/// full wcwidth table would be overkill for truncation (§61).
pub fn char_width(c: char) -> usize {
    let code = c as u32;
    let wide = (0x1100..=0x115F).contains(&code)
        || (0x2E80..=0xA4CF).contains(&code)
        || (0xAC00..=0xD7A3).contains(&code)
        || (0xF900..=0xFAFF).contains(&code)
        || (0xFF00..=0xFF60).contains(&code);
    usize::from(wide) + 1
}

/// Compact `key value` separator used across the dense panels.
pub fn separator(theme: &Theme) -> Span<'static> {
    Span::styled("  │  ", Style::default().fg(theme.faint))
}

/// Render pending toasts docked bottom-right over `area`. One row each;
/// they are few and short-lived (5), so drawing them over the shell is
/// cheap and readable.
pub fn render_toasts(frame: &mut ratatui::Frame, area: Rect, app: &crate::app::App, theme: &Theme) {
    if area.width < 20 || area.height < 1 || app.toasts.is_empty() {
        return;
    }
    for (row, toast) in app.toasts.iter().rev().enumerate() {
        if row as u16 + 1 > area.height {
            break;
        }
        let style = match toast.kind {
            crate::app::ToastKind::Info => Style::default().fg(theme.accent),
            crate::app::ToastKind::Success => Style::default().fg(theme.success),
            crate::app::ToastKind::Warning => Style::default().fg(theme.warning),
            crate::app::ToastKind::Error => Style::default().fg(theme.danger),
        };
        let line = Line::from(vec![
            Span::styled(format!(" {} ", toast.marker()), style),
            truncated_span(
                toast.text.clone(),
                (area.width as usize).saturating_sub(4),
                style,
            ),
        ]);
        let rect = Rect::new(
            area.x,
            area.y + area.height - 1 - row as u16,
            area.width.saturating_sub(1),
            1,
        );
        frame.render_widget(ratatui::widgets::Paragraph::new(line), rect);
    }
}
