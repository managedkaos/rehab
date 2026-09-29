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
use crate::journal::{self, LazyJournalWriter, Record};
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

    // Capture journal rotation settings before `config` is consumed below.
    let auto_prune_enabled = args.prune_journals || config.journal.auto_prune;
    let effective_keep = args.keep_journals.unwrap_or_else(|| config.journal_keep());

    let seq = match &args.sequence {
        Some(name) => config
            .resolve_sequence(name)
            .map_err(|e| RunError::Config(e.to_string()))?,
        None => config
            .resolve_default()
            .map_err(|e| RunError::Config(e.to_string()))?,
    };

    let dry_run = args.dry_run;
    // Renames (and the dry-run plan) are reported when dry-run or verbose.
    let verbose = args.verbose || dry_run;
    // Skipped files (unchanged names, or collisions under --on-collision skip)
    // are noisy, so they are only reported when the user explicitly asks for
    // verbose output — not merely because this is a dry run.
    let report_skips = args.verbose;

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
    // The writer is lazy: the journal file is created only on the first actual
    // rename, so a run that renames nothing leaves no empty journal to shadow
    // the previous run's undo history.
    let writer = if dry_run {
        None
    } else {
        Some(Arc::new(LazyJournalWriter::in_state_dir()?))
    };

    for (path, cleaned) in &computed {
        let plan = match rename::plan_rename(path, cleaned, args.on_collision) {
            Some(p) => p,
            None => {
                if report_skips {
                    println!("skip: {}", path.display());
                }
                continue;
            }
        };
        commit(&plan, dry_run, verbose, writer.as_deref())?;
    }

    // Only mention the journal if it was actually written to.
    if let Some(w) = &writer
        && verbose
        && w.was_created()
    {
        eprintln!("journal: {}", w.path().display());
    }

    // Opt-in journal rotation after a run that actually wrote a journal.
    if !dry_run
        && auto_prune_enabled
        && let Some(dir) = journal::journal_dir()
    {
        let removed = journal::prune_journals(&dir, effective_keep).unwrap_or_default();
        if verbose && !removed.is_empty() {
            eprintln!("pruned {} old journal(s)", removed.len());
        }
    }

    Ok(())
}

