//! Command-line interface definitions (Task 2).
//!
//! Defines the default `run` behavior plus an `undo` subcommand. Flags mirror
//! the plan: `--dry-run/-n`, `--recursive/-r`, `--verbose/-v`, `--sequence/-s`,
//! `--jobs/-j`, `--on-collision`, `--config/-f`, `--list-sequences/-L`.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// rehab — clean up filenames by replacing problematic characters.
#[derive(Debug, Parser)]
#[command(name = "rehab", version, about, long_about = None, disable_version_flag = true)]
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
}

/// Arguments for the default run behavior.
#[derive(Debug, clap::Args)]
pub struct RunArgs {
    /// Show planned changes without modifying the filesystem (implies verbose).
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,

    /// Recurse into subdirectories.
    #[arg(short = 'r', long = "recursive")]
    pub recursive: bool,

    /// Report each rename as it happens.
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
    fn rejects_unknown_flag() {
        assert!(Cli::try_parse_from(["rehab", "--nope"]).is_err());
    }
}
