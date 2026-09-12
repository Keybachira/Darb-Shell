//! Reviewer: tool results → pass/fail gate for the loop.
//!
//! MVP rule, deliberately strict: every executed tool must succeed.
//! A single failure fails the batch with the concrete reasons, so the
//! loop retries with evidence instead of marching on over breakage.

use darb_tools::registry::ToolResult;

/// Verdict over one executed batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewVerdict {
    Pass,
    Fail(Vec<String>),
}

/// Judge a batch. An empty batch passes (nothing ran, nothing broke).
pub fn review(results: &[ToolResult]) -> ReviewVerdict {
    let mut reasons = Vec::new();
    for result in results {
        if !result.success {
            let tool = result
                .metadata
                .get("tool")
                .map(String::as_str)
                .unwrap_or("unknown tool");
            let detail = result.error.as_deref().unwrap_or("no details").to_string();
            reasons.push(format!("{tool} failed: {detail}"));
        }
    }
    if reasons.is_empty() {
        ReviewVerdict::Pass
    } else {
        ReviewVerdict::Fail(reasons)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_success_passes() {
        assert_eq!(
            review(&[ToolResult::ok("a".to_string())]),
            ReviewVerdict::Pass
        );
        assert_eq!(review(&[]), ReviewVerdict::Pass);
    }

    #[test]
    fn any_failure_fails_with_reasons() {
        let verdict = review(&[
            ToolResult::ok("a".to_string()),
            ToolResult::fail("boom".to_string()),
        ]);
        match verdict {
            ReviewVerdict::Fail(reasons) => {
                assert_eq!(reasons.len(), 1);
                assert!(reasons[0].contains("boom"));
            }
            ReviewVerdict::Pass => panic!("must fail"),
        }
    }
}
