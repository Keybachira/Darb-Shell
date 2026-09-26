//! Workspace: the single source of truth about the open project (P0, §28
//! of the vision: "o Darb sabe qual o projecto está aberto, que ficheiros
//! estão activos, que erros apareceram, qual o estado do Git").
//!
//! Why it lives here and is pure: every renderer (TUI today, Slint later)
//! must read the same state, so it cannot live in either. It holds **no I/O**
//! — no `tokio`, no `fs`, no `git` — so a snapshot is constructible in a
//! test and comparable with `==`. `apps/darb` gathers; the Workspace
//! records; the views render. Nothing else decides.
//!
//! Two rules follow from that split:
//!
//! - **Measured, never invented.** An empty field means "not known yet",
//!   and views show `—` rather than zero (Contribuição §41).
//! - **One writer.** Only the application layer mutates a Workspace; a
//!   view that wants a change sends an intent, it does not edit state.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::{PermissionDecision, PermissionsConfig};

/// How much power the AI has (vision §§16–19).
///
/// This is a *power* axis, deliberately separate from `darb-tui`'s
/// `AgentMode`, which is a *task* axis (plan / code / debug). Keeping them
/// apart avoids 3×6 = 18 states for the user to choose between. P1 maps
/// this onto `PermissionDecision`; until then it is recorded, not obeyed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiMode {
    /// Observes and explain. Never touches code, never runs commands.
    #[default]
    Mentor,
    /// Proposes changes; the user applies them.
    Assist,
    /// Edits, runs and tests on its own.
    Autonomous,
}

impl AiMode {
    pub const ALL: [AiMode; 3] = [AiMode::Mentor, AiMode::Assist, AiMode::Autonomous];

    /// The name shown in the footer. These three are product vocabulary
    /// (the vision names them), so they are not translated.
    pub fn name(self) -> &'static str {
        match self {
            AiMode::Mentor => "MENTOR",
            AiMode::Assist => "ASSIST",
            AiMode::Autonomous => "AUTONOMOUS",
        }
    }

    fn next(self) -> Self {
        match self {
            AiMode::Mentor => AiMode::Assist,
            AiMode::Assist => AiMode::Autonomous,
            AiMode::Autonomous => AiMode::Mentor,
        }
    }

    /// The permission profile this mode implies (P1 step 1, vision §§16–19).
    ///
    /// A mode is a *ceiling*, not a suggestion: `Mentor` cannot write even
    /// if the user's config says `edit: allow`. That direction is the one
    /// that matters — a user who has loosened their config once should not
    /// then find the AI writing files in Mentor mode.
    ///
    /// The floor is unchanged in all three modes: `sudo` and `git push` stay
    /// denied (Negócio §§8–9). A mode can widen what is *askable*, never
    /// what is *forbidden*.
    pub fn permissions(self) -> PermissionsConfig {
        use PermissionDecision::{Allow, Ask, Deny};
        match self {
            // Observes and explains. Everything that touches the project is
            // refused outright rather than confirmed: asking "may I edit?"
            // in a mode whose promise is "I will not edit" is noise.
            AiMode::Mentor => PermissionsConfig {
                read: Allow,
                edit: Deny,
                shell: Deny,
                delete: Deny,
                git_push: Deny,
            },
            // Proposes changes; the user applies them.
            AiMode::Assist => PermissionsConfig {
                read: Allow,
                edit: Ask,
                shell: Ask,
                delete: Ask,
                git_push: Deny,
            },
            // Edits, runs and tests on its own. Delete still asks: losing a
            // file is not recoverable by re-running anything.
            AiMode::Autonomous => PermissionsConfig {
                read: Allow,
                edit: Allow,
                shell: Ask,
                delete: Ask,
                git_push: Deny,
            },
        }
    }

    /// Tighten `config` so it never grants more than this mode allows.
    ///
    /// Denials win over allows: the resulting decision for a category is
    /// the *stricter* of what the user configured and what the mode allows.
    /// Asking the model to behave is not a control — the permission gate is.
    pub fn apply_to(self, config: &mut PermissionsConfig) {
        use PermissionDecision::Deny;
        let ceiling = self.permissions();
        config.read = stricter(config.read, ceiling.read);
        config.edit = stricter(config.edit, ceiling.edit);
        config.shell = stricter(config.shell, ceiling.shell);
        config.delete = stricter(config.delete, ceiling.delete);
        config.git_push = Deny;
    }

    /// One line telling the model what this mode means (P1 step 3).
    ///
    /// Deliberately one sentence per mode. A longer prompt means the
    /// permission gate is being described twice, and the two copies drift.
    pub fn instruction(self) -> &'static str {
        match self {
            AiMode::Mentor => {
                "You are in MENTOR mode: explain and suggest only. Do not edit files or run commands."
            }
            AiMode::Assist => {
                "You are in ASSIST mode: propose changes, the user applies them. Never edit without confirmation."
            }
            AiMode::Autonomous => {
                "You are in AUTONOMOUS mode: edit, run and test on your own. Never use sudo or push."
            }
        }
    }
}

