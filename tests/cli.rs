use std::fs;
use std::process::Command;

use tempfile::tempdir;

fn rehab() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rehab"))
}

/// A `rehab` command whose state/config directories are isolated to `home`, so
/// journal files land in a per-test location instead of the real state dir.
/// Sets the vars `dirs` consults on both macOS (`HOME`) and Linux (XDG).
fn rehab_isolated(home: &std::path::Path) -> Command {
    let mut cmd = rehab();
    cmd.env("HOME", home)
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CONFIG_HOME", home.join("config"));
    cmd
}

/// Count journal files across the plausible state locations under `home`.
fn journal_count(home: &std::path::Path) -> usize {
    let candidates = [
        home.join("state/rehab"),
        home.join("data/rehab"),
        home.join(".local/state/rehab"),
        home.join(".local/share/rehab"),
        home.join("Library/Application Support/rehab"),
    ];
    candidates
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|rd| rd.filter_map(|e| e.ok()))
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.starts_with("journal-") && n.ends_with(".jsonl"))
                .unwrap_or(false)
        })
        .count()
}

#[test]
fn prints_help() {
    let output = rehab().arg("--help").output().expect("run rehab");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("rehab"));
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--recursive"));
}

#[test]
fn rejects_unknown_flag() {
    let output = rehab()
        .arg("--definitely-not-a-flag")
        .output()
        .expect("run");
    assert!(!output.status.success());
}

#[test]
fn lists_sequences() {
    let output = rehab().arg("-L").output().expect("run rehab");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("default"));
    assert!(stdout.contains("safe"));
}

#[test]
fn renames_a_bad_name() {
    let dir = tempdir().unwrap();
    let bad = dir.path().join("bad name.txt");
    fs::write(&bad, b"x").unwrap();

    let output = rehab()
        .args(["-s", "safe"])
        .arg(&bad)
        .output()
        .expect("run rehab");
    assert!(output.status.success());
    assert!(!bad.exists());
    assert!(dir.path().join("bad_name.txt").exists());
}

#[test]
fn dry_run_leaves_files_untouched_but_prints_plan() {
    let dir = tempdir().unwrap();
    let bad = dir.path().join("bad name.txt");
    fs::write(&bad, b"x").unwrap();

    let output = rehab()
        .args(["-n", "-s", "safe"])
        .arg(&bad)
        .output()
        .expect("run rehab");
    assert!(output.status.success());
    assert!(bad.exists(), "dry-run must not modify the filesystem");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("bad_name.txt"));
}

#[test]
fn recursive_cleans_nested_tree() {
    let dir = tempdir().unwrap();
    let sub = dir.path().join("sub dir");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("deep file.txt"), b"x").unwrap();

    let output = rehab()
        .args(["-r", "-s", "safe"])
        .arg(dir.path())
        .output()
        .expect("run rehab");
    assert!(output.status.success());

    // The deep file and the subdirectory should both be cleaned.
    assert!(dir.path().join("sub_dir").exists());
    assert!(dir.path().join("sub_dir").join("deep_file.txt").exists());
}

#[test]
fn errors_without_paths() {
    let output = rehab().output().expect("run rehab");
    assert!(!output.status.success());
}

#[test]
fn dry_run_hides_skips_unless_verbose() {
    let dir = tempdir().unwrap();
    // One file needs cleaning, one is already clean (will be skipped).
    fs::write(dir.path().join("bad name.txt"), b"x").unwrap();
    fs::write(dir.path().join("clean.txt"), b"x").unwrap();

    // Dry-run without -v: the plan lists the rename but NOT the skip.
    let out = rehab()
        .args(["-n", "-s", "safe"])
        .arg(dir.path())
        .output()
        .expect("run rehab");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("bad_name.txt"), "rename should be reported");
    assert!(
        !stdout.contains("skip:"),
        "skips must be hidden without --verbose, got:\n{stdout}"
    );

    // Dry-run WITH -v: the skip is reported too.
    let out_v = rehab()
        .args(["-n", "-v", "-s", "safe"])
        .arg(dir.path())
        .output()
        .expect("run rehab");
    assert!(out_v.status.success());
    let stdout_v = String::from_utf8_lossy(&out_v.stdout);
    assert!(stdout_v.contains("skip:"), "verbose should report skips");
    assert!(
        stdout_v.contains("clean.txt"),
        "the skipped file should be named under verbose"
    );
}

