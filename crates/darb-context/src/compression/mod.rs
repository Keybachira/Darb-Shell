//! Small text shaping for prompts: truncate long blobs keeping head and
//! tail (signatures at the top, errors at the bottom) instead of cutting
//! the end off blindly.

/// Cap `text` at `max_chars`, keeping head and tail with a marker.
/// Returns the original when it already fits.
pub fn truncate_middle(text: &str, max_chars: usize) -> String {
    const MARKER: &str = "\n…[truncated]…\n";
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let marker_len = MARKER.chars().count();
    let keep = max_chars.saturating_sub(marker_len);
    let head = keep * 2 / 3;
    let tail = keep - head;
    let chars: Vec<char> = text.chars().collect();
    let head_text: String = chars[..head].iter().collect();
    let tail_text: String = chars[chars.len() - tail..].iter().collect();
    format!("{head_text}{MARKER}{tail_text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_passes_through() {
        assert_eq!(truncate_middle("abc", 100), "abc");
    }

    #[test]
    fn long_text_keeps_head_and_tail() {
        let text = (0..100)
            .map(|n| format!("line{n:03}\n"))
            .collect::<String>();
        let cut = truncate_middle(&text, 100);
        assert!(cut.starts_with("line000"));
        assert!(cut.trim_end().ends_with("line099"));
        assert!(cut.contains("[truncated]"));
        assert!(cut.chars().count() <= 100);
    }
}