/// The stricter of two decisions (P1: a mode is a ceiling).
///
/// Order is Deny > Ask > Allow. Two `Deny`s are the same denial, and a
/// `grant_once` can never widen this — the grant only converts an `Ask`,
/// so a denied category stays denied whatever the user clicks.
fn stricter(a: PermissionDecision, b: PermissionDecision) -> PermissionDecision {
    use PermissionDecision::{Allow, Ask, Deny};
    match (a, b) {
        (Deny, _) | (_, Deny) => Deny,
        (Ask, _) | (_, Ask) => Ask,
        (Allow, Allow) => Allow,
    }
}

/// One running child process (a dev server, a test run). Registered, not
/// spawned: spawning is the application layer's job, and only behind a
/// permission check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningProcess {
    /// Stable identifier, used to correlate output with the process.
    pub id: String,
    /// The command line as the user sees it.
    pub command: String,
    /// Exit code once finished; `None` while still running.
    pub exit_code: Option<i32>,
}

impl RunningProcess {
    pub fn running(id: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            command: command.into(),
            exit_code: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.exit_code.is_none()
    }
}

/// The repository state, parsed from `git status --porcelain=v1 --branch`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitSnapshot {
    /// Current branch. Empty when there is no repository, which views
    /// render as "no section" rather than "branchless".
    pub branch: String,
    /// Files with an unstaged working-tree change.
    pub changes: usize,
    /// Files staged for the next commit.
    pub staged: usize,
    /// Untracked files.
    pub untracked: usize,
}

impl GitSnapshot {
    pub fn is_repo(&self) -> bool {
        !self.branch.is_empty()
    }

    /// Compact `+2 ~1 ?3` summary for the header. Empty when clean, so a
    /// clean tree shows the badge without a zero-filled string.
    pub fn summary(&self) -> String {
        if self.changes == 0 && self.staged == 0 && self.untracked == 0 {
            return String::new();
        }
        let mut parts = Vec::with_capacity(3);
        if self.staged > 0 {
            parts.push(format!("+{}", self.staged));
        }
        if self.changes > 0 {
            parts.push(format!("~{}", self.changes));
        }
        if self.untracked > 0 {
            parts.push(format!("?{}", self.untracked));
        }
        parts.join(" ")
    }

    /// Parse porcelain v1 output. The first line is `## branch…`; the rest
    /// are `XY path` or `?? path`. Unparseable input yields the default
    /// snapshot rather than an error: a missing repository is a valid
    /// state, not a failure.
    pub fn parse(output: &str) -> Self {
        let mut lines = output.lines();
        let head = lines.next().unwrap_or("");
        let branch = head
            .strip_prefix("## ")
            .map(|rest| {
                // Porcelain appends the upstream after `...` and, when the
                // branch is ahead/behind, a `[ahead N]` tag after that.
                // The branch name itself is what the header wants.
                let name = rest.split("...").next().unwrap_or(rest);
                name.split('[').next().unwrap_or(name).trim().to_string()
            })
            .unwrap_or_default();

        let mut staged = 0usize;
        let mut changes = 0usize;
        let mut untracked = 0usize;
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            match line.strip_prefix("?? ") {
                Some(path) if !path.is_empty() => untracked += 1,
                Some(_) => {}
                None => {
                    let codes = line.as_bytes();
                    if codes.len() >= 2 {
                        if codes[0] != b' ' {
                            staged += 1;
                        }
                        if codes[1] != b' ' {
                            changes += 1;
                        }
                    }
                }
            }
        }

        Self {
            branch,
            changes,
            staged,
            untracked,
        }
    }
}

