//! Internationalization: UI strings in PT (default) with EN fallback.
//!
//! Why this shape: interface strings must not be scattered through the
//! code (§15 Contribuição). Code/identifiers stay in English (§16), the
//! `t!()` macro resolves the user-visible text at call time.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use crate::errors::{DarbError, Result};

const PT_TOML: &str = include_str!("locales/pt.toml");
const EN_TOML: &str = include_str!("locales/en.toml");

/// Supported UI locales. Default is Portuguese, fallback is English.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    Pt,
    En,
}

impl Locale {
    /// Parse `"pt"` / `"en"` (case-insensitive). Unknown values fall back to PT.
    pub fn parse(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "en" => Locale::En,
            _ => Locale::Pt,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Locale::Pt => "pt",
            Locale::En => "en",
        }
    }
}

/// Flatten `[section] key = value` into `"section.key" -> value`.
///
/// Strings are taken as parsed: going through `Value::to_string()` would
/// re-escape them, so a translation containing a newline (the CLI usage
/// text) or a quote would reach the UI with literal `\n` in it.
fn flatten(table: &toml::Table, prefix: &str, out: &mut HashMap<String, String>) {
    for (key, value) in table {
        let full = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            toml::Value::Table(inner) => flatten(inner, &full, out),
            toml::Value::String(text) => {
                out.insert(full, text.clone());
            }
            other => {
                out.insert(full, other.to_string());
            }
        }
    }
}

fn parse_locale_toml(text: &str, name: &str) -> Result<HashMap<String, String>> {
    let table: toml::Table = toml::from_str(text)
        .map_err(|e| DarbError::Config(format!("invalid locale '{name}': {e}")))?;
    let mut out = HashMap::new();
    flatten(&table, "", &mut out);
    Ok(out)
}

pub struct I18n {
    language: Locale,
    pt: HashMap<String, String>,
    en: HashMap<String, String>,
}

impl I18n {
    /// Load embedded locales for the given language.
    pub fn new(language: Locale) -> Result<Self> {
        Ok(Self {
            language,
            pt: parse_locale_toml(PT_TOML, "pt")?,
            en: parse_locale_toml(EN_TOML, "en")?,
        })
    }

    pub fn set_language(&mut self, language: Locale) {
        self.language = language;
    }

    pub fn language(&self) -> Locale {
        self.language
    }

    /// Resolve a key with `pt → en` fallback. Returns the key itself when
    /// missing everywhere, so the UI never renders an empty string.
    pub fn text<'a>(&'a self, key: &'a str) -> &'a str {
        let (primary, secondary) = match self.language {
            Locale::Pt => (&self.pt, &self.en),
            Locale::En => (&self.en, &self.pt),
        };
        primary
            .get(key)
            .or_else(|| secondary.get(key))
            .map(String::as_str)
            .unwrap_or(key)
    }
}

static GLOBAL: OnceLock<RwLock<I18n>> = OnceLock::new();

fn global() -> &'static RwLock<I18n> {
    GLOBAL.get_or_init(|| {
        // Embedded locales are validated by unit tests; if they ever fail
        // here, start empty (keys echo) instead of crashing the UI.
        RwLock::new(I18n::new(Locale::Pt).unwrap_or_else(|_| I18n {
            language: Locale::Pt,
            pt: HashMap::new(),
            en: HashMap::new(),
        }))
    })
}

/// Switch the global UI language.
pub fn set_language(language: Locale) {
    if let Ok(mut guard) = global().write() {
        guard.set_language(language);
    }
}

/// Re-parse embedded locales into the global instance. Fails only if the
/// embedded TOML is invalid (covered by tests).
pub fn init_global(language: Locale) -> Result<()> {
    let fresh = I18n::new(language)?;
    if let Ok(mut guard) = global().write() {
        *guard = fresh;
    }
    Ok(())
}

/// Resolve a key through the global instance (owned to avoid lock lifetimes).
pub fn global_text(key: &str) -> String {
    global()
        .read()
        .map(|guard| guard.text(key).to_string())
        .unwrap_or_else(|_| key.to_string())
}

