//! Filter pipeline for cleaning filenames.
//!
//! A [`Filter`] transforms a single path component (a file or directory name,
//! never a full path) into a cleaned form. Filters are composed in order to
//! form a [`crate::sequence::Sequence`]. Each filter must be pure and
//! deterministic so parallel processing produces stable results.

pub mod safe;

pub use safe::SafeFilter;

pub mod cgi_unescape;
pub mod lower;
pub mod unicode_clean;
pub mod wipeup;

pub use cgi_unescape::CgiUnescapeFilter;
pub use lower::LowerFilter;
pub use unicode_clean::UnicodeCleanFilter;
pub use wipeup::WipeupFilter;

/// Default separator used by filters that replace runs of problematic
/// characters with a single character.
pub const DEFAULT_SEPARATOR: char = '_';

/// A single, composable filename-cleaning transformation.
///
/// Implementations operate on one path *component* (basename) at a time and
/// must be deterministic: given the same input they always return the same
/// output. This is required for race-free parallel name computation.
pub trait Filter: Send + Sync {
    /// Stable identifier for this filter, used in sequence listings and config.
    fn name(&self) -> &'static str;

    /// Transform a single filename component, returning the cleaned form.
    fn apply(&self, name: &str) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_separator_is_underscore() {
        assert_eq!(DEFAULT_SEPARATOR, '_');
    }

    #[test]
    fn safe_filter_is_a_filter_trait_object() {
        let f: Box<dyn Filter> = Box::new(SafeFilter::default());
        assert_eq!(f.name(), "safe");
    }
}
