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
/// A roff linter invocation: the program plus the argument list that precedes
/// the file path. Only linters actually proven to support their required
/// options (probed below) are returned, so an unsupported flag can't yield a
/// vacuous "success".
struct Linter {
    tool: &'static str,
    args: Vec<String>,
    /// Exit codes >= this value are treated as fatal (in addition to any
    /// `ERROR:`/`FATAL:` diagnostics). `None` means the tool's exit status is
    /// not a reliable signal and only diagnostics are used.
    fatal_exit_code: Option<i32>,
}

/// Return whether `program` runs at all (used to distinguish "tool absent"
/// from "tool present but option unsupported").
fn program_exists(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .map(|o| o.status.success() || !o.stdout.is_empty() || !o.stderr.is_empty())
        .unwrap_or(false)
        // `mandoc` has no --version; fall back to a harmless probe.
        || Command::new(program).arg("-V").output().is_ok()
}

/// Probe whether `mandoc -T lint` is usable by linting a trivially valid page
/// from stdin. mandoc's exit codes are: 0 = clean, 2 = warnings, 3 = errors,
/// 4 = fatal, 5 = invalid invocation/usage. Anything below 5 means mandoc ran
/// and produced diagnostics normally, so the tool is usable; only a usage error
/// (>= 5) or failure to spawn means we can't use it.
fn mandoc_lint_supported() -> bool {
    use std::io::Write;
    let mut child = match Command::new("mandoc")
        .args(["-T", "lint"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    if let Some(stdin) = child.stdin.as_mut() {
        let _ = stdin.write_all(b".TH T 1 2024-01-01\n.SH NAME\nt \\- t\n");
    }
    match child.wait_with_output() {
        Ok(o) => o.status.code().is_some_and(|c| c < 5),
        Err(_) => false,
    }
}

/// Probe whether this `man` supports `--warnings` and `-l` (local file). GNU
/// man does; BSD/macOS man does not (`--warnings` is unknown there), so we must
/// verify rather than assume.
fn man_warnings_supported() -> bool {
    // GNU man prints help listing --warnings; macOS man does not recognize it.
    match Command::new("man").arg("--help").output() {
        Ok(o) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            text.contains("--warnings") && text.contains("-l")
        }
        Err(_) => false,
    }
}

/// Choose a roff linter that is present *and* proven to support the options we
/// pass. Returns `None` when no compatible linter exists, so the lint check can
/// skip cleanly instead of passing vacuously.
fn find_linter() -> Option<Linter> {
    if program_exists("mandoc") && mandoc_lint_supported() {
        return Some(Linter {
            tool: "mandoc",
            args: vec!["-T".into(), "lint".into()],
            // mandoc: 0 = clean, 2 = warnings (acceptable), 3 = errors,
            // 4 = fatal. Treat >= 3 as fatal.
            fatal_exit_code: Some(3),
        });
    }
    if program_exists("man") && man_warnings_supported() {
        return Some(Linter {
            tool: "man",
            // `--warnings` surfaces roff warnings; `-l` lints a local file.
            args: vec!["--warnings".into(), "-l".into()],
            // GNU man returns 0 for warning-only pages, so status alone is not
            // decisive; rely on ERROR: diagnostics instead.
            fatal_exit_code: None,
        });
    }
    None
}

#[test]
fn config_page_lints_without_fatal_errors() {
    let Some(linter) = find_linter() else {
        eprintln!(
            "no compatible roff linter (mandoc -T lint / man --warnings) available; \
             skipping lint check"
        );
        return;
    };

    let mut args = linter.args.clone();
    args.push(man_page_path().to_string_lossy().into_owned());

    let output = Command::new(linter.tool)
        .args(&args)
        // Discard rendered output; we only care about diagnostics/status.
        .env("MANPAGER", "cat")
        .env("PAGER", "cat")
        .output()
        .unwrap_or_else(|e| panic!("failed to run roff linter {}: {e}", linter.tool));

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Collect hard errors (not STYLE/WARNING noise such as the intentionally
    // empty date or line-length hints).
    let errors: Vec<&str> = stderr
        .lines()
        .filter(|l| l.contains("ERROR:") || l.contains("FATAL:"))
        .collect();
    assert!(
        errors.is_empty(),
        "{} reported roff errors:\n{}",
        linter.tool,
        errors.join("\n")
    );

    // For linters whose exit status is meaningful, an exit code at or above the
    // fatal threshold is a failure even if we didn't recognize the diagnostic
    // text (mandoc: 3 = errors, 4 = fatal; 0/2 = clean/warnings are fine).
    if let Some(threshold) = linter.fatal_exit_code
        && let Some(code) = output.status.code()
    {
        assert!(
            code < threshold,
            "{} exited with code {} (>= fatal threshold {}) while linting; stderr:\n{}",
            linter.tool,
            code,
            threshold,
            stderr
        );
    }
}
