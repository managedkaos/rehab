//! Man page generator for `rehab`.
//!
//! Renders `rehab.1` from the clap [`Cli`] definition using `clap_mangen`,
//! augmenting the auto-generated content with hand-written ENVIRONMENT, FILES,
//! and SEE ALSO sections (which clap_mangen does not emit), plus a section per
//! subcommand (`undo`, `journals`, `init`) so the single page documents the
//! whole tool.
//!
//! Usage: `cargo run --bin gen-man -- [OUTPUT_DIR]` (default: `man/`).

use std::fs;
use std::io;
use std::path::PathBuf;

use clap::CommandFactory;
use rehab::cli::Cli;

fn main() -> io::Result<()> {
    let out_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("man"));
    fs::create_dir_all(&out_dir)?;

    let roff = render_rehab_1()?;
    let path = out_dir.join("rehab.1");
    fs::write(&path, roff)?;
    println!("wrote {}", path.display());
    Ok(())
}

/// Render the complete `rehab.1` man page as ROFF text.
pub fn render_rehab_1() -> io::Result<Vec<u8>> {
    let cmd = Cli::command();
    let man = clap_mangen::Man::new(cmd.clone());

    let mut buf: Vec<u8> = Vec::new();

    // Standard sections rendered by clap_mangen, in the usual order.
    man.render_title(&mut buf)?;
    man.render_name_section(&mut buf)?;
    man.render_synopsis_section(&mut buf)?;
    man.render_description_section(&mut buf)?;
    man.render_options_section(&mut buf)?;

    // Per-subcommand sections so undo/journals/init are documented on the page.
    render_subcommands(&cmd, &mut buf)?;

    // Custom sections clap_mangen does not produce.
    buf.extend_from_slice(ENVIRONMENT_SECTION.as_bytes());
    buf.extend_from_slice(FILES_SECTION.as_bytes());
    buf.extend_from_slice(SEE_ALSO_SECTION.as_bytes());

    man.render_version_section(&mut buf)?;

    Ok(buf)
}

/// Emit a `SUBCOMMANDS` section listing each subcommand with its about text and
/// its own options, so the single top-level page covers them all.
fn render_subcommands(cmd: &clap::Command, buf: &mut Vec<u8>) -> io::Result<()> {
    let subs: Vec<&clap::Command> = cmd.get_subcommands().filter(|c| !c.is_hide_set()).collect();
    if subs.is_empty() {
        return Ok(());
    }

    buf.extend_from_slice(b".SH SUBCOMMANDS\n");
    for sub in subs {
        render_one_subcommand(cmd.get_name(), sub, buf)?;
    }
    Ok(())
}

/// Render a single subcommand (and one level of nested subcommands) as a
/// `.SS` subsection with a synopsis, description, and options.
fn render_one_subcommand(parent: &str, sub: &clap::Command, buf: &mut Vec<u8>) -> io::Result<()> {
    let name = sub.get_name();
    buf.extend_from_slice(format!(".SS {parent} {name}\n").as_bytes());

    if let Some(about) = sub.get_about() {
        buf.extend_from_slice(format!("{about}\n.PP\n").as_bytes());
    }

    // A compact synopsis for the subcommand. clap_mangen emits each section
    // with its own top-level `.SH SYNOPSIS`/`.SH OPTIONS` header and a repeated
    // `.ds Aq` preamble; strip those so the content nests cleanly under the
    // `.SS` subsection instead of resetting the page's section structure.
    let full = format!("{parent} {name}");
    let man = clap_mangen::Man::new(sub.clone());
    let mut sub_buf: Vec<u8> = Vec::new();
    man.render_synopsis_section(&mut sub_buf)?;
    man.render_options_section(&mut sub_buf)?;
    buf.extend_from_slice(clean_subcommand_roff(&sub_buf).as_bytes());

    // One level of nested subcommands (e.g. `journals list` / `journals prune`).
    for nested in sub.get_subcommands().filter(|c| !c.is_hide_set()) {
        render_one_subcommand(&full, nested, buf)?;
    }
    Ok(())
}

