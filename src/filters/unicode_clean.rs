//! The `unicode-clean` filter: strip Unicode control and format characters that
//! are invisible or disruptive in filenames (e.g. zero-width spaces,
//! bidirectional overrides, C0/C1 control codes).

use super::Filter;

/// Removes Unicode control (category `Cc`) and format (category `Cf`)
/// characters. Ordinary whitespace like the regular space is *not* a control
/// character and is left untouched (the `safe` filter handles spaces).
#[derive(Debug, Clone, Default)]
pub struct UnicodeCleanFilter;

impl UnicodeCleanFilter {
    /// Create a `unicode-clean` filter.
    pub fn new() -> Self {
        Self
    }
}

/// Returns `true` if the character should be stripped: Unicode control
/// characters (`Cc`) or format characters (`Cf`).
fn is_control_or_format(c: char) -> bool {
    // `char::is_control` covers the Cc category (C0 and C1 control codes).
    // Format characters (Cf) include zero-width joiners/non-joiners, the BOM,
    // bidirectional overrides, and similar invisible marks.
    c.is_control() || is_format_char(c)
}

/// Returns `true` for Unicode general-category `Cf` (Format) characters.
///
/// Implemented as an explicit range check to avoid pulling in a Unicode-tables
/// dependency; covers the ranges defined in the Unicode standard for `Cf`.
fn is_format_char(c: char) -> bool {
    matches!(c as u32,
        0x00AD                    // SOFT HYPHEN
        | 0x0600..=0x0605         // Arabic number/formatting signs
        | 0x061C                  // ARABIC LETTER MARK
        | 0x06DD                  // ARABIC END OF AYAH
        | 0x070F                  // SYRIAC ABBREVIATION MARK
        | 0x0890..=0x0891         // Arabic pound/piastre marks
        | 0x08E2                  // ARABIC DISPUTED END OF AYAH
        | 0x180E                  // MONGOLIAN VOWEL SEPARATOR
        | 0x200B..=0x200F         // zero-width space..right-to-left mark
        | 0x202A..=0x202E         // bidirectional embedding/override
        | 0x2060..=0x2064         // word joiner..invisible plus
        | 0x2066..=0x206F         // bidi isolates and deprecated format chars
        | 0xFEFF                  // ZERO WIDTH NO-BREAK SPACE (BOM)
        | 0xFFF9..=0xFFFB         // interlinear annotation anchors
        | 0x110BD                 // KAITHI NUMBER SIGN
        | 0x110CD                 // KAITHI NUMBER SIGN ABOVE
        | 0x13430..=0x1343F       // Egyptian hieroglyph format controls
        | 0x1BCA0..=0x1BCA3       // Shorthand format controls
        | 0x1D173..=0x1D17A       // musical symbol begin/end controls
        | 0xE0001                 // LANGUAGE TAG
        | 0xE0020..=0xE007F       // tag characters
    )
}

impl Filter for UnicodeCleanFilter {
    fn name(&self) -> &'static str {
        "unicode-clean"
    }

    fn apply(&self, name: &str) -> String {
        name.chars().filter(|&c| !is_control_or_format(c)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(input: &str) -> String {
        UnicodeCleanFilter::new().apply(input)
    }

    #[test]
    fn strips_zero_width_space() {
        assert_eq!(clean("a\u{200B}b.txt"), "ab.txt");
    }

    #[test]
    fn strips_bom() {
        assert_eq!(clean("\u{FEFF}file.txt"), "file.txt");
    }

    #[test]
    fn strips_bidi_override() {
        assert_eq!(clean("a\u{202E}b"), "ab");
    }

    #[test]
    fn strips_c0_control_codes() {
        assert_eq!(clean("a\u{0000}\u{0007}b"), "ab");
    }

    #[test]
    fn strips_soft_hyphen() {
        assert_eq!(clean("soft\u{00AD}hyphen"), "softhyphen");
    }

    #[test]
    fn preserves_ordinary_space() {
        // Spaces are not control chars; the safe filter deals with them.
        assert_eq!(clean("a b.txt"), "a b.txt");
    }

    #[test]
    fn preserves_normal_unicode_letters() {
        assert_eq!(clean("café_señor.txt"), "café_señor.txt");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(clean(""), "");
    }

    #[test]
    fn all_control_chars_yields_empty() {
        assert_eq!(clean("\u{0001}\u{0002}"), "");
    }
}
