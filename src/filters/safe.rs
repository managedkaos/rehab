//! The `safe` filter: replace shell-problematic characters and whitespace with
//! a configurable separator (default `_`).

use super::{DEFAULT_SEPARATOR, Filter};

/// Characters that are awkward or dangerous to type in a POSIX shell and are
/// therefore replaced by the separator. The path separator `/` is never part
/// of a single component, but is included defensively.
const PROBLEM_CHARS: &[char] = &[
    ' ', '\t', '\n', '\r', '(', ')', '{', '}', '[', ']', '&', ';', '|', '<', '>', '*', '?', '!',
    '\'', '"', '$', '`', '\\', ':', '#', '~', '=', ',', '@', '+', '/',
];

/// Replaces problematic characters with a separator character.
#[derive(Debug, Clone)]
pub struct SafeFilter {
    separator: char,
}

impl SafeFilter {
    /// Create a `safe` filter with an explicit separator.
    pub fn new(separator: char) -> Self {
        Self { separator }
    }
}

impl Default for SafeFilter {
    fn default() -> Self {
        Self::new(DEFAULT_SEPARATOR)
    }
}

impl Filter for SafeFilter {
    fn name(&self) -> &'static str {
        "safe"
    }

    fn apply(&self, name: &str) -> String {
        name.chars()
            .map(|c| {
                if PROBLEM_CHARS.contains(&c) {
                    self.separator
                } else {
                    c
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn safe(input: &str) -> String {
        SafeFilter::default().apply(input)
    }

    #[test]
    fn replaces_spaces() {
        assert_eq!(safe("my file.txt"), "my_file.txt");
    }

    #[test]
    fn replaces_parentheses_and_brackets() {
        assert_eq!(safe("file(1)[a]{b}.txt"), "file_1__a__b_.txt");
    }

    #[test]
    fn replaces_shell_metacharacters() {
        assert_eq!(safe("a&b;c|d.txt"), "a_b_c_d.txt");
        assert_eq!(safe("a<b>c*d?e.txt"), "a_b_c_d_e.txt");
    }

    #[test]
    fn replaces_quotes_and_dollar() {
        assert_eq!(safe("it's \"$HOME\".txt"), "it_s___HOME_.txt");
    }

    #[test]
    fn replaces_tabs_and_newlines() {
        assert_eq!(safe("a\tb\nc"), "a_b_c");
    }

    #[test]
    fn leaves_clean_names_untouched() {
        assert_eq!(safe("already_clean-name.txt"), "already_clean-name.txt");
    }

    #[test]
    fn preserves_unicode_letters() {
        assert_eq!(safe("café_señor.txt"), "café_señor.txt");
    }

    #[test]
    fn custom_separator() {
        assert_eq!(SafeFilter::new('-').apply("my file.txt"), "my-file.txt");
    }
}
