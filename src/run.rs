//! Run orchestration (integration of Tasks 3, 5, 10, 11, 12).
//!
//! Ties the pieces together: resolve a sequence from config, walk the paths,
//! compute cleaned names in parallel with rayon, then commit renames in a
//! single deterministic serialized pass (so numeric-suffix collision handling
//! is race-free), journaling each rename. Also implements `undo`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::ThreadPoolBuilder;
use rayon::prelude::*;

use crate::cli::{RunArgs, UndoArgs};
use crate::config::Config;
use crate::journal::{self, JournalWriter, Record};
use crate::rename::{self, Plan};
use crate::sequence::Sequence;
use crate::walk;

/// Top-level error for a run.
#[derive(Debug)]
pub enum RunError {
    Config(String),
    Io(std::io::Error),
    NoPaths,
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Config(m) => write!(f, "{m}"),
            RunError::Io(e) => write!(f, "{e}"),
            RunError::NoPaths => write!(f, "error: no paths given"),
        }
    }
}

impl std::error::Error for RunError {}

impl From<std::io::Error> for RunError {
    fn from(e: std::io::Error) -> Self {
        RunError::Io(e)
    }
}

/// Compute the cleaned basename of a path using the sequence.
fn cleaned_basename(path: &Path, seq: &Sequence) -> Option<String> {
    let name = path.file_name()?.to_string_lossy();
    Some(seq.apply(&name))
}

/// Execute the default run behavior.
pub fn run(args: &RunArgs) -> Result<(), RunError> {
    let config =
        Config::discover(args.config.as_deref()).map_err(|e| RunError::Config(e.to_string()))?;

    // --list-sequences short-circuits.
    if args.list_sequences {
        for (name, filters) in config.list_sequences() {
            println!("{name}: {}", filters.join(", "));
        }
        return Ok(());
    }

    if args.paths.is_empty() {
        return Err(RunError::NoPaths);
    }

    let seq = match &args.sequence {
        Some(name) => config
            .resolve_sequence(name)
            .map_err(|e| RunError::Config(e.to_string()))?,
        None => config
            .resolve_default()
            .map_err(|e| RunError::Config(e.to_string()))?,
    };

    let dry_run = args.dry_run;
    let verbose = args.verbose || dry_run;

    // Configure the rayon pool if a job count was requested.
    if let Some(jobs) = args.jobs {
        // Best-effort: ignore if a global pool already exists.
        let _ = ThreadPoolBuilder::new()
            .num_threads(jobs.max(1))
            .build_global();
    }

    // Traverse to get the entries to process (deepest-first in recursive mode).
    let entries = walk::collect(&args.paths, args.recursive);

    // Phase 1: compute cleaned names in parallel (pure, no FS mutation).
    let mut computed: Vec<(PathBuf, String)> = entries
        .par_iter()
        .filter_map(|p| cleaned_basename(p, &seq).map(|c| (p.clone(), c)))
        .collect();

    // Deterministic order for the serialized commit so suffix numbering is
    // stable across runs and thread counts. Preserve deepest-first by sorting
    // on (reverse component-count, path).
    computed.sort_by(|a, b| {
        let da = a.0.components().count();
        let db = b.0.components().count();
        db.cmp(&da).then_with(|| a.0.cmp(&b.0))
    });

    // Phase 2: serialized commit — collision resolution + rename + journal.
    let writer = if dry_run {
        None
    } else {
        Some(Arc::new(JournalWriter::create_in_state_dir()?))
    };

    for (path, cleaned) in &computed {
        let plan = match rename::plan_rename(path, cleaned, args.on_collision) {
            Some(p) => p,
            None => {
                if verbose {
                    println!("skip: {}", path.display());
                }
                continue;
            }
        };
        commit(&plan, dry_run, verbose, writer.as_deref())?;
    }

    if let Some(w) = &writer
        && verbose
    {
        eprintln!("journal: {}", w.path().display());
    }

    Ok(())
}

