//! Command-line interface definitions (Task 2).
//!
//! Defines the default `run` behavior plus an `undo` subcommand. Flags mirror
//! the plan: `--dry-run/-n`, `--recursive/-r`, `--verbose/-v`, `--sequence/-s`,
//! `--jobs/-j`, `--on-collision`, `--config/-f`, `--list-sequences/-L`.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// rehab — clean up filenames by replacing problematic characters.
#[derive(Debug, Parser)]
#[command(
    name = "rehab",
    version,
    about = "Clean up filenames by replacing problematic characters with safe alternatives.",
    long_about = "rehab renames files and directories to replace problematic characters \
(spaces, shell metacharacters, control characters, CGI escapes) with safe, \
easy-to-type alternatives. Filters are composable, sequences are configurable, \
traversal is parallel, and every run is journaled so it can be undone.\n\n\
When invoked with no subcommand, rehab cleans the given paths. Naming a \
directory without --recursive processes its immediate, non-hidden contents \
(one level, no descent); use --recursive to descend into subdirectories.",
    disable_version_flag = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Options for the default (run) behavior.
    #[command(flatten)]
    pub run: RunArgs,

    /// Print the current rehab version and exit.
    #[arg(short = 'V', long = "version", action = clap::ArgAction::Version)]
    pub version: Option<bool>,
}

/// Subcommands. When omitted, the default run behavior applies.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Reverse a previous run using its journal.
    Undo(UndoArgs),

    /// List or prune saved rename journals.
    Journals(JournalsArgs),

    /// Write a default config file to the standard location.
    Init(InitArgs),
}

/// Arguments for the default run behavior.
#[derive(Debug, clap::Args)]
pub struct RunArgs {
    /// Show the planned renames without modifying the filesystem. Skipped
    /// files are not listed unless `-v` is also given.
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,

    /// Recurse into subdirectories.
    #[arg(short = 'r', long = "recursive")]
    pub recursive: bool,

    /// Report each rename as it happens, and also report skipped files.
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Sequence (ordered filter chain) to apply.
    #[arg(short = 's', long = "sequence")]
    pub sequence: Option<String>,

    /// Number of worker threads (default: available parallelism).
    #[arg(short = 'j', long = "jobs")]
    pub jobs: Option<usize>,

    /// Behavior when a target name already exists.
    #[arg(long = "on-collision", value_enum, default_value_t = OnCollision::Suffix)]
    pub on_collision: OnCollision,

    /// Use this config file instead of the default discovery.
    #[arg(short = 'f', long = "config")]
    pub config: Option<PathBuf>,

    /// List available sequences and exit.
    #[arg(short = 'L', long = "list-sequences")]
    pub list_sequences: bool,

    /// Keep only the N most recent journals (0 = unlimited). Overrides config.
    #[arg(long = "keep-journals")]
    pub keep_journals: Option<usize>,

    /// Prune old journals after this run (opt-in). Uses --keep-journals or config keep.
    #[arg(long = "prune-journals")]
    pub prune_journals: bool,

    /// Files or directories to process.
    pub paths: Vec<PathBuf>,
}

/// Arguments for the `undo` subcommand.
#[derive(Debug, clap::Args)]
pub struct UndoArgs {
    /// Journal file to reverse (default: most recent).
    #[arg(long = "journal")]
    pub journal: Option<PathBuf>,

    /// Show planned reversals without modifying the filesystem.
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,

    /// Report each reversal as it happens.
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,
}

/// Arguments for the `journals` subcommand.
#[derive(Debug, clap::Args)]
pub struct JournalsArgs {
    #[command(subcommand)]
    pub command: JournalsCommand,
}

/// `journals` operations.
#[derive(Debug, Subcommand)]
pub enum JournalsCommand {
    /// List saved journals (newest first).
    List,
    /// Delete all but the newest N journals.
    Prune(JournalsPruneArgs),
}

