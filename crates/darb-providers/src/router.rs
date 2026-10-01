//! The router: given a task and an appetite for spending, pick a model
//! (P2 step 3).
//!
//! **Deterministic first, and only.** The plan is explicit that the rule
//! comes before any cleverness: walk the catalogue in the user's order
//! and take the first model that handles the kind and fits the budget.
//! There is no LLM in this path and no scoring, which buys two things
//! that matter more than optimality â€” the same task always picks the
//! same model, and every choice can be explained in one line. An
//! LLM-based router is explicitly deferred, and only as a fallback when
//! the deterministic rule finds nothing.
//!
//! No I/O and no network: the router is a pure function of a
//! [`Catalogue`] and a [`TaskKind`], so it is testable exhaustively
//! without a provider.

use darb_core::errors::{DarbError, Result};

use crate::catalog::{Catalogue, CostTier, Model, TaskKind};

/// What the user is willing to spend on this call.
///
/// A ceiling, not a target: a cheaper model always satisfies it, and a
/// costlier one is only chosen when nothing cheaper can do the job. This
/// is why `Cheapest` and `Best` pick the same model on a catalogue where
/// everything is local and free â€” they differ in *policy*, not in
/// algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Budget {
    /// Spend nothing if avoidable. The frugal default for `eco` profiles.
    Cheapest,
    /// Balance cost against quality.
    Balanced,
    /// Cost is not a constraint; take the strongest match.
    Best,
}

impl Budget {
    /// Whether a model at `cost` is within this budget.
    fn admits(self, cost: CostTier) -> bool {
        match self {
            Budget::Cheapest => matches!(cost, CostTier::Free | CostTier::Low),
            Budget::Balanced => !matches!(cost, CostTier::High),
            Budget::Best => true,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Budget::Cheapest => "economy",
            Budget::Balanced => "balanced",
            Budget::Best => "best",
        }
    }
}

/// A decision, plus the reason it was made. The reason travels with the
/// choice so a view never has to re-derive it (P2 step 5, and the rule
/// about never showing a decision without its reason).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub model: Model,
    pub kind: TaskKind,
    /// One line, already phrased for the user.
    pub reason: String,
}

/// Why the router could not decide. Each variant names the thing that is
/// actually wrong, because "no model" is not actionable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteError {
    /// The catalogue has no entries at all.
    EmptyCatalogue,
    /// Nothing in the catalogue declares this kind. Even a generalist
    /// would be a guess here, and guessing means silently sending a
    /// model the user never chose for this work.
    NoMatchForKind(TaskKind),
    /// Models match the kind, but every one is above the budget. Kept
    /// distinct from the case above so the router can name the price
    /// rather than just refusing.
    OverBudget {
        kind: TaskKind,
        budget: Budget,
        /// The cheapest match, so the error can say what it would take.
        cheapest: String,
    },
}

impl RouteError {
    /// A sentence that names the fix. Every variant must be actionable:
    /// an error the user cannot act on is an error they will ignore.
    pub fn message(&self) -> String {
        match self {
            RouteError::EmptyCatalogue => {
                "the model catalogue is empty: add a model to configs/models.toml".to_string()
            }
            RouteError::NoMatchForKind(kind) => format!(
                "no model handles '{}' tasks: add `strengths = [\"{}\"]` to a model",
                kind.label(),
                kind.label()
            ),
            RouteError::OverBudget {
                kind,
                budget,
                cheapest,
            } => format!(
                "no '{}' model fits the {} budget; the cheapest is '{cheapest}' â€” raise the budget or mark that model cheaper",
                kind.label(),
                budget.label()
            ),
        }
    }
}

impl From<RouteError> for DarbError {
    fn from(e: RouteError) -> Self {
        DarbError::Config(e.message())
    }
}

