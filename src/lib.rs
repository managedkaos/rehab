//! rehab — a modern reimagining of `detox`: clean up filenames by replacing
//! problematic characters, with composable filters, named sequences, parallel
//! processing, and an undo journal.
//!
//! See `docs/IMPLEMENTATION_PLAN.md` for the full design and task breakdown.

pub mod filters;

pub mod sequence;

pub mod config;

pub mod journal;

pub mod cli;

pub mod walk;

pub mod rename;

pub mod run;

pub use filters::{Filter, SafeFilter};

/// Apply an ordered slice of filters to a single filename component.
///
/// This is the core composition primitive: the output of each filter feeds the
/// next. It is pure and deterministic, which is what makes parallel name
/// computation safe.
pub fn apply_filters(filters: &[Box<dyn Filter>], name: &str) -> String {
    filters
        .iter()
        .fold(name.to_string(), |acc, f| f.apply(&acc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pipeline_is_identity() {
        let filters: Vec<Box<dyn Filter>> = vec![];
        assert_eq!(apply_filters(&filters, "a b.txt"), "a b.txt");
    }

    #[test]
    fn single_safe_filter_cleans_name() {
        let filters: Vec<Box<dyn Filter>> = vec![Box::new(SafeFilter::default())];
        assert_eq!(
            apply_filters(&filters, "my file (1).txt"),
            "my_file__1_.txt"
        );
    }
}
