//! Event bus: components communicate through events, not direct calls.
//!
//! Why broadcast: the Agent stays independent of the TUI, plugins can
//! observe, and everything is testable (Arquitetura §8). Emitting never
//! spawns tasks and never blocks producers — slow consumers lag, they
//! don't stall the agent (low-memory / no-polling rules).

use tokio::sync::broadcast;

use crate::errors::{DarbError, Result};

/// Agent lifecycle states (Agents §9, Negócio §28).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    Idle,
    Analyzing,
    Planning,
    WaitingProvider,
    WaitingPermission,
    Executing,
    Reviewing,
    Retrying,
    Completed,
    Failed,
    Cancelled,
}

/// Domain events. Small and cloneable: receivers only borrow what they need.
#[derive(Debug, Clone)]
pub enum DarbEvent {
    AgentStarted {
        task: String,
    },
    AgentStateChanged {
        from: AgentState,
        to: AgentState,
    },
    ContextRequested {
        query: String,
    },
    ContextReady {
        files: usize,
    },
    ToolRequested {
        tool: String,
        target: String,
    },
    ToolCompleted {
        tool: String,
        success: bool,
    },
    PermissionRequested {
        tool: String,
        target: String,
    },
    PermissionResolved {
        tool: String,
        allowed: bool,
    },
    ReviewRequested {
        files: usize,
    },
    AgentFinished {
        summary: String,
    },
    ConfigReloaded,
    ErrorOccurred {
        message: String,
    },
    /// One streamed text fragment from the provider. The TUI appends it
    /// to the in-progress answer; the final text still arrives via the
    /// normal response path, so missing a delta only affects liveness.
    ProviderDelta {
        text: String,
    },
}

/// Default channel capacity: large enough for an agent burst
/// (state + tool + permission events per iteration), small enough
/// to stay far from the low-memory budget.
const DEFAULT_CAPACITY: usize = 64;

pub struct EventBus {
    sender: broadcast::Sender<DarbEvent>,
}

impl EventBus {
    /// Create a bus. A zero capacity would panic in broadcast, so it
    /// falls back to the default instead of crashing.
    pub fn new(capacity: usize) -> Self {
        let capacity = if capacity == 0 {
            DEFAULT_CAPACITY
        } else {
            capacity
        };
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Subscribe to events. Each subscriber drains at its own pace.
    pub fn subscribe(&self) -> broadcast::Receiver<DarbEvent> {
        self.sender.subscribe()
    }

    /// Emit an event. Succeeds even with zero subscribers (e.g. headless
    /// runs): nobody listening is a valid state, not an error.
    pub fn emit(&self, event: DarbEvent) -> Result<()> {
        match self.sender.send(event) {
            Ok(_) => Ok(()),
            Err(_) => Ok(()),
        }
    }

    /// Number of active subscribers (useful for tests/diagnostics).
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Convert a lag-overflow into a typed error for callers that `recv().await`.
    pub fn recv_error_message(e: broadcast::error::RecvError) -> DarbError {
        DarbError::Internal(format!("event bus receive failed: {e}"))
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emit_reaches_subscriber() {
        let bus = EventBus::default();
        let mut rx = bus.subscribe();
        bus.emit(DarbEvent::AgentStarted {
            task: "fix login".to_string(),
        })
        .expect("emit must succeed");
        let event = rx.recv().await.expect("must receive");
        assert!(matches!(event, DarbEvent::AgentStarted { .. }));
    }

    #[tokio::test]
    async fn emit_without_subscribers_is_ok() {
        let bus = EventBus::default();
        bus.emit(DarbEvent::ConfigReloaded).expect("must succeed");
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[test]
    fn zero_capacity_falls_back_to_default() {
        let bus = EventBus::new(0);
        bus.emit(DarbEvent::ConfigReloaded).expect("must succeed");
    }
}
