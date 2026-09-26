// errors — typed errors (Arquitetura §49). User-facing messages go through i18n.
//
// `Display` is written by hand instead of deriving it with `thiserror`:
// a single enum with nine string variants does not justify a proc-macro
// dependency in the core (Docs/design.md §2). Keep this in sync if a
// variant is added or renamed.
use std::fmt;

#[derive(Debug)]
pub enum DarbError {
    Config(String),
    Provider(String),
    Context(String),
    Tool(String),
    Permission(String),
    Storage(String),
    Git(String),
    Network(String),
    Internal(String),
}

impl DarbError {
    /// The category prefix, e.g. `config`. Callers that need to branch on
    /// the kind of failure (retry, ask the user, give up) match on the enum
    /// variant, never on this string.
    fn kind(&self) -> &'static str {
        match self {
            Self::Config(_) => "config",
            Self::Provider(_) => "provider",
            Self::Context(_) => "context",
            Self::Tool(_) => "tool",
            Self::Permission(_) => "permission",
            Self::Storage(_) => "storage",
            Self::Git(_) => "git",
            Self::Network(_) => "network",
            Self::Internal(_) => "internal",
        }
    }

    /// The detail message without the category prefix.
    fn detail(&self) -> &str {
        match self {
            Self::Config(m)
            | Self::Provider(m)
            | Self::Context(m)
            | Self::Tool(m)
            | Self::Permission(m)
            | Self::Storage(m)
            | Self::Git(m)
            | Self::Network(m)
            | Self::Internal(m) => m,
        }
    }
}

impl fmt::Display for DarbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} error: {}", self.kind(), self.detail())
    }
}

impl std::error::Error for DarbError {}

pub type Result<T> = std::result::Result<T, DarbError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_keeps_kind_and_detail() {
        assert_eq!(
            DarbError::Config("bad toml".into()).to_string(),
            "config error: bad toml"
        );
        assert_eq!(
            DarbError::Network("timeout".into()).to_string(),
            "network error: timeout"
        );
    }

    #[test]
    fn every_variant_has_a_kind_and_detail() {
        let all = [
            DarbError::Config("a".into()),
            DarbError::Provider("a".into()),
            DarbError::Context("a".into()),
            DarbError::Tool("a".into()),
            DarbError::Permission("a".into()),
            DarbError::Storage("a".into()),
            DarbError::Git("a".into()),
            DarbError::Network("a".into()),
            DarbError::Internal("a".into()),
        ];
        for error in all {
            assert!(error.to_string().ends_with("error: a"), "{error}");
        }
    }
}