/// Rewrite a subcommand's rendered ROFF so it nests under a `.SS` subsection:
/// drop the repeated `.ds Aq` preamble lines and demote the `.SH SYNOPSIS` /
/// `.SH OPTIONS` headers (which would otherwise reset the page's sectioning)
/// to inline emphasis lines.
fn clean_subcommand_roff(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let mut out = String::new();
    for line in text.lines() {
        if line.starts_with(".ds Aq") || line == ".ie \\n(.g .ds Aq \\(aq" || line == ".el .ds Aq '"
        {
            continue;
        }
        let mapped = match line {
            ".SH SYNOPSIS" => "\\fISynopsis:\\fR\n.br",
            ".SH OPTIONS" => "\\fIOptions:\\fR\n.br",
            other => other,
        };
        out.push_str(mapped);
        out.push('\n');
    }
    out
}

const ENVIRONMENT_SECTION: &str = r#".SH ENVIRONMENT
.TP
.B XDG_CONFIG_HOME
Overrides the base directory used to locate the configuration file. When set,
rehab reads \fB$XDG_CONFIG_HOME/rehab/config.toml\fR instead of
\fB~/.config/rehab/config.toml\fR.
.TP
.B XDG_STATE_HOME
Overrides the base directory used for undo journals. When set, journals are
written under \fB$XDG_STATE_HOME/rehab/\fR instead of
\fB~/.local/state/rehab/\fR.
"#;

const FILES_SECTION: &str = r#".SH FILES
.TP
.B ~/.config/rehab/config.toml
Default configuration file (same path on Linux and macOS). See
\fBrehab-config\fR(5). Overridden by \fB\-\-config\fR or by
\fB$XDG_CONFIG_HOME\fR.
.TP
.B ~/.local/state/rehab/journal-<timestamp>.jsonl
Per-run undo journal. One JSONL file is written per non-dry-run. Overridden by
\fB$XDG_STATE_HOME\fR.
"#;

const SEE_ALSO_SECTION: &str = r#".SH SEE ALSO
.BR rehab-config (5),
.BR ascii (7),
.BR iso_8859-1 (7),
.BR unicode (7),
.BR utf-8 (7)
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered() -> String {
        String::from_utf8(render_rehab_1().expect("render")).expect("utf8")
    }

    #[test]
    fn has_title_and_name() {
        let roff = rendered();
        assert!(roff.contains(".TH rehab 1"), "missing .TH title");
        assert!(roff.contains(".SH NAME"), "missing NAME section");
    }

    #[test]
    fn documents_key_flags() {
        let roff = rendered();
        for flag in ["\\-\\-dry\\-run", "\\-\\-recursive", "\\-\\-on\\-collision"] {
            assert!(roff.contains(flag), "missing flag {flag}");
        }
    }

    #[test]
    fn documents_all_subcommands() {
        let roff = rendered();
        assert!(roff.contains(".SH SUBCOMMANDS"), "missing SUBCOMMANDS");
        for sub in ["rehab undo", "rehab journals", "rehab init"] {
            assert!(roff.contains(sub), "missing subcommand {sub}");
        }
        // Nested journals subcommands.
        assert!(roff.contains("rehab journals list"));
        assert!(roff.contains("rehab journals prune"));
    }

    #[test]
    fn has_environment_section_with_both_vars() {
        let roff = rendered();
        assert!(roff.contains(".SH ENVIRONMENT"));
        assert!(roff.contains("XDG_CONFIG_HOME"));
        assert!(roff.contains("XDG_STATE_HOME"));
    }

    #[test]
    fn has_files_section_with_both_paths() {
        let roff = rendered();
        assert!(roff.contains(".SH FILES"));
        assert!(roff.contains("~/.config/rehab/config.toml"));
        assert!(roff.contains("~/.local/state/rehab/journal-<timestamp>.jsonl"));
    }

    #[test]
    fn has_see_also_with_all_references() {
        let roff = rendered();
        assert!(roff.contains(".SH SEE ALSO"));
        for page in [
            "rehab-config (5)",
            "ascii (7)",
            "iso_8859-1 (7)",
            "unicode (7)",
            "utf-8 (7)",
        ] {
            assert!(roff.contains(page), "missing SEE ALSO ref {page}");
        }
    }
}
