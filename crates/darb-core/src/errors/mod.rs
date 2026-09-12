// errors — typed errors (Arquitetura §49). User-facing messages go through i18n.
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DarbError {
    #[error("config error: {0}")]
    Config(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("context error: {0}")]
    Context(String),
    #[error("tool error: {0}")]
    Tool(String),
    #[error("permission error: {0}")]
    Permission(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("git error: {0}")]
    Git(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, DarbError>;