/// Arguments for `journals prune`.
#[derive(Debug, clap::Args)]
pub struct JournalsPruneArgs {
    /// Number of journals to keep (default: config value or 20).
    #[arg(long = "keep")]
    pub keep: Option<usize>,
    /// Show what would be removed without deleting.
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,
    /// Report each removed journal.
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,
    /// Use this config file instead of the default discovery.
    #[arg(short = 'f', long = "config")]
    pub config: Option<PathBuf>,
}

/// Arguments for the `init` subcommand.
#[derive(Debug, clap::Args)]
pub struct InitArgs {
    /// Overwrite an existing config file.
    #[arg(long = "force")]
    pub force: bool,

    /// Write to this path instead of the default location.
    #[arg(short = 'f', long = "config")]
    pub config: Option<PathBuf>,

    /// Show what would be written without creating the file.
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,
}

/// Strategy when a computed target name already exists on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OnCollision {
    /// Append a numeric suffix (`-1`, `-2`, …) before the extension.
    Suffix,
    /// Leave the source file unchanged.
    Skip,
    /// Replace the existing target.
    Overwrite,
}

impl Cli {
    /// Parse from the process arguments.
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli_definition() {
        // Ensures the clap derive is internally consistent.
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_default_flags() {
        let cli = Cli::try_parse_from(["rehab", "-n", "-r", "a b.txt"]).unwrap();
        assert!(cli.run.dry_run);
        assert!(cli.run.recursive);
        assert_eq!(cli.run.paths, vec![PathBuf::from("a b.txt")]);
        assert_eq!(cli.run.on_collision, OnCollision::Suffix);
    }

    #[test]
    fn parses_sequence_and_jobs() {
        let cli = Cli::try_parse_from(["rehab", "-s", "safe-lower", "-j", "4", "f"]).unwrap();
        assert_eq!(cli.run.sequence.as_deref(), Some("safe-lower"));
        assert_eq!(cli.run.jobs, Some(4));
    }

    #[test]
    fn parses_on_collision_variants() {
        let cli = Cli::try_parse_from(["rehab", "--on-collision", "skip", "f"]).unwrap();
        assert_eq!(cli.run.on_collision, OnCollision::Skip);
    }

    #[test]
    fn parses_undo_subcommand() {
        let cli = Cli::try_parse_from(["rehab", "undo", "-n"]).unwrap();
        match cli.command {
            Some(Command::Undo(args)) => assert!(args.dry_run),
            _ => panic!("expected undo subcommand"),
        }
    }

    #[test]
    fn parses_journals_list_and_prune() {
        let list = Cli::try_parse_from(["rehab", "journals", "list"]).unwrap();
        match list.command {
            Some(Command::Journals(j)) => {
                assert!(matches!(j.command, JournalsCommand::List))
            }
            _ => panic!("expected journals list"),
        }

        let prune = Cli::try_parse_from(["rehab", "journals", "prune", "--keep", "3"]).unwrap();
        match prune.command {
            Some(Command::Journals(j)) => match j.command {
                JournalsCommand::Prune(p) => assert_eq!(p.keep, Some(3)),
                _ => panic!("expected prune"),
            },
            _ => panic!("expected journals prune"),
        }
    }

    #[test]
    fn parses_keep_and_prune_journal_flags() {
        let cli = Cli::try_parse_from(["rehab", "--prune-journals", "--keep-journals", "5", "f"])
            .unwrap();
        assert!(cli.run.prune_journals);
        assert_eq!(cli.run.keep_journals, Some(5));
    }

    #[test]
    fn parses_init_subcommand() {
        let cli = Cli::try_parse_from(["rehab", "init", "--force", "-n"]).unwrap();
        match cli.command {
            Some(Command::Init(args)) => {
                assert!(args.force);
                assert!(args.dry_run);
            }
            _ => panic!("expected init subcommand"),
        }
    }

    #[test]
    fn rejects_unknown_flag() {
        assert!(Cli::try_parse_from(["rehab", "--nope"]).is_err());
    }
}
