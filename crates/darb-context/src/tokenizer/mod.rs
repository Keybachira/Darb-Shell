//! Token estimates without a model tokenizer.
//!
//! Why a heuristic: the exact count depends on the provider's tokenizer,
//! which the Agent doesn't know (Negócio §13). `estimate` is deliberately
//! conservative-ish (≈4 chars/token for code/English) and is used only
//! for *budgeting* — never billed, never asserted exact. A real
//! provider-specific counter can replace this function later.

/// Rough token count for budgeting. Empty text costs nothing.
pub fn estimate(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    // Include a small per-message overhead so tiny prompts don't budget zero.
    (text.chars().count() as u32).div_ceil(4).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_costs_nothing() {
        assert_eq!(estimate(""), 0);
    }

    #[test]
    fn scales_with_length() {
        let short = estimate("hi");
        let long = estimate(&"hello world ".repeat(100));
        assert!(long > short * 10, "{short} vs {long}");
        assert_eq!(estimate("abcd"), 1);
        assert_eq!(estimate("abcde"), 2);
    }
}
