//! The `cgi-unescape` filter: decode percent-encoded (`%XX`) sequences in a
//! filename, as produced by browsers when saving files with URL-encoded names.
//!
//! Decoding is done at the byte level so multibyte UTF-8 sequences (e.g.
//! `%C3%A9` → `é`) are reconstructed correctly. Invalid escapes are left
//! verbatim, and if the decoded bytes are not valid UTF-8 the original input is
//! returned unchanged (never lossy).

use super::Filter;

/// Decodes `%XX` percent-encoded sequences into their byte values.
#[derive(Debug, Clone, Default)]
pub struct CgiUnescapeFilter;

impl CgiUnescapeFilter {
    /// Create a `cgi-unescape` filter.
    pub fn new() -> Self {
        Self
    }
}

/// Convert a single hex ASCII digit to its value, or `None` if not hex.
fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

impl Filter for CgiUnescapeFilter {
    fn name(&self) -> &'static str {
        "cgi-unescape"
    }

    fn apply(&self, name: &str) -> String {
        let bytes = name.as_bytes();
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            // A valid escape needs `%` followed by two hex digits.
            if bytes[i] == b'%'
                && i + 2 < bytes.len()
                && let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
            {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
            out.push(bytes[i]);
            i += 1;
        }

        // Only accept the decoded form if it is valid UTF-8; otherwise keep the
        // original (never produce a lossy/garbled name).
        match String::from_utf8(out) {
            Ok(s) => s,
            Err(_) => name.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unescape(input: &str) -> String {
        CgiUnescapeFilter::new().apply(input)
    }

    #[test]
    fn decodes_space() {
        assert_eq!(unescape("my%20file.txt"), "my file.txt");
    }

    #[test]
    fn decodes_multibyte_utf8() {
        // %C3%A9 is the UTF-8 encoding of 'é'.
        assert_eq!(unescape("caf%C3%A9.txt"), "café.txt");
    }

    #[test]
    fn decodes_multiple_escapes() {
        assert_eq!(unescape("a%20b%20c"), "a b c");
    }

    #[test]
    fn leaves_invalid_escape_verbatim() {
        assert_eq!(unescape("100%done"), "100%done");
        assert_eq!(unescape("a%2"), "a%2");
        assert_eq!(unescape("a%ZZ"), "a%ZZ");
    }

    #[test]
    fn trailing_percent_kept() {
        assert_eq!(unescape("file%"), "file%");
    }

    #[test]
    fn accepts_lowercase_hex() {
        assert_eq!(unescape("a%2fb"), "a/b");
    }

    #[test]
    fn invalid_utf8_returns_original() {
        // %FF is not valid UTF-8 on its own; keep the original string.
        assert_eq!(unescape("bad%FFname"), "bad%FFname");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(unescape(""), "");
    }

    #[test]
    fn no_escapes_untouched() {
        assert_eq!(unescape("plain_name.txt"), "plain_name.txt");
    }
}