#[test]
fn non_recursive_directory_arg_cleans_immediate_contents() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("bad name.txt"), b"x").unwrap();
    let sub = dir.path().join("sub dir");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("deep file.txt"), b"x").unwrap();

    // Name the directory itself, without -r.
    let output = rehab()
        .args(["-s", "safe"])
        .arg(dir.path())
        .output()
        .expect("run rehab");
    assert!(output.status.success());

    // Immediate file cleaned; immediate subdir cleaned; but NOT descended.
    assert!(dir.path().join("bad_name.txt").exists());
    assert!(dir.path().join("sub_dir").exists());
    // The deep file keeps its original name (no descent).
    assert!(dir.path().join("sub_dir").join("deep file.txt").exists());
}

#[test]
fn run_then_undo_restores_original_names() {
    let home = tempdir().unwrap();
    let work = tempdir().unwrap();
    let bad = work.path().join("bad name.txt");
    fs::write(&bad, b"x").unwrap();

    // Real run (isolated state dir) renames the file and writes a journal.
    let run = rehab_isolated(home.path())
        .args(["-s", "safe"])
        .arg(&bad)
        .output()
        .expect("run rehab");
    assert!(run.status.success());
    assert!(!bad.exists());
    assert!(work.path().join("bad_name.txt").exists());
    assert_eq!(
        journal_count(home.path()),
        1,
        "run should write one journal"
    );

    // Undo (most recent journal) restores the original name.
    let undo = rehab_isolated(home.path())
        .arg("undo")
        .output()
        .expect("run rehab undo");
    assert!(undo.status.success());
    assert!(bad.exists(), "undo should restore the original name");
    assert!(!work.path().join("bad_name.txt").exists());
}

#[test]
fn run_with_no_renames_writes_no_journal() {
    let home = tempdir().unwrap();
    let work = tempdir().unwrap();
    // Already-clean files: nothing to rename.
    fs::write(work.path().join("clean.txt"), b"x").unwrap();
    fs::write(work.path().join("also_clean.txt"), b"x").unwrap();

    let out = rehab_isolated(home.path())
        .args(["-s", "safe"])
        .arg(work.path())
        .output()
        .expect("run rehab");
    assert!(out.status.success());

    // No journal file was created, so a later `undo` finds nothing to undo
    // (rather than picking up an empty journal that shadows real history).
    assert_eq!(
        journal_count(home.path()),
        0,
        "a run that renames nothing must not create a journal"
    );

    let undo = rehab_isolated(home.path())
        .arg("undo")
        .output()
        .expect("run rehab undo");
    assert!(!undo.status.success(), "undo with no journal should error");
}

#[test]
fn journals_list_and_prune() {
    let home = tempdir().unwrap();
    let work = tempdir().unwrap();
    for name in ["a one.txt", "b two.txt", "c three.txt"] {
        let f = work.path().join(name);
        std::fs::write(&f, b"x").unwrap();
        assert!(
            rehab_isolated(home.path())
                .args(["-s", "safe"])
                .arg(&f)
                .output()
                .unwrap()
                .status
                .success()
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(journal_count(home.path()), 3);

    let list = rehab_isolated(home.path())
        .args(["journals", "list"])
        .output()
        .unwrap();
    assert!(list.status.success());
    assert_eq!(
        String::from_utf8_lossy(&list.stdout)
            .matches("renames")
            .count(),
        3
    );

    let prune = rehab_isolated(home.path())
        .args(["journals", "prune", "--keep", "1"])
        .output()
        .unwrap();
    assert!(prune.status.success());
    assert_eq!(journal_count(home.path()), 1);
}

#[test]
fn auto_prune_is_opt_in() {
    let home = tempdir().unwrap();
    let work = tempdir().unwrap();
    for name in ["a one.txt", "b two.txt"] {
        let f = work.path().join(name);
        std::fs::write(&f, b"x").unwrap();
        rehab_isolated(home.path())
            .args(["-s", "safe"])
            .arg(&f)
            .output()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // No auto-prune requested: both journals remain.
    assert_eq!(journal_count(home.path()), 2);

    // Third run WITH --prune-journals --keep-journals 1 collapses to 1.
    let f = work.path().join("c three.txt");
    std::fs::write(&f, b"x").unwrap();
    rehab_isolated(home.path())
        .args(["-s", "safe", "--prune-journals", "--keep-journals", "1"])
        .arg(&f)
        .output()
        .unwrap();
    assert_eq!(journal_count(home.path()), 1);
}
