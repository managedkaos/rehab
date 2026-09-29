//! Tests for the hand-written `man/rehab-config.5` page: it must exist, carry
//! the required roff structure, document every config key, and cross-reference
//! the related pages. When a man/mandoc linter is available, it must lint
//! without fatal errors; otherwise that check is skipped.

use std::path::PathBuf;
use std::process::Command;

fn man_page_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("man")
        .join("rehab-config.5")
}

fn man_page() -> String {
    let path = man_page_path();
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

#[test]
fn config_page_has_required_structure() {
    let page = man_page();
    for macro_line in [".TH REHAB-CONFIG 5", ".SH NAME", ".SH SEE ALSO"] {
        assert!(
            page.contains(macro_line),
            "missing roff macro: {macro_line}"
        );
    }
}

#[test]
fn config_page_documents_all_keys() {
    let page = man_page();
    for key in [
        "default",
        "[sequences.<name>]",
        "filters",
        "separator",
        "on_collision",
        "[journal]",
        "keep",
        "auto_prune",
    ] {
        assert!(page.contains(key), "config page is missing key: {key}");
    }
}

#[test]
fn config_page_documents_filters_and_builtins() {
    let page = man_page();
    for name in [
        "safe",
        "wipeup",
        "lower",
        "unicode-clean",
        "cgi-unescape",
        "iso8859_1",
        "utf8",
    ] {
        assert!(page.contains(name), "config page is missing: {name}");
    }
}

#[test]
fn config_page_notes_unwired_caveat() {
    let page = man_page();
    // The separator/on_collision "not yet wired" caveat must be present so the
    // page matches actual behavior.
    assert!(
        page.contains("not yet") && page.contains("wired"),
        "config page must note that separator/on_collision are not yet wired"
    );
}

#[test]
fn config_page_cross_references_related_pages() {
    let page = man_page();
    for reference in [
        ".BR rehab (1)",
        ".BR ascii (7)",
        ".BR iso_8859-1 (7)",
        ".BR unicode (7)",
        ".BR utf-8 (7)",
    ] {
        assert!(
            page.contains(reference),
            "missing SEE ALSO ref: {reference}"
        );
    }
}

/// Locate a roff linter: prefer `mandoc -T lint`, fall back to `man --warnings`.
/// Returns `None` if neither is available (e.g. bare CI images), so the test
/// can skip gracefully rather than fail on environment.
fn find_linter() -> Option<(&'static str, Vec<String>)> {
    if Command::new("mandoc").arg("-V").output().is_ok() {
        return Some(("mandoc", vec!["-T".into(), "lint".into()]));
    }
    // `man --warnings` prints mandoc-style warnings to stderr; exit status is
    // still 0 for style/warning-only pages.
    if Command::new("man").arg("--version").output().is_ok() {
        return Some(("man", vec!["--warnings".into(), "-l".into()]));
    }
    None
}

#[test]
fn config_page_lints_without_fatal_errors() {
    let Some((tool, mut args)) = find_linter() else {
        eprintln!("no roff linter (mandoc/man) available; skipping lint check");
        return;
    };
    args.push(man_page_path().to_string_lossy().into_owned());

    let output = Command::new(tool)
        .args(&args)
        .output()
        .expect("failed to run roff linter");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Fail only on hard ERRORs, not STYLE/WARNING lint noise (e.g. the
    // intentionally empty date, or line-length style hints).
    let errors: Vec<&str> = stderr.lines().filter(|l| l.contains("ERROR:")).collect();
    assert!(
        errors.is_empty(),
        "roff linter reported errors:\n{}",
        errors.join("\n")
    );
}
