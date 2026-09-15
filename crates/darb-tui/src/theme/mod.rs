//! Theme: DARB SYSTEM design tokens in one place.
//!
//! Ported from the web reference (`Criar interface editável/src/App.tsx`,
//! the `C` token object) so the terminal and the illustration share one
//! palette. Components take colors from here instead of hardcoding them,
//! so a future theme plugin only swaps this struct.
//!
//! Field names follow the web tokens, except `ok`/`warn`/`err`, which keep
//! the pre-existing `success`/`warning`/`danger` names to avoid churning
//! every component in the token port. `primary` (near-white text) and
//! `highlight` (selection) keep their names for the same reason; selection
//! roles should prefer `accent` in new code.

use ratatui::style::Color;

/// `accent` (#00d4ff): the single source for the cyan signal color.
/// `primary` and `highlight` are aliases — one value, three roles.
const ACCENT: Color = Color::Rgb(0x00, 0xd4, 0xff);

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    // Surfaces, darkest first.
    pub root: Color,
    pub shell: Color,
    pub panel: Color,
    pub surface: Color,
    pub elevated: Color,
    pub hover: Color,
    // Lines and dividers.
    pub border: Color,
    pub border2: Color,
    pub dim: Color,
    // Text ramp.
    pub text: Color,
    pub secondary: Color,
    pub muted: Color,
    pub faint: Color,
    // Signal colors.
    pub primary: Color,
    pub accent: Color,
    pub accent2: Color,
    pub highlight: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            root: Color::Rgb(0x06, 0x0b, 0x14),
            shell: Color::Rgb(0x08, 0x0e, 0x1a),
            panel: Color::Rgb(0x0b, 0x13, 0x22),
            surface: Color::Rgb(0x10, 0x1b, 0x2e),
            elevated: Color::Rgb(0x14, 0x21, 0x3a),
            hover: Color::Rgb(0x16, 0x23, 0x3a),
            border: Color::Rgb(0x1a, 0x29, 0x42),
            border2: Color::Rgb(0x23, 0x40, 0x5f),
            dim: Color::Rgb(0x25, 0x35, 0x50),
            text: Color::Rgb(0xe6, 0xee, 0xfb),
            secondary: Color::Rgb(0x8b, 0xa3, 0xc2),
            muted: Color::Rgb(0x4d, 0x64, 0x84),
            faint: Color::Rgb(0x2c, 0x42, 0x5f),
            primary: ACCENT,
            accent: ACCENT,
            accent2: Color::Rgb(0x4d, 0x7c, 0xff),
            highlight: ACCENT,
            success: Color::Rgb(0x00, 0xe6, 0x7a),
            warning: Color::Rgb(0xff, 0xb2, 0x24),
            danger: Color::Rgb(0xff, 0x5d, 0x5d),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brightness(color: Color) -> u16 {
        let Color::Rgb(r, g, b) = color else {
            panic!("DARB SYSTEM tokens must be explicit RGB, got {color:?}");
        };
        r as u16 + g as u16 + b as u16
    }

    #[test]
    fn accent_matches_the_web_token() {
        assert_eq!(Theme::default().accent, Color::Rgb(0x00, 0xd4, 0xff));
    }

    #[test]
    fn signal_aliases_share_one_value() {
        let theme = Theme::default();
        assert_eq!(theme.primary, ACCENT);
        assert_eq!(theme.highlight, ACCENT);
    }

    #[test]
    fn surfaces_run_dark_to_light() {
        let theme = Theme::default();
        let ramp = [
            theme.root,
            theme.shell,
            theme.panel,
            theme.surface,
            theme.elevated,
            theme.hover,
        ];
        let levels: Vec<u16> = ramp.iter().map(|c| brightness(*c)).collect();
        assert!(
            levels.windows(2).all(|w| w[0] < w[1]),
            "surface ramp must darken toward root: {levels:?}"
        );
    }

    #[test]
    fn signal_colors_are_distinct() {
        let theme = Theme::default();
        assert_ne!(theme.success, theme.warning);
        assert_ne!(theme.success, theme.danger);
        assert_ne!(theme.warning, theme.danger);
        assert_ne!(theme.accent, theme.accent2);
    }
}
