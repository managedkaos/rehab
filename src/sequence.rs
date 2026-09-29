//! Sequences: ordered chains of filters applied to a filename component.
//!
//! A [`Sequence`] wraps an ordered `Vec<Box<dyn Filter>>`. Built-in sequences
//! cover the common cases (`safe`, `safe-lower`, `iso8859_1`, `utf8`), and the
//! `default` sequence chains `safe` → `wipeup` → `unicode-clean`.
//!
//! Filters are constructed by name via [`filter_by_name`], which is the single
//! place that maps a stable filter identifier to a concrete implementation.
//! Configuration ([`crate::config`]) builds custom sequences on top of this.

use crate::apply_filters;
use crate::filters::{
    CgiUnescapeFilter, Filter, LowerFilter, SafeFilter, UnicodeCleanFilter, WipeupFilter,
};

/// The default separator-collapsing / cleaning chain applied when no sequence
/// is selected: `safe` → `wipeup` → `unicode-clean`.
pub const DEFAULT_SEQUENCE: &str = "default";

/// Ordered chain of filters plus a human-facing name.
pub struct Sequence {
    name: String,
    filters: Vec<Box<dyn Filter>>,
}

impl Sequence {
    /// Build a sequence from a name and an ordered list of filter names.
    ///
    /// Returns an error naming the first unknown filter.
    pub fn from_filter_names(name: &str, filter_names: &[&str]) -> Result<Self, String> {
        let mut filters: Vec<Box<dyn Filter>> = Vec::with_capacity(filter_names.len());
        for &fname in filter_names {
            filters.push(filter_by_name(fname)?);
        }
        Ok(Self {
            name: name.to_string(),
            filters,
        })
    }

    /// Construct a sequence directly from owned filter boxes.
    pub fn from_filters(name: impl Into<String>, filters: Vec<Box<dyn Filter>>) -> Self {
        Self {
            name: name.into(),
            filters,
        }
    }

    /// The sequence's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The ordered filter identifiers making up this sequence.
    pub fn filter_names(&self) -> Vec<&'static str> {
        self.filters.iter().map(|f| f.name()).collect()
    }

    /// Borrow the underlying filter chain (e.g. to pass to [`apply_filters`]).
    pub fn filters(&self) -> &[Box<dyn Filter>] {
        &self.filters
    }

    /// Apply the whole chain to a single filename component.
    pub fn apply(&self, name: &str) -> String {
        apply_filters(&self.filters, name)
    }
}

impl std::fmt::Debug for Sequence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sequence")
            .field("name", &self.name)
            .field("filters", &self.filter_names())
            .finish()
    }
}

/// Construct a single filter from its stable name.
///
/// This is the canonical name→filter mapping used by both built-in sequences
/// and user configuration. Unknown names produce a descriptive error.
pub fn filter_by_name(name: &str) -> Result<Box<dyn Filter>, String> {
    match name {
        "safe" => Ok(Box::new(SafeFilter::default())),
        "wipeup" => Ok(Box::new(WipeupFilter::default())),
        "lower" => Ok(Box::new(LowerFilter::new())),
        "unicode-clean" => Ok(Box::new(UnicodeCleanFilter::new())),
        "cgi-unescape" => Ok(Box::new(CgiUnescapeFilter::new())),
        other => Err(format!("unknown filter: {other}")),
    }
}

/// The list of built-in sequence names paired with their ordered filter names.
///
/// This is the single source of truth for built-in sequences; both
/// [`builtin`] and [`list_builtins`] derive from it.
const BUILTIN_DEFS: &[(&str, &[&str])] = &[
    ("default", &["safe", "wipeup", "unicode-clean"]),
    ("safe", &["safe"]),
    ("safe-lower", &["safe", "wipeup", "lower"]),
    // iso8859_1 / utf8 focus on transcoding (handled at the byte layer); as
    // text chains they apply cgi-unescape then the safe cleanup pipeline.
    (
        "iso8859_1",
        &["cgi-unescape", "safe", "wipeup", "unicode-clean"],
    ),
    ("utf8", &["cgi-unescape", "safe", "wipeup", "unicode-clean"]),
];