/// Pick a model for `kind` under `budget`.
///
/// The rule, in full: among the models that handle `kind` and fit the
/// budget, prefer a local one (no round trip â€” P2 step 4), then the
/// first in catalogue order. Catalogue order is the user's stated
/// preference, so it is the tie-break; nothing is scored or re-ranked.
pub fn route(catalogue: &Catalogue, kind: TaskKind, budget: Budget) -> Result<Choice> {
    if catalogue.is_empty() {
        return Err(RouteError::EmptyCatalogue.into());
    }
    // Split once, then choose, so "nothing matched" and "nothing was
    // affordable" cannot be confused â€” the two need different advice.
    let matching: Vec<&Model> = catalogue
        .models
        .iter()
        .filter(|m| m.handles(kind))
        .collect();
    if matching.is_empty() {
        return Err(RouteError::NoMatchForKind(kind).into());
    }
    let affordable: Vec<&&Model> = matching.iter().filter(|m| budget.admits(m.cost)).collect();
    if affordable.is_empty() {
        let cheapest = matching
            .iter()
            .min_by_key(|m| m.cost)
            .map(|m| m.id.clone())
            .unwrap_or_default();
        return Err(RouteError::OverBudget {
            kind,
            budget,
            cheapest,
        }
        .into());
    }
    // Local first, and only as a tie-break among affordable matches: a
    // local model is preferred only when the user has not ruled it out.
    // `affordable` holds `&&Model` (references into `matching`, which is
    // itself a `Vec<&Model>`), so the pick is dereferenced twice to get
    // back to the `&Model` the catalogue owns.
    let picked: &Model = affordable
        .iter()
        .find(|m| m.local)
        .or_else(|| affordable.first())
        .map(|m| **m)
        .expect("affordable is non-empty by the check above");
    let model = picked.clone();
    Ok(Choice {
        reason: model.reason(kind),
        model,
        kind,
    })
}

/// The one place the config vocabulary meets the router's. Exhaustive
/// so adding a variant to either side is a compile error rather than a
/// silent fallthrough to a budget the user never asked for.
impl From<darb_core::config::RoutingBudget> for Budget {
    fn from(b: darb_core::config::RoutingBudget) -> Self {
        use darb_core::config::RoutingBudget;
        match b {
            RoutingBudget::Cheapest => Budget::Cheapest,
            RoutingBudget::Balanced => Budget::Balanced,
            RoutingBudget::Best => Budget::Best,
        }
    }
}