/// A problem the toolchain reported about a file. Keyed by path in a
/// `BTreeMap` so a new message for a file replaces the old one instead of
/// stacking duplicates after every rebuild.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostics {
    by_file: BTreeMap<String, Vec<String>>,
}

impl Diagnostics {
    /// Record a message for `path`. An empty message clears that file's
    /// entry, which is how a resolved error disappears instead of
    /// lingering until the project is reindexed.
    pub fn set(&mut self, path: impl Into<String>, message: impl Into<String>) {
        let path = path.into();
        let message = message.into();
        if message.is_empty() {
            self.by_file.remove(&path);
            return;
        }
        self.by_file.insert(path, vec![message]);
    }

    pub fn clear(&mut self) {
        self.by_file.clear();
    }

    pub fn get(&self, path: &str) -> Option<&[String]> {
        self.by_file.get(path).map(|v| v.as_slice())
    }

    pub fn len(&self) -> usize {
        self.by_file.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_file.is_empty()
    }

    /// Every file with a known problem, ordered by path.
    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.by_file.keys()
    }
}

/// Everything both the TUI and the future desktop read. Cloneable and
/// comparable, so a view can diff two snapshots without reaching for the
/// source of truth again.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Workspace {
    /// Absolute path of the project. Empty until a root is adopted.
    pub project_root: PathBuf,
    /// Short name for the header; empty when the root has no final
    /// component to take.
    pub project_name: String,
    /// The file currently open in the editor, if any.
    pub open_file: String,
    /// Open files with unsaved edits, in insertion order.
    pub dirty_files: Vec<String>,
    /// The terminal the user is typing into.
    pub active_terminal: String,
    /// Running dev servers and test runs.
    pub processes: Vec<RunningProcess>,
    pub diagnostics: Diagnostics,
    pub git: GitSnapshot,
    pub mode: AiMode,
}