/// Look up a built-in sequence by name, returning `None` if it is not built in.
pub fn builtin(name: &str) -> Option<Sequence> {
    BUILTIN_DEFS.iter().find_map(|(sname, fnames)| {
        if *sname == name {
            // Built-in definitions only reference known filters, so this
            // cannot fail; unwrap keeps the signature clean.
            Some(
                Sequence::from_filter_names(sname, fnames)
                    .expect("built-in sequence references only known filters"),
            )
        } else {
            None
        }
    })
}

/// Resolve a sequence by name, falling back to built-ins.
///
/// Returns an error listing available built-ins if the name is unknown.
pub fn resolve(name: &str) -> Result<Sequence, String> {
    builtin(name).ok_or_else(|| {
        let names: Vec<&str> = BUILTIN_DEFS.iter().map(|(n, _)| *n).collect();
        format!("unknown sequence: {name} (available: {})", names.join(", "))
    })
}

/// The default sequence.
pub fn default_sequence() -> Sequence {
    builtin(DEFAULT_SEQUENCE).expect("default sequence is built in")
}

/// List built-in sequences as `(name, filter-names)` pairs, in a stable order.
pub fn list_builtins() -> Vec<(&'static str, Vec<&'static str>)> {
    BUILTIN_DEFS
        .iter()
        .map(|(name, fnames)| (*name, fnames.to_vec()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sequence_has_expected_filters() {
        let seq = default_sequence();
        assert_eq!(seq.name(), "default");
        assert_eq!(seq.filter_names(), vec!["safe", "wipeup", "unicode-clean"]);
    }

    #[test]
    fn safe_builtin_is_single_filter() {
        let seq = builtin("safe").unwrap();
        assert_eq!(seq.filter_names(), vec!["safe"]);
    }

    #[test]
    fn safe_lower_builtin_order() {
        let seq = builtin("safe-lower").unwrap();
        assert_eq!(seq.filter_names(), vec!["safe", "wipeup", "lower"]);
    }

    #[test]
    fn iso8859_1_and_utf8_builtins_exist() {
        assert!(builtin("iso8859_1").is_some());
        assert!(builtin("utf8").is_some());
    }

    #[test]
    fn resolve_unknown_errors_with_available_list() {
        let err = resolve("nope").unwrap_err();
        assert!(err.contains("unknown sequence: nope"));
        assert!(err.contains("default"));
        assert!(err.contains("safe-lower"));
    }

    #[test]
    fn filter_by_name_known_and_unknown() {
        assert_eq!(filter_by_name("safe").unwrap().name(), "safe");
        assert_eq!(filter_by_name("wipeup").unwrap().name(), "wipeup");
        assert_eq!(filter_by_name("lower").unwrap().name(), "lower");
        assert_eq!(
            filter_by_name("unicode-clean").unwrap().name(),
            "unicode-clean"
        );
        assert_eq!(
            filter_by_name("cgi-unescape").unwrap().name(),
            "cgi-unescape"
        );
        assert!(filter_by_name("bogus").is_err());
    }

    #[test]
    fn from_filter_names_reports_unknown_filter() {
        let err = Sequence::from_filter_names("x", &["safe", "bogus"]).unwrap_err();
        assert!(err.contains("unknown filter: bogus"));
    }

    #[test]
    fn default_sequence_applies_end_to_end() {
        // safe leaves underscores/dots; wipeup collapses the "__" run and trims
        // the leading "_". The "_" before ".txt" is internal, so it stays.
        let seq = default_sequence();
        assert_eq!(seq.apply("_a__b_.txt"), "a_b_.txt");
    }

    #[test]
    fn safe_lower_lowercases() {
        let seq = builtin("safe-lower").unwrap();
        assert_eq!(seq.apply("My File.TXT"), "my_file.txt");
    }

    #[test]
    fn list_builtins_contains_all() {
        let names: Vec<&str> = list_builtins().into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            names,
            vec!["default", "safe", "safe-lower", "iso8859_1", "utf8"]
        );
    }

    #[test]
    fn list_builtins_pairs_names_with_filters() {
        let list = list_builtins();
        let default = list.iter().find(|(n, _)| *n == "default").unwrap();
        assert_eq!(default.1, vec!["safe", "wipeup", "unicode-clean"]);
    }
}
