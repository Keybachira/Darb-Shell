//! Background supervisor: the agent works a queue of tasks without the
//! user submitting each one (P1.5, "agente a tempo inteiro").
//!
//! What this is **not**, because the docs forbid the obvious version:
//!
//! - **Not a daemon.** Nothing here outlives the process. Closing the
//!   terminal kills the run (Arquitetura Â§4.2 forbids permanent
//!   background indexing).
//! - **Not a second agent loop.** [`darb_agent::Agent::run`] is still the
//!   only loop. The supervisor decides *what runs next*, never *how*.
//! - **Not always-on.** `AiMode::Autonomous` is the only mode allowed to
//!   hold a queue at all (WORKLOGIC Â§3: a feature earns its place by
//!   saving attention; a background agent nobody authorised is exactly
//!   the surprise the mode system exists to prevent).
//!
//! The kill-switch is [`Supervisor::stop`]. It is a flag rather than a
//! `JoinHandle::abort` on purpose: aborting mid-`edit_file` could leave a
//! half-written file, and the loop already checks a cancellation flag at
//! every iteration boundary. Stopping is therefore *cooperative*, and the
//! one place waiting is not safe â€” a provider call â€” is already covered
//! by the loop's own timeout.
//!
//! Everything below the supervisor is the same `EventBus` the TUI already
//! drains, so background progress renders with no new UI plumbing.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use darb_core::events::{DarbEvent, EventBus};
use darb_core::workspace::AiMode;

/// Pause between tasks, so a background run stays inside the "low
/// background activity" budget the Arquitetura doc sets (Â§4.2).
pub const IDLE_BEAT: Duration = Duration::from_secs(5);

/// Encode/decode [`AiMode`] through an atomic. The enum is already
/// `#[repr]`-free but fieldless and `Copy`, so a cast is exact today; the
/// match on the way out keeps it total if a variant is ever added.
fn to_u8(mode: AiMode) -> u8 {
    mode as u8
}

fn from_u8(raw: u8) -> AiMode {
    match raw {
        1 => AiMode::Assist,
        2 => AiMode::Autonomous,
        _ => AiMode::Mentor,
    }
}

/// Why the supervisor stopped handing out work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The user asked it to stop (the kill-switch).
    User,
    /// The AI mode left `Autonomous`. Background work stops the moment
    /// the user takes the power back â€” this is the important direction:
    /// dropping to `Mentor` must never leave edits running.
    ModeChanged(AiMode),
}

impl StopReason {
    /// Short, stable text for the footer. Not translated: this sits in a
    /// fixed-width slot, and mode names are already untranslated product
    /// vocabulary (see `AiMode::name`).
    pub fn label(self) -> &'static str {
        match self {
            StopReason::User => "stopped",
            StopReason::ModeChanged(_) => "paused",
        }
    }
}

/// Hands out tasks to the agent loop, one at a time, while the mode allows.
///
/// Deliberately free of I/O: it holds counters and a mode, so the rules
/// that matter (mode ceiling, kill-switch, ordering) are unit-testable
/// without a provider, a terminal, or a timer. The async driver is
/// [`Supervisor::drive`], the only part that needs a runtime.
pub struct Supervisor {
    mode: Arc<AtomicU8>,
    stopped: Arc<AtomicBool>,
    /// Tasks completed since start, for the footer. `u64` so a long
    /// session cannot wrap into a negative-looking number.
    done: Arc<AtomicU64>,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new(AiMode::default())
    }
}

impl Supervisor {
    pub fn new(mode: AiMode) -> Self {
        Self {
            mode: Arc::new(AtomicU8::new(to_u8(mode))),
            stopped: Arc::new(AtomicBool::new(false)),
            done: Arc::new(AtomicU64::new(0)),
        }
    }

    /// The mode the supervisor is currently willing to act under.
    pub fn mode(&self) -> AiMode {
        from_u8(self.mode.load(Ordering::SeqCst))
    }

