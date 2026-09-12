//! Agent state machine (Agents §§8–10).
//!
//! The canonical states live in `darb-core::events` (re-exported here so
//! the TUI and the Agent name the same states). This module adds the only
//! two things the loop needs: which states are terminal, and which
//! transitions are legal — every transition emits `AgentStateChanged`.

use darb_core::events::AgentState as State;

/// True for states the loop never leaves: [`State::Completed`],
/// [`State::Failed`], [`State::Cancelled`].
pub fn is_terminal(state: &State) -> bool {
    matches!(state, State::Completed | State::Failed | State::Cancelled)
}

/// Legal transitions of the main loop. Anything else is a bug in the
/// loop, and the loop asserts on it in tests.
pub fn is_legal_transition(from: &State, to: &State) -> bool {
    use State::*;
    matches!(
        (from, to),
        (Idle, Analyzing)
            | (Analyzing, Planning)
            | (Planning, WaitingProvider)
            | (WaitingProvider, Executing)
            | (WaitingProvider, Completed)
            | (WaitingProvider, Failed)
            | (Executing, Reviewing)
            | (Executing, WaitingPermission)
            | (WaitingPermission, Executing)
            | (Reviewing, Completed)
            | (Reviewing, Retrying)
            | (Reviewing, Failed)
            | (Retrying, Executing)
            | (Retrying, WaitingProvider)
            | (_, Failed)
            | (_, Cancelled)
            | (Cancelled, Idle)
            | (Completed, Idle)
            | (Failed, Idle)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_states() {
        assert!(is_terminal(&State::Completed));
        assert!(is_terminal(&State::Failed));
        assert!(is_terminal(&State::Cancelled));
        assert!(!is_terminal(&State::Executing));
    }

    #[test]
    fn happy_path_is_legal() {
        let path = [
            (State::Idle, State::Analyzing),
            (State::Analyzing, State::Planning),
            (State::Planning, State::WaitingProvider),
            (State::WaitingProvider, State::Executing),
            (State::Executing, State::Reviewing),
            (State::Reviewing, State::Completed),
        ];
        for (from, to) in path {
            assert!(is_legal_transition(&from, &to), "{from:?} -> {to:?}");
        }
    }

    #[test]
    fn skips_are_illegal() {
        assert!(!is_legal_transition(&State::Idle, &State::Executing));
        assert!(!is_legal_transition(&State::Planning, &State::Reviewing));
    }
}
