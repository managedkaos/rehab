//! Per-run rename journal in JSON Lines (JSONL) format.
//!
//! Each `run` writes a timestamped journal file (one per invocation) under the
//! platform state directory (`~/.local/state/rehab/journal-<ts>.jsonl` on
//! Linux, via [`dirs::state_dir`], falling back to the data-local dir). Every
//! rename appends one [`Record`] as a single line of JSON.
//!
//! The [`JournalWriter`] wraps its file handle in a [`Mutex`] so it can be
//! shared across a rayon thread pool and appended to concurrently; each line is
//! written atomically under the lock and flushed, so a crash mid-run still
//! leaves a valid prefix of complete records. [`read_journal`] reads the
//! records back in the order they were written.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// A single rename event: the original path, the new path, and a timestamp
/// (milliseconds since the Unix epoch) when the record was created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The path before renaming.
    pub from: String,
    /// The path after renaming.
    pub to: String,
    /// Milliseconds since the Unix epoch.
    pub ts: u64,
}

impl Record {
    /// Create a record stamped with the current time.
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            ts: now_millis(),
        }
    }

    /// Create a record with an explicit timestamp (useful for tests).
    pub fn with_ts(from: impl Into<String>, to: impl Into<String>, ts: u64) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            ts,
        }
    }
}

/// Milliseconds since the Unix epoch. Saturates to 0 if the clock is before the
/// epoch (which should not happen in practice).
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A synchronized, append-only JSONL journal writer.
///
/// Cloneable handles are not provided; share a single writer behind an `Arc`
/// when using it across rayon tasks:
///
/// ```no_run
/// use std::sync::Arc;
/// use rehab::journal::{JournalWriter, Record};
/// let writer = Arc::new(JournalWriter::create("/tmp/journal.jsonl").unwrap());
/// // in parallel tasks:
/// writer.append(&Record::new("a", "b")).unwrap();
/// ```
pub struct JournalWriter {
    path: PathBuf,
    file: Mutex<File>,
}

impl JournalWriter {
    /// Create (or truncate) a journal file at `path`, creating parent
    /// directories as needed.
    pub fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        Ok(Self {
            path,
            file: Mutex::new(file),
        })
    }

    /// Open a new timestamped journal in the default state directory and return
    /// the writer. The file name is `journal-<ts>.jsonl`.
    pub fn create_in_state_dir() -> std::io::Result<Self> {
        let dir = journal_dir().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "could not determine a state directory for the journal",
            )
        })?;
        Self::create(dir.join(journal_file_name(now_millis())))
    }

    /// The path this journal writes to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one record as a single JSON line, then flush.
    ///
    /// The lock is held for the whole serialize + write + flush so records from
    /// concurrent rayon tasks never interleave within a line.
    pub fn append(&self, record: &Record) -> std::io::Result<()> {
        let line = serde_json::to_string(record)?;
        let mut file = self
            .file
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(())
    }
}

/// A journal writer that defers creating the file until the first record is
/// appended.
///
/// A run that renames nothing must not leave behind an empty journal, because
/// the empty file would be the newest one and would shadow the previous run's
/// real undo history. This wrapper resolves the target path up front (so path
/// errors surface early) but only touches the filesystem once there is
/// something to record.
///
/// It is `Send + Sync` and safe to share behind an `Arc` across rayon tasks:
/// the first `append` under the lock creates the file, and subsequent appends
/// reuse it.
pub struct LazyJournalWriter {
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl LazyJournalWriter {
    /// Prepare a lazy writer targeting `path`. No file is created yet.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            file: Mutex::new(None),
        }
    }

    /// Prepare a lazy writer targeting a new timestamped journal in the default
    /// state directory. Fails only if no state directory can be determined; the
    /// file itself is still created lazily.
    pub fn in_state_dir() -> std::io::Result<Self> {
        let dir = journal_dir().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "could not determine a state directory for the journal",
            )
        })?;
        Ok(Self::new(dir.join(journal_file_name(now_millis()))))
    }

    /// The path this journal will write to (whether or not it exists yet).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `true` if the journal file has actually been created (i.e. at least one
    /// record was appended).
    pub fn was_created(&self) -> bool {
        self.file.lock().map(|f| f.is_some()).unwrap_or(true)
    }

    /// Append one record, creating the file (and its parent directories) on the
    /// first call. The lock is held across create + serialize + write + flush
    /// so concurrent rayon tasks never interleave within a line and the file is
    /// created exactly once.
    pub fn append(&self, record: &Record) -> std::io::Result<()> {
        let line = serde_json::to_string(record)?;
        let mut guard = self
            .file
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.is_none() {
            if let Some(parent) = self.path.parent() {
                fs::create_dir_all(parent)?;
            }
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&self.path)?;
            *guard = Some(file);
        }
        let file = guard.as_mut().expect("file initialized above");
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(())
    }
}