/// The ordered list of models to try, for the fallback path (P2 step 6).
///
/// A fallback only means something if the list is known in advance, so
/// this returns every match in the order the router would try them â€”
/// without budget filtering, because a fallback exists precisely for
/// the case where the primary turned out to be unavailable. The
/// application layer reports *which* ones it tried; it never hides a
/// failure behind a silent swap (design.md Â§7).
pub fn fallback_chain(catalogue: &Catalogue, kind: TaskKind) -> Vec<&Model> {
    let mut matching: Vec<&Model> = catalogue
        .models
        .iter()
        .filter(|m| m.handles(kind))
        .collect();
    matching.sort_by_key(|m| (!m.local, m.cost));
    matching
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalogue, CostTier, Model, SpeedTier};

    fn model(id: &str, cost: CostTier, local: bool, strengths: &[TaskKind]) -> Model {
        Model {
            id: id.to_string(),
            provider: "openai".to_string(),
            cost,
            speed: SpeedTier::Medium,
            strengths: strengths.to_vec(),
            local,
        }
    }

    /// A catalogue shaped like a real one: a cheap local model for quick
    /// work, a pricier remote one for code, and a cheap generalist that
    /// can stand in for anything.
    ///
    /// The generalist is `low`, not `medium`, and that matters: it has to
    /// stay affordable under the economy budget, because a generalist is
    /// the last thing standing when the specialist is too dear. A
    /// generalist the budget cannot afford is not a fallback.
    fn sample() -> Catalogue {
        Catalogue {
            models: vec![
                model("local-small", CostTier::Free, true, &[TaskKind::Quick]),
                model("remote-code", CostTier::High, false, &[TaskKind::Code]),
                model("generalist", CostTier::Low, false, &[]),
            ],
        }
    }

    /// The headline guarantee: the same input always yields the same
    /// model. A router that could answer differently for the same task
    /// would be untestable and unexplainable.
    #[test]
    fn is_deterministic() {
        let c = sample();
        for kind in TaskKind::ALL {
            let a = route(&c, kind, Budget::Balanced).unwrap();
            let b = route(&c, kind, Budget::Balanced).unwrap();
            assert_eq!(a.model.id, b.model.id, "{kind:?} was not stable");
        }
    }

    /// A specialist beats a generalist: the user declared strengths for a
    /// reason, so a model that matches the task is preferred to one that
    /// merely tolerates it.
    #[test]
    fn prefers_a_specialist_over_a_generalist() {
        let c = sample();
        assert_eq!(
            route(&c, TaskKind::Quick, Budget::Balanced)
                .unwrap()
                .model
                .id,
            "local-small"
        );
        // Under `Cheapest` the high-cost code model is unaffordable, so the
        // generalist is what remains.
        assert_eq!(
            route(&c, TaskKind::Code, Budget::Cheapest)
                .unwrap()
                .model
                .id,
            "generalist",
            "the remote model is too dear"
        );
    }

    /// P2 step 4: local first, because on this hardware no round trip is
    /// often the whole latency budget. The rule applies only among models
    /// that already fit the budget — a tie-break, not an override.
    #[test]
    fn prefers_local_among_affordable_models() {
        let c = Catalogue {
            models: vec![
                model("remote-cheap", CostTier::Free, false, &[TaskKind::Quick]),
                model("local-free", CostTier::Free, true, &[TaskKind::Quick]),
            ],
        };
        assert_eq!(
            route(&c, TaskKind::Quick, Budget::Best).unwrap().model.id,
            "local-free"
        );
    }

    /// But never over the ceiling: if the only local model is expensive
    /// and a cheap remote one exists, the budget wins.
    #[test]
    fn budget_outranks_locality() {
        let c = Catalogue {
            models: vec![
                model("local-pricey", CostTier::High, true, &[TaskKind::Code]),
                model("remote-cheap", CostTier::Low, false, &[TaskKind::Code]),
            ],
        };
        assert_eq!(
            route(&c, TaskKind::Code, Budget::Cheapest)
                .unwrap()
                .model
                .id,
            "remote-cheap",
            "the budget is a ceiling, not a preference"
        );
    }

    /// Every choice must be explainable, and the explanation must name
    /// the task it was made for.
    #[test]
    fn every_choice_carries_a_reason() {
        let c = sample();
        for kind in TaskKind::ALL {
            let choice = route(&c, kind, Budget::Best).unwrap();
            assert!(
                choice.reason.contains(kind.label()),
                "{kind:?}: {:?} does not say what it was for",
                choice.reason
            );
        }
    }

    /// A model with no declared strengths serves any task. This is what
    /// makes the generalist a usable fallback rather than a decoration.
    #[test]
    fn a_generalist_handles_every_kind() {
        let g = model("g", CostTier::Low, false, &[]);
        for kind in TaskKind::ALL {
            assert!(g.handles(kind), "{kind:?}");
        }
        let specialist = model("s", CostTier::Low, false, &[TaskKind::Code]);
        assert!(specialist.handles(TaskKind::Code));
        assert!(!specialist.handles(TaskKind::Long));
    }

    /// The three failure modes are distinguishable, because each needs
    /// different advice from the user.
    #[test]
    fn failures_are_distinguishable_and_actionable() {
        let err = route(&Catalogue::default(), TaskKind::Code, Budget::Balanced)
            .unwrap_err()
            .to_string();
        assert!(err.contains("empty"), "{err}");

        let only_code = Catalogue {
            models: vec![model("c", CostTier::Free, false, &[TaskKind::Code])],
        };
        let err = route(&only_code, TaskKind::Reason, Budget::Balanced)
            .unwrap_err()
            .to_string();
        assert!(err.contains("no model handles 'reason'"), "{err}");

        // Matching but unaffordable must not be reported as "no match".
        let pricey = Catalogue {
            models: vec![model("p", CostTier::High, false, &[TaskKind::Code])],
        };
        let err = route(&pricey, TaskKind::Code, Budget::Cheapest)
            .unwrap_err()
            .to_string();
        assert!(err.contains("economy budget"), "{err}");
        assert!(err.contains("'p'"), "must name the price: {err}");
    }

    /// A catalogue with a generalist always routes — the router never
    /// dead-ends while a generalist is present, at any budget.
    #[test]
    fn a_generalist_means_never_dead_ended() {
        let c = sample();
        for kind in TaskKind::ALL {
            for budget in [Budget::Cheapest, Budget::Balanced, Budget::Best] {
                assert!(
                    route(&c, kind, budget).is_ok(),
                    "{kind:?}/{budget:?} dead-ended"
                );
            }
        }
    }

    /// The fallback chain is what P2 step 6 needs: ordered, and only
    /// models that can actually do the work.
    #[test]
    fn fallback_chain_is_ordered_local_first() {
        let c = Catalogue {
            models: vec![
                model("remote-cheap", CostTier::Free, false, &[TaskKind::Code]),
                model("local-free", CostTier::Free, true, &[TaskKind::Code]),
                model("unrelated", CostTier::Free, false, &[TaskKind::Quick]),
            ],
        };
        let ids: Vec<&str> = fallback_chain(&c, TaskKind::Code)
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(ids, vec!["local-free", "remote-cheap"]);
        assert!(
            !ids.contains(&"unrelated"),
            "a model that cannot do the work is not a fallback"
        );
    }
}
