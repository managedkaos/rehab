//! The `lower` filter: lowercase the entire filename component.

use super::Filter;

/// Lowercases a filename component using Unicode-aware lowercasing.
#[derive(Debug, Clone, Default)]
pub struct LowerFilter;

impl LowerFilter {
    /// Create a `lower` filter.
    pub fn new() -> Self {
        Self
    }
}

impl Filter for LowerFilter {
    fn name(&self) -> &'static str {
        "lower"
    }

    fn apply(&self, name: &str) -> String {
        name.to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(input: &str) -> String {
        LowerFilter::new().apply(input)
    }

    #[test]
    fn lowercases_ascii() {
        assert_eq!(lower("MyFile.TXT"), "myfile.txt");
    }

    #[test]
    fn leaves_already_lowercase_untouched() {
        assert_eq!(lower("already_lower.txt"), "already_lower.txt");
    }

    #[test]
    fn lowercases_unicode() {
        assert_eq!(lower("CAFÉ_SEÑOR.TXT"), "café_señor.txt");
    }

    #[test]
    fn preserves_digits_and_separators() {
        assert_eq!(lower("File_123-A.TXT"), "file_123-a.txt");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(lower(""), "");
    }
}
