//! Planner: task → trackable [`Plan`].
//!
//! Honest scope for now: the loop creates a single-step plan from the
//! user task and marks progress as tools run. Multi-step decomposition
//! by the model is future work — this module owns the bookkeeping shape
//! so that future doesn't reshuffle the loop.

/// One step of a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    pub description: String,
    pub done: bool,
}

/// A task broken into steps the loop works through.
#[derive(Debug, Clone)]
pub struct Plan {
    pub goal: String,
    pub steps: Vec<PlanStep>,
}

impl Plan {
    /// A plan with a single step: the task itself. Enough for the MVP
    /// loop; the shape already supports finer breakdowns later.
    pub fn single(goal: impl Into<String>) -> Self {
        let goal = goal.into();
        Self {
            steps: vec![PlanStep {
                description: goal.clone(),
                done: false,
            }],
            goal,
        }
    }

    pub fn pending_count(&self) -> usize {
        self.steps.iter().filter(|step| !step.done).count()
    }

    pub fn mark_next_done(&mut self) {
        if let Some(step) = self.steps.iter_mut().find(|step| !step.done) {
            step.done = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_plan_tracks_progress() {
        let mut plan = Plan::single("fix login");
        assert_eq!(plan.pending_count(), 1);
        plan.mark_next_done();
        assert_eq!(plan.pending_count(), 0);
    }
}
