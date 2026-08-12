//! Tiny case-insensitive subsequence matcher used to drive the TUI filter.

pub fn matches(pattern: &str, haystack: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    let pattern_lower: Vec<char> = pattern.chars().flat_map(|c| c.to_lowercase()).collect();
    let mut pi = 0usize;
    for hc in haystack.chars().flat_map(|c| c.to_lowercase()) {
        if pi < pattern_lower.len() && hc == pattern_lower[pi] {
            pi += 1;
            if pi == pattern_lower.len() {
                return true;
            }
        }
    }
    pi == pattern_lower.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substring_matches() {
        assert!(matches("bil", "billing-svc"));
    }

    #[test]
    fn case_insensitive() {
        assert!(matches("BSV", "billing-svc"));
        assert!(matches("mAiN", "MAIN"));
    }

    #[test]
    fn empty_pattern_matches_anything() {
        assert!(matches("", "anything"));
        assert!(matches("", ""));
    }

    #[test]
    fn non_match_returns_false() {
        assert!(!matches("xyz", "billing-svc"));
    }

    #[test]
    fn order_is_enforced() {
        assert!(!matches("cba", "abc"));
    }

    #[test]
    fn non_ascii() {
        assert!(matches("café", "Café Latté"));
    }
}