/// Resolve a key, then fill `{name}` placeholders from `args`.
///
/// Unknown placeholders stay verbatim: a missing argument shows up as
/// `{name}` in the UI instead of silently collapsing to an empty string,
/// which is what makes a bad translation obvious (Contribuição §14).
pub fn format_text(text: &str, args: &[(&str, &str)]) -> String {
    let mut out = text.to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// [`global_text`] + [`format_text`] for the current language.
pub fn global_format(key: &str, args: &[(&str, &str)]) -> String {
    format_text(&global_text(key), args)
}

/// `t!("agent.thinking")` → user-visible string in the current language.
///
/// Placeholders are filled with named arguments, so translators control
/// word order: `t!("app.profile", name = "eco")`.
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::global_text($key)
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::global_format(
            $key,
            &[$( (stringify!($name), $value.to_string().as_str()) ),+],
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_parse_defaults_to_pt() {
        assert_eq!(Locale::parse("pt"), Locale::Pt);
        assert_eq!(Locale::parse("en"), Locale::En);
        assert_eq!(Locale::parse("xx"), Locale::Pt);
    }

    #[test]
    fn resolves_pt_by_default() {
        let i18n = I18n::new(Locale::Pt).expect("locales must parse");
        assert_eq!(i18n.text("agent.thinking"), "A pensar...");
    }

    /// Build an instance from raw TOML, bypassing the embedded locales.
    fn from_toml(pt: &str, en: &str, language: Locale) -> I18n {
        I18n {
            language,
            pt: parse_locale_toml(pt, "pt").expect("pt must parse"),
            en: parse_locale_toml(en, "en").expect("en must parse"),
        }
    }

    #[test]
    fn falls_back_to_en_when_pt_missing() {
        let i18n = from_toml(
            "[a]\npt_only = \"PT\"",
            "[a]\npt_only = \"PT\"\nen_only = \"EN\"",
            Locale::Pt,
        );
        assert_eq!(i18n.text("a.en_only"), "EN");
        assert_eq!(i18n.text("a.pt_only"), "PT");
    }

    /// Both shipped locales must define the same keys: the en fallback is
    /// a safety net, not a place to park untranslated strings.
    #[test]
    fn shipped_locales_have_the_same_keys() {
        let i18n = I18n::new(Locale::Pt).expect("locales must parse");
        let mut missing_in_pt: Vec<&str> = i18n
            .en
            .keys()
            .filter(|key| !i18n.pt.contains_key(*key))
            .map(String::as_str)
            .collect();
        let mut missing_in_en: Vec<&str> = i18n
            .pt
            .keys()
            .filter(|key| !i18n.en.contains_key(*key))
            .map(String::as_str)
            .collect();
        missing_in_pt.sort_unstable();
        missing_in_en.sort_unstable();
        assert!(
            missing_in_pt.is_empty(),
            "missing in pt.toml: {missing_in_pt:?}"
        );
        assert!(
            missing_in_en.is_empty(),
            "missing in en.toml: {missing_in_en:?}"
        );
    }

    #[test]
    fn multi_line_values_keep_their_newlines() {
        let i18n = I18n::new(Locale::Pt).expect("locales must parse");
        let usage = i18n.text("cli.usage");
        assert!(usage.contains('\n'), "{usage:?}");
        assert!(usage.contains("darb doctor"), "{usage:?}");
    }

    #[test]
    fn placeholders_are_filled_and_unknown_ones_stay() {
        assert_eq!(
            format_text("Perfil: {name}", &[("name", "eco")]),
            "Perfil: eco"
        );
        assert_eq!(format_text("a {x} b", &[]), "a {x} b");
    }

    #[test]
    fn macro_fills_named_arguments() {
        init_global(Locale::Pt).expect("init must succeed");
        assert_eq!(crate::t!("app.profile", name = "eco"), "Perfil: eco");
        assert_eq!(
            crate::t!("agent.tool_failed", tool = "shell"),
            "shell falhou"
        );
    }

    #[test]
    fn unknown_key_echoes() {
        let i18n = I18n::new(Locale::En).expect("locales must parse");
        assert_eq!(i18n.text("does.not.exist"), "does.not.exist");
    }

    #[test]
    fn global_text_and_macro_agree() {
        init_global(Locale::En).expect("init must succeed");
        assert_eq!(global_text("agent.thinking"), "Thinking...");
        assert_eq!(crate::t!("agent.thinking"), "Thinking...");
        set_language(Locale::Pt);
        assert_eq!(crate::t!("agent.thinking"), "A pensar...");
    }
}