/// Commit a single plan: rename on disk (unless dry-run) and journal it.
fn commit(
    plan: &Plan,
    dry_run: bool,
    verbose: bool,
    writer: Option<&LazyJournalWriter>,
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

/// The default config template, embedded at build time. Written verbatim by
/// `rehab init`; kept in sync with `docs/config.default.toml`.
const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../docs/config.default.toml");

/// `init`: write the default config to the standard location (or `-f` path).
///
/// Refuses to overwrite an existing file unless `--force`. Creates parent
/// directories as needed, then validates the written file by parsing it back.
pub fn init(args: &crate::cli::InitArgs) -> Result<(), RunError> {
    let path = match &args.config {
        Some(p) => p.clone(),
        None => crate::config::default_config_path().ok_or_else(|| {
            RunError::Config("could not determine the default config location".into())
        })?,
    };

    if args.dry_run {
        println!("would write default config to {}", path.display());
        println!("---");
        print!("{DEFAULT_CONFIG_TEMPLATE}");
        return Ok(());
    }

    if path.exists() && !args.force {
        return Err(RunError::Config(format!(
            "config already exists at {} (use --force to overwrite)",
            path.display()
        )));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, DEFAULT_CONFIG_TEMPLATE)?;

    // Validate what we just wrote by parsing it back.
    Config::from_path(&path).map_err(|e| {
        RunError::Config(format!(
            "wrote {} but it failed to parse: {e}",
            path.display()
        ))
    })?;

    println!("wrote default config to {}", path.display());
    Ok(())
}

/// `journals list`: print saved journals, newest first.
pub fn journals_list() -> Result<(), RunError> {
    let dir = journal::journal_dir()
        .ok_or_else(|| RunError::Config("no state directory for journals".into()))?;
    let journals = journal::list_journals(&dir);
    if journals.is_empty() {
        println!("no journals in {}", dir.display());
        return Ok(());
    }
    for info in journals {
        println!("{}  ({} renames)", info.path.display(), info.records);
    }
    Ok(())
}

/// `journals prune`: keep the newest N journals (flag > config > default).
pub fn journals_prune(args: &crate::cli::JournalsPruneArgs) -> Result<(), RunError> {
    let config =
        Config::discover(args.config.as_deref()).map_err(|e| RunError::Config(e.to_string()))?;
    let keep = args.keep.unwrap_or_else(|| config.journal_keep());
    let dir = journal::journal_dir()
        .ok_or_else(|| RunError::Config("no state directory for journals".into()))?;

    if args.dry_run {
        let journals = journal::list_journals(&dir);
        let to_remove: Vec<_> = if keep == 0 {
            Vec::new()
        } else {
            journals.into_iter().skip(keep).collect()
        };
        if to_remove.is_empty() {
            println!("nothing to prune (keep = {keep})");
        } else {
            for info in &to_remove {
                println!("would remove: {}", info.path.display());
            }
        }
        return Ok(());
    }

    let removed = journal::prune_journals(&dir, keep)?;
    if args.verbose {
        for p in &removed {
            println!("removed: {}", p.display());
        }
    }
    println!("pruned {} journal(s), kept up to {keep}", removed.len());
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
    fn embedded_default_template_parses_and_matches_builtins() {
        let cfg = Config::from_toml_str(DEFAULT_CONFIG_TEMPLATE).unwrap();
        // The template should represent the built-in default behavior.
        assert_eq!(cfg.default_sequence_name(), "default");
        assert_eq!(
            cfg.resolve_default().unwrap().filter_names(),
            vec!["safe", "wipeup", "unicode-clean"]
        );
        assert_eq!(cfg.journal_keep(), 20);
        assert!(!cfg.journal.auto_prune);
    }

    #[test]
    fn init_writes_and_refuses_overwrite() {
        use crate::cli::InitArgs;
        let dir = tempdir().unwrap();
        let path = dir.path().join("sub/config.toml");

        // First write creates the file (and parent dir) and validates.
        let args = InitArgs {
            force: false,
            config: Some(path.clone()),
            dry_run: false,
        };
        init(&args).unwrap();
        assert!(path.exists());
        // Written file parses as the built-in default config.
        assert_eq!(
            Config::from_path(&path).unwrap().default_sequence_name(),
            "default"
        );

        // Second write without --force is refused.
        assert!(init(&args).is_err());

        // With --force it succeeds.
        let forced = InitArgs {
            force: true,
            config: Some(path.clone()),
            dry_run: false,
        };
        init(&forced).unwrap();
    }

    #[test]
    fn init_dry_run_writes_nothing() {
        use crate::cli::InitArgs;
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let args = InitArgs {
            force: false,
            config: Some(path.clone()),
            dry_run: true,
        };
        init(&args).unwrap();
        assert!(!path.exists(), "dry-run must not create the file");
    }

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
            keep_journals: None,
            prune_journals: false,
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
            keep_journals: None,
            prune_journals: false,
            paths: vec![bad.clone()],
        };
        run(&args).unwrap();

        assert!(bad.exists(), "dry-run must not modify the filesystem");
        assert!(!dir.path().join("a_b.txt").exists());
    }

    /// Helper: build UndoArgs pointing at an explicit journal.
    fn undo_args(journal: &Path, dry_run: bool) -> UndoArgs {
        UndoArgs {
            journal: Some(journal.to_path_buf()),
            dry_run,
            verbose: false,
        }
    }

    #[test]
    fn undo_restores_renamed_files() {
        // Rename a file, recording the rename in an explicit journal, then undo.
        let dir = tempdir().unwrap();
        let from = dir.path().join("a b.txt");
        let to = dir.path().join("a_b.txt");
        fs::write(&from, b"x").unwrap();
        fs::rename(&from, &to).unwrap();

        let journal = dir.path().join("journal.jsonl");
        {
            let w = LazyJournalWriter::new(&journal);
            w.append(&Record::new(
                from.to_string_lossy().to_string(),
                to.to_string_lossy().to_string(),
            ))
            .unwrap();
        }

        undo(&undo_args(&journal, false)).unwrap();

        // Original name restored; cleaned name gone.
        assert!(from.exists());
        assert!(!to.exists());
    }

    #[test]
    fn undo_dry_run_changes_nothing() {
        let dir = tempdir().unwrap();
        let from = dir.path().join("a b.txt");
        let to = dir.path().join("a_b.txt");
        fs::write(&to, b"x").unwrap();

        let journal = dir.path().join("journal.jsonl");
        {
            let w = LazyJournalWriter::new(&journal);
            w.append(&Record::new(
                from.to_string_lossy().to_string(),
                to.to_string_lossy().to_string(),
            ))
            .unwrap();
        }

        undo(&undo_args(&journal, true)).unwrap();

        // Dry-run: nothing moved.
        assert!(to.exists());
        assert!(!from.exists());
    }

    #[test]
    fn undo_skips_missing_source_and_occupied_target() {
        // Journal references a rename whose "to" no longer exists: undo must
        // skip it gracefully rather than error.
        let dir = tempdir().unwrap();
        let from = dir.path().join("gone from.txt");
        let to = dir.path().join("gone_from.txt"); // does not exist on disk

        let journal = dir.path().join("journal.jsonl");
        {
            let w = LazyJournalWriter::new(&journal);
            w.append(&Record::new(
                from.to_string_lossy().to_string(),
                to.to_string_lossy().to_string(),
            ))
            .unwrap();
        }

        // Should complete without error and without creating anything.
        undo(&undo_args(&journal, false)).unwrap();
        assert!(!from.exists());
        assert!(!to.exists());
    }
}