/// Read all records from a journal file, in written order. Blank lines are
/// skipped; a malformed line produces an error.
pub fn read_journal(path: impl AsRef<Path>) -> std::io::Result<Vec<Record>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let record: Record = serde_json::from_str(&line)?;
        records.push(record);
    }
    Ok(records)
}

/// The directory where journals are stored: the platform state directory (or
/// data-local dir as a fallback) joined with `rehab`.
pub fn journal_dir() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .map(|d| d.join("rehab"))
}

/// The file name for a journal stamped at `ts` (ms since epoch).
pub fn journal_file_name(ts: u64) -> String {
    format!("journal-{ts}.jsonl")
}

/// Default number of journals to retain when pruning.
pub const DEFAULT_KEEP: usize = 20;

/// Metadata about a journal file on disk.
#[derive(Debug, Clone)]
pub struct JournalInfo {
    /// Full path to the journal file.
    pub path: PathBuf,
    /// Last-modified time, used for ordering.
    pub modified: SystemTime,
    /// Number of rename records in the file (0 if it could not be read).
    pub records: usize,
}

/// Return `true` if a path looks like a rehab journal (`journal-*.jsonl`).
pub fn is_journal_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("journal-") && n.ends_with(".jsonl"))
        .unwrap_or(false)
}

/// List journals in `dir`, newest first (by mtime, then path desc). Empty if
/// the directory does not exist or cannot be read.
pub fn list_journals(dir: &Path) -> Vec<JournalInfo> {
    let read = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut journals: Vec<JournalInfo> = read
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| is_journal_file(p))
        .map(|path| {
            let modified = fs::metadata(&path)
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH);
            let records = read_journal(&path).map(|r| r.len()).unwrap_or(0);
            JournalInfo {
                path,
                modified,
                records,
            }
        })
        .collect();
    journals.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| b.path.cmp(&a.path))
    });
    journals
}