    /// Follow the workspace mode. Returns why work was halted, or `None`
    /// when the new mode still allows it.
    ///
    /// Going from a permissive mode to a stricter one stops the queue;
    /// going the other way does **not** silently resume it. A user who
    /// dropped to `Mentor` because the agent was misbehaving has to say
    /// "go ahead" again â€” resuming on its own would be the surprise this
    /// whole design exists to prevent.
    pub fn set_mode(&self, mode: AiMode) -> Option<StopReason> {
        let previous = self.mode();
        self.mode.store(to_u8(mode), Ordering::SeqCst);
        if mode == AiMode::Autonomous {
            return None;
        }
        if previous == AiMode::Autonomous {
            self.stopped.store(true, Ordering::SeqCst);
            return Some(StopReason::ModeChanged(mode));
        }
        None
    }

    /// The kill-switch. Idempotent, and never resumes on its own.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    /// Clear the kill-switch. Re-arming while the mode is not
    /// `Autonomous` still refuses work: [`Self::can_run`] is the single
    /// gate, so no path skips it.
    pub fn restart(&self) {
        self.stopped.store(false, Ordering::SeqCst);
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// The single authorisation check. Every path to running a task goes
    /// through here, which is why `restart` cannot bypass the mode.
    pub fn can_run(&self) -> bool {
        !self.is_stopped() && self.mode() == AiMode::Autonomous
    }

    pub fn completed(&self) -> u64 {
        self.done.load(Ordering::SeqCst)
    }

    /// A cloneable handle to the same state, for the driving task. It
    /// reads the mode without holding the `Workspace`, which is pure and
    /// has no business crossing a task boundary.
    pub fn handle(&self) -> Handle {
        Handle {
            mode: Arc::clone(&self.mode),
            stopped: Arc::clone(&self.stopped),
            done: Arc::clone(&self.done),
        }
    }

    /// Run queued tasks one at a time until the queue empties or the
    /// supervisor is stopped. Delegates to the handle so the driving
    /// task and this owner share one implementation — the gate is the
    /// same function either way, which is the point.
    ///
    /// `run_one` is injected so this loop is testable with a recording
    /// closure instead of a real agent and a real provider.
    pub async fn drive<F, Fut>(&self, queue: Vec<String>, events: &EventBus, run_one: F)
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        self.handle().drive(queue, events, run_one).await
    }

    /// [`Self::drive`] with an explicit inter-task pause. The parameter
    /// exists so tests drain a queue without sleeping five seconds per
    /// task; production callers use `drive`.
    pub async fn drive_with_beat<F, Fut>(
        &self,
        queue: Vec<String>,
        events: &EventBus,
        beat: Duration,
        run_one: F,
    ) where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        self.handle()
            .drive_with_beat(queue, events, beat, run_one)
            .await
    }
}

/// Cheap clone of the supervisor's shared state.
#[derive(Clone)]
pub struct Handle {
    mode: Arc<AtomicU8>,
    stopped: Arc<AtomicBool>,
    done: Arc<AtomicU64>,
}

impl Handle {
    pub fn mode(&self) -> AiMode {
        from_u8(self.mode.load(Ordering::SeqCst))
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// The single authorisation check, mirrored from
    /// [`Supervisor::can_run`] so the driving task applies the same rule
    /// without owning the `Supervisor`.
    pub fn can_run(&self) -> bool {
        !self.is_stopped() && self.mode() == AiMode::Autonomous
    }

    /// Run queued tasks one at a time until the queue empties or the
    /// supervisor is stopped. This is the only place `Agent::run` is
    /// awaited, and the gate is re-checked *between* tasks, so a mode
    /// change lands within one task rather than one session.
    pub async fn drive<F, Fut>(&self, queue: Vec<String>, events: &EventBus, run_one: F)
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        self.drive_with_beat(queue, events, IDLE_BEAT, run_one)
            .await
    }

