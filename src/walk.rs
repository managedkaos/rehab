//! Filesystem traversal (Task 5).
//!
//! Collects entries to process. In recursive mode we descend into
//! subdirectories using `walkdir`, skipping hidden entries (leading `.`) unless
//! they were named explicitly on the command line. Entries are returned
//! deepest-first (children before parents) so that renaming a directory does
//! not invalidate the paths of entries beneath it.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Return `true` if a path's file name begins with a dot.
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

/// Collect the entries to process for a set of command-line paths.
///
/// - Non-recursive: files named directly pass through unchanged. A directory
///   named directly is expanded to its immediate, non-hidden children (one
///   level, no descent) — so `rehab .` cleans the current directory's contents,
///   matching `detox .`. The directory itself is not renamed.
/// - Recursive: descends into directories, skipping hidden entries, and returns
///   results deepest-first so child renames commit before their parents.
pub fn collect(paths: &[PathBuf], recursive: bool) -> Vec<PathBuf> {
    if !recursive {
        let mut out: Vec<PathBuf> = Vec::new();
        for path in paths {
            if path.is_dir() {
                // Expand a directory to its immediate non-hidden children.
                out.extend(immediate_children(path));
            } else {
                // A file (or non-existent path) is processed as named.
                out.push(path.clone());
            }
        }
        return out;
    }

    let mut out: Vec<(usize, PathBuf)> = Vec::new();
    for root in paths {
        for entry in WalkDir::new(root).into_iter().filter_entry(|e| {
            // Always allow the root itself; otherwise skip hidden entries.
            e.depth() == 0 || !is_hidden(e.path())
        }) {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            // Do not rename a directory passed as a traversal root: renaming it
            // would move the whole tree we were asked to descend and invalidate
            // every child path. A root that is a *file* is still renamed.
            if entry.depth() == 0 && entry.file_type().is_dir() {
                continue;
            }
            out.push((entry.depth(), entry.path().to_path_buf()));
        }
    }

    // Deepest first so children are renamed before parents.
    out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    out.into_iter().map(|(_, p)| p).collect()
}

/// List a directory's immediate, non-hidden children in a stable (sorted)
/// order. Returns an empty vec if the directory cannot be read.
fn immediate_children(dir: &Path) -> Vec<PathBuf> {
    let mut children: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| !is_hidden(p))
            .collect(),
        Err(_) => Vec::new(),
    };
    children.sort();
    children
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn non_recursive_passes_through_files() {
        // Non-directory paths are returned as named.
        let paths = vec![PathBuf::from("a b.txt"), PathBuf::from("c d.txt")];
        assert_eq!(collect(&paths, false), paths);
    }

    #[test]
    fn non_recursive_expands_directory_to_children() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("one file.txt"), b"x").unwrap();
        fs::write(root.join("two.txt"), b"x").unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub").join("deep.txt"), b"x").unwrap();

        let got = collect(&[root.to_path_buf()], false);

        // Immediate children are included (files and the subdir itself)...
        assert!(got.iter().any(|p| p.ends_with("one file.txt")));
        assert!(got.iter().any(|p| p.ends_with("two.txt")));
        assert!(got.iter().any(|p| p.ends_with("sub")));
        // ...but it does NOT descend into the subdirectory.
        assert!(!got.iter().any(|p| p.ends_with("deep.txt")));
        // The directory root itself is not in the output.
        assert!(!got.iter().any(|p| p == root));
    }

    #[test]
    fn non_recursive_directory_expansion_skips_hidden() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("visible.txt"), b"x").unwrap();
        fs::write(root.join(".hidden"), b"x").unwrap();
        fs::create_dir(root.join(".git")).unwrap();

        let got = collect(&[root.to_path_buf()], false);
        assert!(got.iter().any(|p| p.ends_with("visible.txt")));
        assert!(!got.iter().any(|p| p.to_string_lossy().contains(".hidden")));
        assert!(!got.iter().any(|p| p.to_string_lossy().contains(".git")));
    }

    #[test]
    fn recursive_descends_and_orders_deepest_first() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("top.txt"), b"x").unwrap();
        fs::write(root.join("sub").join("deep.txt"), b"x").unwrap();

        let got = collect(&[root.to_path_buf()], true);

        let pos_deep = got
            .iter()
            .position(|p| p.ends_with("sub/deep.txt"))
            .unwrap();
        let pos_sub = got.iter().position(|p| p.ends_with("sub")).unwrap();
        // Deeper entry must come before its parent directory.
        assert!(pos_deep < pos_sub);
    }

    #[test]
    fn recursive_skips_hidden_entries() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git").join("cfg"), b"x").unwrap();
        fs::write(root.join("keep.txt"), b"x").unwrap();

        let got = collect(&[root.to_path_buf()], true);
        assert!(got.iter().any(|p| p.ends_with("keep.txt")));
        assert!(!got.iter().any(|p| p.to_string_lossy().contains(".git")));
    }

    #[test]
    fn recursive_does_not_include_directory_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("child.txt"), b"x").unwrap();

        let got = collect(&[root.to_path_buf()], true);
        // The traversal root directory itself must not be returned for rename.
        assert!(!got.iter().any(|p| p == root));
        assert!(got.iter().any(|p| p.ends_with("child.txt")));
    }

    #[test]
    fn recursive_includes_file_root() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("a file.txt");
        fs::write(&file, b"x").unwrap();

        let inputs = [file.clone()];
        let got = collect(&inputs, true);
        // A file passed directly is still eligible for renaming.
        assert!(got.contains(&file));
    }
}
