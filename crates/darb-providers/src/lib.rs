//! darb-providers — common Provider trait + adapters (§22).
//! All HTTP goes through reqwest behind the adapter.

pub mod catalog;
pub mod interface;
pub mod openai;
pub mod router;

pub use catalog::{Catalogue, CostTier, Model, SpeedTier, TaskKind};
pub use router::{route, Budget, Choice, RouteError};