    /// [`Self::drive`] with an explicit inter-task pause, so tests can
    /// drain a queue without sleeping the production beat per task.
    pub async fn drive_with_beat<F, Fut>(
        &self,
        mut queue: Vec<String>,
        events: &EventBus,
        beat: Duration,
        mut run_one: F,
    ) where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        for task in queue.drain(..) {
            if !self.can_run() {
                let _ = events.emit(DarbEvent::BackgroundHalted {
                    reason: self.halt_label(),
                });
                return;
            }
            let _ = events.emit(DarbEvent::BackgroundTaskStarted { task: task.clone() });
            run_one(task).await;
            self.done.fetch_add(1, Ordering::SeqCst);
            if self.is_stopped() {
                // No waiting out the beat once stopped: the kill-switch has
                // to feel immediate, and the run is already cancelled.
                break;
            }
            if !beat.is_zero() {
                tokio::time::sleep(beat).await;
            }
        }
        if !self.is_stopped() {
            let _ = events.emit(DarbEvent::BackgroundIdle {
                completed: self.completed(),
            });
        }
    }

    /// The kill-switch, reachable from a running task: this is how a
    /// driver stops itself (or its siblings) without holding the
    /// `Supervisor`, which the task deliberately does not own.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    pub fn completed(&self) -> u64 {
        self.done.load(Ordering::SeqCst)
    }

    /// Which stop to report, distinguishing "the user pressed stop" from
    /// "the mode took the power back" â€” they look identical in the UI
    /// unless the text says which.
    fn halt_label(&self) -> &'static str {
        if self.is_stopped() {
            StopReason::User.label()
        } else {
            StopReason::ModeChanged(self.mode()).label()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule the whole design exists for: background work only ever
    /// happens in `Autonomous`. Mentor and Assist are refused even with
    /// the kill-switch clear.
    #[test]
    fn only_autonomous_may_run() {
        for mode in [AiMode::Mentor, AiMode::Assist] {
            let s = Supervisor::new(mode);
            assert!(!s.can_run(), "{mode:?} must not run in the background");
        }
        assert!(Supervisor::new(AiMode::Autonomous).can_run());
    }

    /// Dropping to a stricter mode halts the queue and says why.
    #[test]
    fn leaving_autonomous_halts() {
        let s = Supervisor::new(AiMode::Autonomous);
        assert_eq!(
            s.set_mode(AiMode::Mentor),
            Some(StopReason::ModeChanged(AiMode::Mentor))
        );
        assert!(s.is_stopped());
        assert!(!s.can_run());
    }

    /// Mentor -> Assist must not trip the halt: neither mode ever ran
    /// anything, so there is nothing to stop and no reason to claim the
    /// queue was interrupted.
    #[test]
    fn switching_between_restricted_modes_is_quiet() {
        let s = Supervisor::new(AiMode::Mentor);
        assert_eq!(s.set_mode(AiMode::Assist), None);
        assert!(!s.is_stopped());
    }

    /// Raising the mode must not resurrect a queue the user stopped:
    /// re-arming is an explicit act, not a side effect of a mode change.
    #[test]
    fn raising_mode_does_not_resume() {
        let s = Supervisor::new(AiMode::Autonomous);
        s.set_mode(AiMode::Mentor);
        assert!(s.set_mode(AiMode::Autonomous).is_none());
        assert!(s.is_stopped(), "a mode change alone must not resume");
        s.restart();
        assert!(s.can_run());
    }

    /// `restart` is not a bypass: outside `Autonomous` the gate still
    /// refuses, because `can_run` is the only authority.
    #[test]
    fn restart_cannot_bypass_the_mode() {
        let s = Supervisor::new(AiMode::Mentor);
        s.restart();
        assert!(!s.can_run());
    }

    #[test]
    fn stop_is_idempotent() {
        let s = Supervisor::new(AiMode::Autonomous);
        s.stop();
        s.stop();
        assert!(s.is_stopped());
    }

    /// A handle observes the same state as its supervisor, which is what
    /// lets the driving task check the mode without the `Workspace`.
    #[test]
    fn handle_mirrors_supervisor() {
        let s = Supervisor::new(AiMode::Autonomous);
        let h = s.handle();
        s.set_mode(AiMode::Mentor);
        assert!(h.is_stopped());
        assert_eq!(h.mode(), AiMode::Mentor);
    }

    /// A recorded task body, boxed so every closure in these tests shares
    /// one type regardless of what it captures.
    type Body = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;

    /// Count tasks a driver actually ran, so a test can assert on work
    /// that must *not* have happened.
    fn counter() -> (Arc<AtomicU64>, impl FnMut(String) -> Body) {
        let ran = Arc::new(AtomicU64::new(0));
        let sink = Arc::clone(&ran);
        (ran, move |_t: String| {
            let c = Arc::clone(&sink);
            Box::pin(async move {
                c.fetch_add(1, Ordering::SeqCst);
            })
        })
    }

    /// The drive loop honours the gate: stopped means not one queued task
    /// runs. This is the "no surprise edits" guarantee, tested.
    #[tokio::test]
    async fn stopped_queue_runs_nothing() {
        let s = Supervisor::new(AiMode::Autonomous);
        s.stop();
        let (ran, mut run_one) = counter();
        s.drive_with_beat(
            vec!["a".into(), "b".into()],
            &EventBus::default(),
            Duration::ZERO,
            &mut run_one,
        )
        .await;
        assert_eq!(ran.load(Ordering::SeqCst), 0);
        assert_eq!(s.completed(), 0);
    }

    /// The same holds when the mode is merely *not* Autonomous: the queue
    /// is refused without anyone pressing stop.
    #[tokio::test]
    async fn mentor_queue_runs_nothing() {
        let s = Supervisor::new(AiMode::Mentor);
        let (ran, mut run_one) = counter();
        s.drive_with_beat(
            vec!["a".into()],
            &EventBus::default(),
            Duration::ZERO,
            &mut run_one,
        )
        .await;
        assert_eq!(ran.load(Ordering::SeqCst), 0);
    }

    /// In Autonomous with nothing stopping it, the queue drains in order
    /// and the counter reflects the work actually done.
    #[tokio::test]
    async fn drains_queue_in_order() {
        let s = Supervisor::new(AiMode::Autonomous);
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        s.drive_with_beat(
            vec!["one".into(), "two".into()],
            &EventBus::default(),
            Duration::ZERO,
            move |t: String| {
                let v = Arc::clone(&sink);
                Box::pin(async move { v.lock().expect("lock").push(t) })
            },
        )
        .await;
        assert_eq!(*seen.lock().expect("lock"), vec!["one", "two"]);
        assert_eq!(s.completed(), 2);
    }

    /// Stopping mid-queue halts before the next task: the remaining work
    /// is dropped rather than run after the user asked it to stop.
    #[tokio::test]
    async fn stopping_mid_queue_drops_the_rest() {
        let s = Supervisor::new(AiMode::Autonomous);
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let stopper = s.handle();
        s.drive_with_beat(
            vec!["one".into(), "two".into(), "three".into()],
            &EventBus::default(),
            Duration::ZERO,
            move |t: String| {
                let v = Arc::clone(&sink);
                let h = stopper.clone();
                Box::pin(async move {
                    v.lock().expect("lock").push(t);
                    h.stop();
                })
            },
        )
        .await;
        assert_eq!(*seen.lock().expect("lock"), vec!["one"]);
        assert!(s.is_stopped());
    }

    /// Halting is announced with a reason a human can act on, so the
    /// footer never shows a bare stop of unknown origin. Both cases need
    /// something to actually halt: an `Autonomous` supervisor that nobody
    /// stopped runs its queue, which is the correct behaviour and is what
    /// `drains_queue_in_order` covers.
    #[tokio::test]
    async fn halt_reports_a_reason() {
        let stopped = Supervisor::new(AiMode::Autonomous);
        stopped.stop();
        for (supervisor, expected) in [
            (stopped, "stopped"),
            (Supervisor::new(AiMode::Mentor), "paused"),
        ] {
            let bus = EventBus::default();
            let mut rx = bus.subscribe();
            supervisor
                .drive_with_beat(vec!["t".into()], &bus, Duration::ZERO, |_t: String| async {
                })
                .await;
            match rx.try_recv().expect("a halt event") {
                DarbEvent::BackgroundHalted { reason } => assert_eq!(reason, expected),
                other => panic!("expected a halt, got {other:?}"),
            }
        }
    }
}