/// Commit a single plan: rename on disk (unless dry-run) and journal it.
fn commit(
    plan: &Plan,
    dry_run: bool,
    verbose: bool,
    writer: Option<&JournalWriter>,
) -> Result<(), RunError> {
    rename::execute(plan, dry_run, verbose)?;
    if !dry_run && let Some(w) = writer {
        w.append(&Record::new(
            plan.from.to_string_lossy().to_string(),
            plan.to.to_string_lossy().to_string(),
        ))?;
    }
    Ok(())
}

/// Execute the `undo` subcommand: reverse the renames from a journal.
pub fn undo(args: &UndoArgs) -> Result<(), RunError> {
    let path = match &args.journal {
        Some(p) => p.clone(),
        None => latest_journal()?
            .ok_or_else(|| RunError::Config("no journal found to undo".to_string()))?,
    };

    let mut records = journal::read_journal(&path)?;
    // Reverse in inverse order so later renames are undone first.
    records.reverse();

    let verbose = args.verbose || args.dry_run;
    for r in &records {
        // To undo, move `to` back to `from`.
        let from = Path::new(&r.to);
        let to = Path::new(&r.from);
        if verbose {
            println!("{} -> {}", from.display(), to.display());
        }
        if args.dry_run {
            continue;
        }
        if !from.exists() {
            eprintln!("warning: source missing, skipping: {}", from.display());
            continue;
        }
        if to.exists() {
            eprintln!("warning: target exists, skipping: {}", to.display());
            continue;
        }
        if let Err(e) = std::fs::rename(from, to) {
            eprintln!("warning: failed to undo {}: {e}", from.display());
        }
    }
    Ok(())
}

/// Find the most recent journal file in the state directory.
fn latest_journal() -> Result<Option<PathBuf>, RunError> {
    let dir = match journal::journal_dir() {
        Some(d) => d,
        None => return Ok(None),
    };
    if !dir.exists() {
        return Ok(None);
    }
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let p = entry.path();
        let is_journal = p
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("journal-") && n.ends_with(".jsonl"))
            .unwrap_or(false);
        if !is_journal {
            continue;
        }
        let mtime = entry.metadata()?.modified()?;
        if newest.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
            newest = Some((mtime, p));
        }
    }
    Ok(newest.map(|(_, p)| p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::OnCollision;
    use crate::sequence;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn cleaned_basename_uses_sequence() {
        let seq = sequence::builtin("safe").unwrap();
        let got = cleaned_basename(Path::new("/tmp/a b.txt"), &seq);
        assert_eq!(got.as_deref(), Some("a_b.txt"));
    }

    #[test]
    fn run_renames_a_single_file() {
        let dir = tempdir().unwrap();
        let bad = dir.path().join("a b.txt");
        fs::write(&bad, b"x").unwrap();

        let args = RunArgs {
            dry_run: false,
            recursive: false,
            verbose: false,
            sequence: Some("safe".to_string()),
            jobs: Some(1),
            on_collision: OnCollision::Suffix,
            config: None,
            list_sequences: false,
            paths: vec![bad.clone()],
        };
        run(&args).unwrap();

        assert!(!bad.exists());
        assert!(dir.path().join("a_b.txt").exists());
    }

    #[test]
    fn dry_run_does_not_rename() {
        let dir = tempdir().unwrap();
        let bad = dir.path().join("a b.txt");
        fs::write(&bad, b"x").unwrap();

        let args = RunArgs {
            dry_run: true,
            recursive: false,
            verbose: false,
            sequence: Some("safe".to_string()),
            jobs: Some(1),
            on_collision: OnCollision::Suffix,
            config: None,
            list_sequences: false,
            paths: vec![bad.clone()],
        };
        run(&args).unwrap();

        assert!(bad.exists(), "dry-run must not modify the filesystem");
        assert!(!dir.path().join("a_b.txt").exists());
    }
}
