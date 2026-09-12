//! Theme: named terminal colors in one place.
//!
//! A single dark, low-distraction palette. Components take colors from
//! here instead of hardcoding them, so a future theme plugin only swaps
//! this struct.

use ratatui::style::Color;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub primary: Color,
    pub text: Color,
    pub muted: Color,
    pub border: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub highlight: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            primary: Color::Cyan,
            text: Color::Reset,
            muted: Color::DarkGray,
            border: Color::DarkGray,
            success: Color::Green,
            warning: Color::Yellow,
            danger: Color::Red,
            highlight: Color::Blue,
        }
    }
}
