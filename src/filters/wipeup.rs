//! The `wipeup` filter: collapse runs of the separator into one and trim
//! leading/trailing separators and dots.
//!
//! This runs after `safe` (which turns problematic characters into separators)
//! to tidy up the result: `a__b` becomes `a_b`, and `_a_.` becomes `a`.

use super::{DEFAULT_SEPARATOR, Filter};

/// Collapses repeated separators into a single separator and trims leading and
/// trailing separators and dots.
#[derive(Debug, Clone)]
pub struct WipeupFilter {
    separator: char,
}

impl WipeupFilter {
    /// Create a `wipeup` filter with an explicit separator.
    pub fn new(separator: char) -> Self {
        Self { separator }
    }
}

impl Default for WipeupFilter {
    fn default() -> Self {
        Self::new(DEFAULT_SEPARATOR)
    }
}

impl Filter for WipeupFilter {
    fn name(&self) -> &'static str {
        "wipeup"
    }

    fn apply(&self, name: &str) -> String {
        // Collapse consecutive separators into a single one.
        let mut collapsed = String::with_capacity(name.len());
        let mut prev_was_sep = false;
        for c in name.chars() {
            if c == self.separator {
                if !prev_was_sep {
                    collapsed.push(c);
                }
                prev_was_sep = true;
            } else {
                collapsed.push(c);
                prev_was_sep = false;
            }
        }

        // Trim leading/trailing separators and dots. Guard against an
        // empty result so we never produce a nameless entry.
        let trimmed = collapsed.trim_matches(|c| c == self.separator || c == '.');
        if trimmed.is_empty() {
            collapsed
        } else {
            trimmed.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wipeup(input: &str) -> String {
        WipeupFilter::default().apply(input)
    }

    #[test]
    fn collapses_repeated_separators() {
        assert_eq!(wipeup("a__b___c"), "a_b_c");
    }

    #[test]
    fn trims_leading_and_trailing_separators() {
        assert_eq!(wipeup("_a_b_"), "a_b");
    }

    #[test]
    fn trims_leading_and_trailing_dots() {
        assert_eq!(wipeup("...a.b..."), "a.b");
    }

    #[test]
    fn trims_mixed_separators_and_dots() {
        assert_eq!(wipeup("_._a_b_._"), "a_b");
    }

    #[test]
    fn preserves_internal_dots() {
        assert_eq!(wipeup("file.name.txt"), "file.name.txt");
    }

    #[test]
    fn leaves_clean_name_untouched() {
        assert_eq!(wipeup("already_clean-name.txt"), "already_clean-name.txt");
    }

    #[test]
    fn empty_result_guard_keeps_something() {
        // A name of only separators/dots would trim to empty; the guard keeps
        // the collapsed form instead of returning nothing.
        assert_eq!(wipeup("___"), "_");
        assert_eq!(wipeup("..."), "...");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(wipeup(""), "");
    }

    #[test]
    fn multibyte_safe() {
        assert_eq!(wipeup("_café__señor_"), "café_señor");
    }

    #[test]
    fn custom_separator() {
        assert_eq!(WipeupFilter::new('-').apply("-a--b-"), "a-b");
    }
}
