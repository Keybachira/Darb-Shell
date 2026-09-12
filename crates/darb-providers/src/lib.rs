//! darb-providers — common Provider trait + adapters (§22).
//! All HTTP goes through reqwest behind the adapter.

pub mod anthropic;
pub mod custom;
pub mod google;
pub mod interface;
pub mod ollama;
pub mod openai;
pub mod openrouter;
