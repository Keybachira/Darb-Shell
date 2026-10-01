//! Model catalogue: what models exist and what they are good at (P2
//! step 1).
//!
//! Why a file and not a list in the binary: adding a model must not
//! require recompiling Darb. The catalogue is human-edited TOML, the
//! same way the rest of the human configuration is (Stacks §26), and an
//! unknown model is therefore a config error the user can see and fix —
//! not a panic in a release build.
//!
//! Nothing here talks to a provider. This module describes models; the
//! router in `crate::router` picks between them.

use std::path::Path;

use darb_core::errors::{DarbError, Result};
use serde::{Deserialize, Serialize};

/// What a model is being asked to do.
///
/// Four values, deliberately. The plan says this is the vocabulary of
/// routing, and more would mean more states than the user can predict —
/// the failure mode of every "smart" router. `Quick` and `Code` are the
/// common cases, `Reason` is planning, `Long` is a big file walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    /// Planning and multi-step reasoning before any tool runs.
    Reason,
    /// Editing and reviewing code.
    Code,
    /// Cheap, high-volume: summaries, labels, file triage.
    Quick,
    /// Large-context work — reading many files in one prompt.
    Long,
}

impl TaskKind {
    pub const ALL: [TaskKind; 4] = [
        TaskKind::Reason,
        TaskKind::Code,
        TaskKind::Quick,
        TaskKind::Long,
    ];

    /// Short label for the UI: the context panel shows *why* a model was
    /// chosen (`code → local (rápido)`), not just its name (P2 step 5).
    pub fn label(self) -> &'static str {
        match self {
            TaskKind::Reason => "reason",
            TaskKind::Code => "code",
            TaskKind::Quick => "quick",
            TaskKind::Long => "long",
        }
    }
}

/// Relative cost. Not a price: it orders "cheapest" against "pricier" so
/// a config can say "this is my frugal option" without quoting a price
/// that goes stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CostTier {
    Free,
    Low,
    Medium,
    High,
}

/// Relative speed, same reasoning as [`CostTier`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeedTier {
    Slow,
    Medium,
    Fast,
}

/// One entry in the catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    /// The id sent to the provider. The only field the wire sees.
    pub id: String,
    /// Which adapter serves it, matching `ProviderConfig::name`.
    pub provider: String,
    #[serde(default = "default_cost")]
    pub cost: CostTier,
    #[serde(default = "default_speed")]
    pub speed: SpeedTier,
    /// What this model is good at. Empty means "a generalist": usable for
    /// any task, which is a real choice and not a missing one.
    #[serde(default)]
    pub strengths: Vec<TaskKind>,
    /// Whether it runs on this machine. A local model beats a remote one
    /// for the same task because there is no round trip — on the E2-1800
    /// that is often the whole latency budget (P2 step 4).
    #[serde(default)]
    pub local: bool,
}

fn default_cost() -> CostTier {
    CostTier::Medium
}

fn default_speed() -> SpeedTier {
    SpeedTier::Medium
}

impl CostTier {
    pub fn as_str(self) -> &'static str {
        match self {
            CostTier::Free => "free",
            CostTier::Low => "low",
            CostTier::Medium => "medium",
            CostTier::High => "high",
        }
    }
}

impl SpeedTier {
    pub fn as_str(self) -> &'static str {
        match self {
            SpeedTier::Slow => "slow",
            SpeedTier::Medium => "medium",
            SpeedTier::Fast => "fast",
        }
    }
}

impl Model {
    /// Can this model serve `kind`? A model with no declared strengths is
    /// a generalist and answers for every kind; otherwise it has to name
    /// the kind explicitly. This is the whole matching rule, and it is a
    /// function of data so it is exhaustively tested.
    pub fn handles(&self, kind: TaskKind) -> bool {
        self.strengths.is_empty() || self.strengths.contains(&kind)
    }

