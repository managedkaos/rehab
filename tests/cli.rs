use std::fs;
use std::process::Command;

use tempfile::tempdir;

fn rehab() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rehab"))
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
