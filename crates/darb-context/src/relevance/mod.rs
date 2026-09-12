//! Relevance: order files by likely usefulness for a task (Agents §16).
//!
//! Priority implemented now: explicitly mentioned paths first, then
//! filename word overlap with the task. Import graphs and related-module
//! analysis are future work — the scoring shape already ranks, so that
//! plugs in without reshuffling callers.

use crate::project_map::ProjectFile;

/// A file with its relevance score (higher first).
#[derive(Debug, Clone)]
pub struct ScoredFile<'a> {
    pub file: &'a ProjectFile,
    pub score: u32,
}

/// Score every file against `task`, highest first. Ties break by path
/// for deterministic prompts.
pub fn score<'a>(task: &str, files: &'a [ProjectFile]) -> Vec<ScoredFile<'a>> {
    let task_lower = task.to_lowercase();
    let task_words: Vec<&str> = task_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 2)
        .collect();
    let mut scored: Vec<ScoredFile> = files
        .iter()
        .map(|file| {
            let path_lower = file.path.to_lowercase();
            let mut points = 0u32;
            // 1. Explicitly mentioned files (basename or full path in task).
            if let Some(name) = file.path.rsplit('/').next() {
                if !name.is_empty() && task_lower.contains(&name.to_lowercase()) {
                    points += 100;
                }
            }
            if task_lower.contains(&path_lower) {
                points += 50;
            }
            // 2. Shared vocabulary between task and path segments.
            for segment in path_lower.split('/') {
                let stem = segment.split('.').next().unwrap_or("");
                for word in &task_words {
                    if stem == *word {
                        points += 10;
                    }
                }
            }
            ScoredFile {
                file,
                score: points,
            }
        })
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.file.path.cmp(&b.file.path))
    });
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<ProjectFile> {
        ["auth/login.ts", "auth/session.ts", "payments/cart.ts"]
            .iter()
            .map(|path| ProjectFile {
                path: path.to_string(),
                language: "typescript".to_string(),
                bytes: 10,
            })
            .collect()
    }

    #[test]
    fn explicit_mention_wins() {
        let files = files();
        let ranked = score("fix auth/session.ts validation", &files);
        assert_eq!(ranked[0].file.path, "auth/session.ts");
        assert!(ranked[0].score >= 100);
    }

    #[test]
    fn word_overlap_beats_unrelated() {
        let files = files();
        let ranked = score("fix the login flow", &files);
        assert_eq!(ranked[0].file.path, "auth/login.ts");
        assert_eq!(ranked.last().expect("files").file.path, "payments/cart.ts");
    }
}