/// Delete all but the `keep` newest journals in `dir`, returning removed paths.
/// `keep == 0` means unlimited (nothing pruned). Attempts all deletions and
/// returns the first error (if any) after trying the rest.
pub fn prune_journals(dir: &Path, keep: usize) -> std::io::Result<Vec<PathBuf>> {
    if keep == 0 {
        return Ok(Vec::new());
    }
    let journals = list_journals(dir);
    if journals.len() <= keep {
        return Ok(Vec::new());
    }
    let mut removed = Vec::new();
    let mut first_err: Option<std::io::Error> = None;
    for info in journals.into_iter().skip(keep) {
        match fs::remove_file(&info.path) {
            Ok(()) => removed.push(info.path),
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(removed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn is_journal_file_matches_pattern() {
        assert!(is_journal_file(Path::new("/x/journal-123.jsonl")));
        assert!(!is_journal_file(Path::new("/x/notes.txt")));
        assert!(!is_journal_file(Path::new("/x/journal-.txt")));
    }

    #[test]
    fn list_journals_orders_newest_first() {
        use std::thread::sleep;
        use std::time::Duration;
        let dir = tempdir().unwrap();
        for i in 0..3 {
            let w = LazyJournalWriter::new(dir.path().join(journal_file_name(i)));
            w.append(&Record::with_ts("a", "b", i)).unwrap();
            sleep(Duration::from_millis(10));
        }
        let list = list_journals(dir.path());
        assert_eq!(list.len(), 3);
        assert!(list[0].path.ends_with("journal-2.jsonl"));
        assert_eq!(list[0].records, 1);
    }

    #[test]
    fn prune_keeps_newest_n() {
        use std::thread::sleep;
        use std::time::Duration;
        let dir = tempdir().unwrap();
        for i in 0..5 {
            let w = LazyJournalWriter::new(dir.path().join(journal_file_name(i)));
            w.append(&Record::with_ts("a", "b", i)).unwrap();
            sleep(Duration::from_millis(10));
        }
        let removed = prune_journals(dir.path(), 2).unwrap();
        assert_eq!(removed.len(), 3);
        let left = list_journals(dir.path());
        assert_eq!(left.len(), 2);
        assert!(left[0].path.ends_with("journal-4.jsonl"));
        assert!(left[1].path.ends_with("journal-3.jsonl"));
    }

    #[test]
    fn prune_zero_keep_is_unlimited() {
        let dir = tempdir().unwrap();
        for i in 0..3 {
            let w = LazyJournalWriter::new(dir.path().join(journal_file_name(i)));
            w.append(&Record::with_ts("a", "b", i)).unwrap();
        }
        assert_eq!(prune_journals(dir.path(), 0).unwrap().len(), 0);
        assert_eq!(list_journals(dir.path()).len(), 3);
    }

    #[test]
    fn lazy_writer_creates_no_file_until_first_append() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested/journal.jsonl");
        let writer = LazyJournalWriter::new(&path);

        // Nothing on disk, and the writer reports it hasn't been created.
        assert!(!path.exists());
        assert!(!writer.was_created());

        // First append creates the file (and its parent directory).
        writer.append(&Record::with_ts("a b", "a_b", 1)).unwrap();
        assert!(path.exists());
        assert!(writer.was_created());

        let records = read_journal(&path).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].from, "a b");
    }

    #[test]
    fn lazy_writer_dropped_without_append_leaves_no_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("journal.jsonl");
        {
            let writer = LazyJournalWriter::new(&path);
            assert!(!writer.was_created());
            // No append; writer goes out of scope here.
        }
        assert!(
            !path.exists(),
            "an unused lazy writer must not create a file"
        );
    }

    #[test]
    fn lazy_writer_appends_are_reused_across_calls() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("journal.jsonl");
        let writer = Arc::new(LazyJournalWriter::new(&path));
        for i in 0..3 {
            writer
                .append(&Record::with_ts(format!("from{i}"), format!("to{i}"), i))
                .unwrap();
        }
        let records = read_journal(&path).unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[2].from, "from2");
    }
    use tempfile::tempdir;

    #[test]
    fn record_roundtrips_through_json() {
        let r = Record::with_ts("a b.txt", "a_b.txt", 123);
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"from\":\"a b.txt\""));
        assert!(s.contains("\"to\":\"a_b.txt\""));
        assert!(s.contains("\"ts\":123"));
        let back: Record = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn append_then_read_preserves_order() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("journal.jsonl");
        let writer = JournalWriter::create(&path).unwrap();
        for i in 0..5 {
            writer
                .append(&Record::with_ts(format!("from{i}"), format!("to{i}"), i))
                .unwrap();
        }
        let records = read_journal(&path).unwrap();
        assert_eq!(records.len(), 5);
        for (i, r) in records.iter().enumerate() {
            assert_eq!(r.from, format!("from{i}"));
            assert_eq!(r.to, format!("to{i}"));
        }
    }

    #[test]
    fn creates_parent_directories() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested/sub/journal.jsonl");
        let writer = JournalWriter::create(&path).unwrap();
        writer.append(&Record::with_ts("a", "b", 1)).unwrap();
        assert!(path.exists());
        assert_eq!(read_journal(&path).unwrap().len(), 1);
    }

    #[test]
    fn concurrent_appends_under_rayon_all_captured() {
        use rayon::prelude::*;

        let dir = tempdir().unwrap();
        let path = dir.path().join("journal.jsonl");
        let writer = Arc::new(JournalWriter::create(&path).unwrap());

        let n = 1000;
        (0..n).into_par_iter().for_each(|i| {
            writer
                .append(&Record::with_ts(format!("from{i}"), format!("to{i}"), i))
                .unwrap();
        });

        let records = read_journal(&path).unwrap();
        // Every append is captured and every line is well-formed (no
        // interleaving corrupted the JSON).
        assert_eq!(records.len() as u64, n);

        // The set of "from" fields matches exactly (order is not guaranteed
        // under parallelism, but completeness is).
        let mut froms: Vec<String> = records.into_iter().map(|r| r.from).collect();
        froms.sort();
        let mut expected: Vec<String> = (0..n).map(|i| format!("from{i}")).collect();
        expected.sort();
        assert_eq!(froms, expected);
    }

    #[test]
    fn empty_lines_are_skipped_by_reader() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("journal.jsonl");
        {
            let mut f = File::create(&path).unwrap();
            writeln!(
                f,
                "{}",
                serde_json::to_string(&Record::with_ts("a", "b", 1)).unwrap()
            )
            .unwrap();
            writeln!(f).unwrap(); // blank line
            writeln!(
                f,
                "{}",
                serde_json::to_string(&Record::with_ts("c", "d", 2)).unwrap()
            )
            .unwrap();
        }
        let records = read_journal(&path).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].from, "a");
        assert_eq!(records[1].from, "c");
    }

    #[test]
    fn malformed_line_errors() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("journal.jsonl");
        {
            let mut f = File::create(&path).unwrap();
            writeln!(f, "not json at all").unwrap();
        }
        assert!(read_journal(&path).is_err());
    }

    #[test]
    fn journal_file_name_format() {
        assert_eq!(journal_file_name(42), "journal-42.jsonl");
    }

    #[test]
    fn now_millis_is_nonzero() {
        assert!(now_millis() > 0);
    }
}