impl Workspace {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        let project_root = project_root.into();
        let project_name = project_root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            project_root,
            project_name,
            ..Self::default()
        }
    }

    /// The one place a file becomes "open". Re-opening a file that is
    /// already open keeps its dirty flag: clicking the entry again did not
    /// discard the buffer.
    pub fn open(&mut self, path: impl Into<String>) {
        let path = path.into();
        if !path.is_empty() {
            self.open_file = path;
        }
    }

    /// Close the open file. Dirty files are intentionally *kept*: closing
    /// without saving must not lose the warning that something is unsaved.
    pub fn close(&mut self) {
        self.open_file.clear();
    }

    pub fn mark_dirty(&mut self, path: impl Into<String>) {
        let path = path.into();
        if path.is_empty() || self.dirty_files.contains(&path) {
            return;
        }
        self.dirty_files.push(path);
    }

    /// Save clears the dirty flag for `path` — or for every file when
    /// `path` is empty, which is what "save all" means.
    pub fn mark_saved(&mut self, path: &str) {
        if path.is_empty() {
            self.dirty_files.clear();
        } else {
            self.dirty_files.retain(|p| p != path);
        }
    }

    pub fn is_dirty(&self, path: &str) -> bool {
        self.dirty_files.iter().any(|p| p == path)
    }

    /// Register a process, replacing any earlier entry with the same id so
    /// a restart does not append a second row for the same server.
    pub fn register_process(&mut self, process: RunningProcess) {
        match self.processes.iter_mut().find(|p| p.id == process.id) {
            Some(slot) => *slot = process,
            None => self.processes.push(process),
        }
    }

    /// Record a process exit. An unknown id is ignored: the process ended
    /// before the workspace ever heard of it.
    pub fn finish_process(&mut self, id: &str, exit_code: i32) {
        if let Some(process) = self.processes.iter_mut().find(|p| p.id == id) {
            process.exit_code = Some(exit_code);
        }
    }

    pub fn running_count(&self) -> usize {
        self.processes.iter().filter(|p| p.is_running()).count()
    }

    pub fn cycle_mode(&mut self) -> AiMode {
        self.mode = self.mode.next();
        self.mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> Workspace {
        Workspace::new("/home/edmil/darb-project")
    }

    // ── project root ──────────────────────────────────────────────

    #[test]
    fn new_takes_the_name_from_the_root() {
        let ws = workspace();
        assert_eq!(ws.project_name, "darb-project");
        assert_eq!(ws.project_root, PathBuf::from("/home/edmil/darb-project"));
    }

    #[test]
    fn default_has_no_project_and_no_invented_state() {
        let ws = Workspace::default();
        assert!(ws.project_name.is_empty());
        assert!(ws.open_file.is_empty());
        assert!(!ws.git.is_repo());
        assert!(ws.diagnostics.is_empty());
        // A fresh workspace must not look like a healthy one.
        assert_eq!(ws.git.summary(), "");
    }

    // ── open file / dirty files ───────────────────────────────────

    #[test]
    fn open_file_is_recorded() {
        let mut ws = workspace();
        ws.open("auth/login.ts");
        assert_eq!(ws.open_file, "auth/login.ts");
    }

    #[test]
    fn reopening_keeps_the_dirty_flag() {
        let mut ws = workspace();
        ws.open("auth/login.ts");
        ws.mark_dirty("auth/login.ts");
        ws.open("auth/login.ts");
        assert!(
            ws.is_dirty("auth/login.ts"),
            "reopen must not discard the buffer"
        );
    }

    #[test]
    fn empty_path_never_becomes_the_open_file() {
        let mut ws = workspace();
        ws.open("auth/login.ts");
        ws.open("");
        assert_eq!(ws.open_file, "auth/login.ts", "an empty path is a no-op");
    }

    #[test]
    fn close_keeps_unsaved_files_on_record() {
        let mut ws = workspace();
        ws.open("auth/login.ts");
        ws.mark_dirty("auth/login.ts");
        ws.close();
        assert!(ws.open_file.is_empty());
        assert_eq!(ws.dirty_files, vec!["auth/login.ts".to_string()]);
    }

    #[test]
    fn saving_one_file_leaves_the_others_dirty() {
        let mut ws = workspace();
        ws.mark_dirty("a.ts");
        ws.mark_dirty("b.ts");
        ws.mark_saved("a.ts");
        assert_eq!(ws.dirty_files, vec!["b.ts".to_string()]);
    }

    #[test]
    fn saving_with_no_path_clears_everything() {
        let mut ws = workspace();
        ws.mark_dirty("a.ts");
        ws.mark_dirty("b.ts");
        ws.mark_saved("");
        assert!(ws.dirty_files.is_empty());
    }

    #[test]
    fn marking_dirty_twice_does_not_duplicate() {
        let mut ws = workspace();
        ws.mark_dirty("a.ts");
        ws.mark_dirty("a.ts");
        assert_eq!(ws.dirty_files.len(), 1);
    }

    // ── processes ─────────────────────────────────────────────────

    // ── processes ─────────────────────────────────────────────────

    #[test]
    fn a_fresh_process_counts_as_running() {
        let mut ws = workspace();
        ws.register_process(RunningProcess::running("dev", "npm run dev"));
        assert!(ws.processes[0].is_running());
        assert_eq!(ws.running_count(), 1);
    }

    #[test]
    fn re_registering_the_same_id_replaces_it() {
        let mut ws = workspace();
        ws.register_process(RunningProcess::running("dev", "npm run dev"));
        ws.register_process(RunningProcess::running("dev", "npm run dev --port 3000"));
        assert_eq!(ws.processes.len(), 1, "a restart must not append a row");
        assert_eq!(ws.processes[0].command, "npm run dev --port 3000");
    }

    #[test]
    fn finishing_records_the_exit_code() {
        let mut ws = workspace();
        ws.register_process(RunningProcess::running("test", "npm test"));
        ws.finish_process("test", 1);
        assert_eq!(ws.processes[0].exit_code, Some(1));
        assert!(!ws.processes[0].is_running());
        assert_eq!(ws.running_count(), 0);
    }

    #[test]
    fn finishing_an_unknown_process_is_ignored() {
        let mut ws = workspace();
        ws.finish_process("never-registered", 0);
        assert!(ws.processes.is_empty());
    }

    // ── diagnostics ───────────────────────────────────────────────

    #[test]
    fn diagnostics_replace_rather_than_stack() {
        let mut ws = workspace();
        ws.diagnostics.set("a.ts", "unused import");
        ws.diagnostics.set("a.ts", "type mismatch");
        assert_eq!(ws.diagnostics.len(), 1, "one file, one message");
        assert_eq!(
            ws.diagnostics.get("a.ts").unwrap(),
            &["type mismatch".to_string()]
        );
    }

    #[test]
    fn an_empty_message_resolves_the_error() {
        let mut ws = workspace();
        ws.diagnostics.set("a.ts", "boom");
        ws.diagnostics.set("a.ts", "");
        assert!(ws.diagnostics.is_empty(), "a fixed error must disappear");
    }

    #[test]
    fn diagnostics_come_back_in_path_order() {
        let mut ws = workspace();
        ws.diagnostics.set("z.ts", "e");
        ws.diagnostics.set("a.ts", "e");
        let paths: Vec<&String> = ws.diagnostics.paths().collect();
        assert_eq!(paths, vec!["a.ts", "z.ts"]);
    }

    #[test]
    fn clear_drops_every_file() {
        let mut ws = workspace();
        ws.diagnostics.set("a.ts", "e");
        ws.diagnostics.set("b.ts", "e");
        ws.diagnostics.clear();
        assert!(ws.diagnostics.is_empty());
        assert_eq!(ws.diagnostics.len(), 0);
    }

    // ── git ───────────────────────────────────────────────────────

    #[test]
    fn parses_a_typical_porcelain_status() {
        let snap =
            GitSnapshot::parse("## main...origin/main\n M src/a.ts\nM  src/b.ts\n?? new.ts\n");
        assert_eq!(snap.branch, "main");
        assert_eq!(snap.changes, 1, " M is unstaged only");
        assert_eq!(snap.staged, 1, "M  is staged only");
        assert_eq!(snap.untracked, 1);
    }

    #[test]
    fn strips_the_upstream_from_the_branch() {
        let snap = GitSnapshot::parse("## feature/x...origin/feature/x [ahead 2]\n");
        assert_eq!(snap.branch, "feature/x");
    }

    #[test]
    fn a_clean_tree_has_a_branch_and_no_counts() {
        let snap = GitSnapshot::parse("## main\n");
        assert!(snap.is_repo());
        assert_eq!(snap.summary(), "", "a clean tree shows no numbers");
    }

    #[test]
    fn no_repository_parses_to_a_snapshot_that_is_not_a_repo() {
        let snap = GitSnapshot::parse("");
        assert!(!snap.is_repo());
        assert_eq!(snap.branch, "");
    }

    #[test]
    fn garbage_does_not_panic() {
        // No branch header, blank lines, a truncated status line.
        let snap = GitSnapshot::parse("nonsense\n\n  \nM\n?? \n");
        assert!(!snap.is_repo());
    }

    #[test]
    fn summary_counts_staged_and_unstaged_separately() {
        let snap = GitSnapshot::parse("## main\nMM both.ts\n");
        assert_eq!(snap.summary(), "+1 ~1");
    }

    #[test]
    fn summary_covers_all_three_kinds() {
        let snap = GitSnapshot::parse("## main\nM  s.ts\n M u.ts\n?? n.ts\n");
        assert_eq!(snap.summary(), "+1 ~1 ?1");
    }

    // ── mode ──────────────────────────────────────────────────────

    #[test]
    fn mentor_is_the_default_and_cycles_through_all_three() {
        let mut ws = workspace();
        assert_eq!(ws.mode, AiMode::Mentor, "least power by default");
        assert_eq!(ws.cycle_mode(), AiMode::Assist);
        assert_eq!(ws.cycle_mode(), AiMode::Autonomous);
        assert_eq!(ws.cycle_mode(), AiMode::Mentor, "cycles back to the start");
    }

    #[test]
    fn every_mode_has_a_display_name() {
        for mode in AiMode::ALL {
            assert!(!mode.name().is_empty());
        }
    }

    /// P1 step 2. The table the whole feature rests on: every mode against
    /// every category, as one readable grid instead of 15 scattered asserts.
    /// A mode that can do something unexpected shows up here as a wrong
    /// cell, not as a surprising edit months later.
    #[test]
    fn the_mode_permission_table_is_exactly_the_documented_one() {
        use PermissionDecision::{Allow, Ask, Deny};
        let cell = |mode: AiMode, f: fn(&PermissionsConfig) -> PermissionDecision| {
            f(&mode.permissions())
        };
        let read = |c: &PermissionsConfig| c.read;
        let edit = |c: &PermissionsConfig| c.edit;
        let shell = |c: &PermissionsConfig| c.shell;
        let delete = |c: &PermissionsConfig| c.delete;
        let push = |c: &PermissionsConfig| c.git_push;

        //            read    edit    shell   delete  push
        assert_eq!(cell(AiMode::Mentor, read), Allow);
        assert_eq!(cell(AiMode::Mentor, edit), Deny);
        assert_eq!(cell(AiMode::Mentor, shell), Deny);
        assert_eq!(cell(AiMode::Mentor, delete), Deny);
        assert_eq!(cell(AiMode::Mentor, push), Deny);

        assert_eq!(cell(AiMode::Assist, read), Allow);
        assert_eq!(cell(AiMode::Assist, edit), Ask);
        assert_eq!(cell(AiMode::Assist, shell), Ask);
        assert_eq!(cell(AiMode::Assist, delete), Ask);
        assert_eq!(cell(AiMode::Assist, push), Deny);

        assert_eq!(cell(AiMode::Autonomous, read), Allow);
        assert_eq!(cell(AiMode::Autonomous, edit), Allow);
        assert_eq!(cell(AiMode::Autonomous, shell), Ask);
        assert_eq!(cell(AiMode::Autonomous, delete), Ask);
        assert_eq!(cell(AiMode::Autonomous, push), Deny);
    }

    #[test]
    fn the_modes_are_ordered_from_least_to_most_power() {
        use PermissionDecision::{Allow, Ask, Deny};
        // Monotonic in write power: no mode may be more permissive than a
        // later one in the cycle, or "Autonomous" would be a trapdoor.
        let rank = |d: PermissionDecision| match d {
            Deny => 0,
            Ask => 1,
            Allow => 2,
        };
        let mut previous = 0;
        for mode in [
            AiMode::Mentor,
            AiMode::Assist,
            AiMode::Autonomous,
        ] {
            let p = mode.permissions();
            let power = rank(p.edit) + rank(p.shell);
            assert!(power >= previous, "{} is not more powerful", mode.name());
            previous = power;
        }
    }

    #[test]
    fn a_mode_never_widens_a_users_stricter_choice() {
        use PermissionDecision::Deny;
        // A user who denied shell stays denied in every mode: the mode is a
        // ceiling, and this is the direction that protects them.
        let mut config = PermissionsConfig {
            shell: Deny,
            edit: Deny,
            ..PermissionsConfig::default()
        };
        for mode in AiMode::ALL {
            mode.apply_to(&mut config);
            assert_eq!(config.shell, Deny, "{}", mode.name());
            assert_eq!(config.edit, Deny, "{}", mode.name());
        }
    }

    #[test]
    fn a_mode_can_tighten_a_loose_config() {
        use PermissionDecision::{Allow, Deny};
        // The opposite direction: a config that allows everything must not
        // hand Mentor write access.
        let mut config = PermissionsConfig {
            read: Allow,
            edit: Allow,
            shell: Allow,
            delete: Allow,
            git_push: Allow,
        };
        AiMode::Mentor.apply_to(&mut config);
        assert_eq!(config.read, Allow, "mentor still reads");
        assert_eq!(config.edit, Deny);
        assert_eq!(config.shell, Deny);
        assert_eq!(config.delete, Deny);
        assert_eq!(config.git_push, Deny, "push is never allowed");
    }

    #[test]
    fn git_push_is_denied_in_every_mode() {
        use PermissionDecision::{Allow, Deny};
        // Even with an all-allow config, no mode may enable push.
        for mode in AiMode::ALL {
            let mut config = PermissionsConfig {
                read: Allow,
                edit: Allow,
                shell: Allow,
                delete: Allow,
                git_push: Allow,
            };
            mode.apply_to(&mut config);
            assert_eq!(config.git_push, Deny, "{}", mode.name());
        }
    }

    #[test]
    fn the_default_config_survives_autonomous_unchanged() {
        // The shipped defaults already equal the Autonomous profile, so
        // enabling the mode must not surprise a user who configured nothing.
        let mut config = PermissionsConfig::default();
        let before = config.clone();
        AiMode::Autonomous.apply_to(&mut config);
        assert_eq!(config, before, "autonomous == defaults");
    }

    #[test]
    fn every_mode_explains_itself_in_one_sentence() {
        for mode in AiMode::ALL {
            let text = mode.instruction();
            assert!(!text.is_empty(), "{}", mode.name());
            assert!(
                text.contains(mode.name()),
                "{} does not name itself: {text}",
                mode.name()
            );
        }
    }
}