    /// A short reason this model fits, for the "why" the UI shows. It
    /// states the rule that was actually applied rather than a marketing
    /// line, so the explanation cannot drift from the decision.
    pub fn reason(&self, kind: TaskKind) -> String {
        let basis = if self.strengths.is_empty() {
            "generalist".to_string()
        } else {
            format!("{} specialist", kind.label())
        };
        let local = if self.local { ", local" } else { "" };
        format!(
            "{}: {basis} · {}/{}{local}",
            kind.label(),
            self.speed.as_str(),
            self.cost.as_str()
        )
    }
}

/// The parsed catalogue.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalogue {
    /// Models in preference order. The router walks this order and takes
    /// the first match, so the order in the file is the user's policy —
    /// no hidden scoring, no tie-break surprises.
    #[serde(default)]
    pub models: Vec<Model>,
}

impl Catalogue {
    pub fn parse(text: &str) -> Result<Self> {
        toml::from_str(text)
            .map_err(|e| DarbError::Config(format!("failed to parse the model catalogue: {e}")))
    }

    /// Load from disk, with the path in the error so a wrong path is
    /// obvious (Contribuição §13).
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| {
            DarbError::Config(format!(
                "could not read the model catalogue '{}': {e}",
                path.display()
            ))
        })?;
        Self::parse(&text)
    }

    pub fn get(&self, id: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[[models]]
id = "local-small"
provider = "ollama"
cost = "free"
speed = "fast"
strengths = ["quick"]
local = true

[[models]]
id = "remote-code"
provider = "openai"
cost = "high"
speed = "medium"
strengths = ["code", "reason"]

[[models]]
id = "generalist"
provider = "openai"
"#;

    /// The file is the contract between the user and the router, so the
    /// exact shape the docs promise has to parse.
    #[test]
    fn parses_the_documented_shape() {
        let c = Catalogue::parse(SAMPLE).expect("parses");
        assert_eq!(c.models.len(), 3);
        let local = c.get("local-small").expect("present");
        assert!(local.local);
        assert_eq!(local.cost, CostTier::Free);
        assert_eq!(local.speed, SpeedTier::Fast);
        assert!(local.handles(TaskKind::Quick));
        assert!(!local.handles(TaskKind::Code));
    }

    /// Omitted fields take the documented defaults, so a two-line entry
    /// is a valid entry. Otherwise every model in the file would need
    /// boilerplate nobody maintains.
    #[test]
    fn omitted_fields_use_defaults() {
        let c = Catalogue::parse(SAMPLE).expect("parses");
        let g = c.get("generalist").expect("present");
        assert_eq!(g.cost, CostTier::Medium);
        assert_eq!(g.speed, SpeedTier::Medium);
        assert!(!g.local);
        assert!(g.strengths.is_empty());
    }

    /// An empty file is legal and means "nothing configured" — it must
    /// not fail to parse, so the router can report a clear message
    /// instead of the app dying on a config read.
    #[test]
    fn empty_catalogue_parses() {
        assert!(Catalogue::parse("").expect("parses").is_empty());
    }

    /// A typo in the file is a config error naming the file's content,
    /// not a panic and not a silently empty catalogue.
    #[test]
    fn a_bad_file_is_an_error_not_an_empty_catalogue() {
        let err = Catalogue::parse("[[models]]\nid = 5\n").unwrap_err();
        assert!(err.to_string().contains("model catalogue"), "{err}");
    }

    /// A missing file names its path, so the fix is obvious.
    #[test]
    fn a_missing_file_names_its_path() {
        let err = Catalogue::load("definitely/not/here.toml").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("definitely/not/here.toml"), "{text}");
    }

    /// The shipped `configs/models.toml` must parse. A default config
    /// that does not load would break the app on first run, and a test
    /// is the only thing that notices.
    #[test]
    fn the_shipped_catalogue_loads() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../configs/models.toml");
        let c = Catalogue::load(path).unwrap_or_else(|e| panic!("{e}"));
        assert!(!c.is_empty(), "the shipped catalogue is empty");
        // It must at least cover every kind, or some task can never
        // route without the user editing the file first.
        for kind in TaskKind::ALL {
            assert!(
                c.models.iter().any(|m| m.handles(kind)),
                "the shipped catalogue cannot route {kind:?}"
            );
        }
    }
}
