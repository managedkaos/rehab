//! Renaming and collision resolution (Tasks 3 & 4).
//!
//! Computes a cleaned basename via the filter pipeline and renames the entry on
//! disk. `--dry-run` prints the plan without touching the filesystem;
//! `--verbose` reports each rename. Collision handling appends numeric suffixes
//! before the extension, or skips/overwrites per policy.

use std::path::{Path, PathBuf};

use crate::cli::OnCollision;

/// A single planned rename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// Split a filename into (stem, extension-with-dot). Only the final extension
/// is considered, and dotfiles (leading `.`) are treated as having no
/// extension so `.gitignore` keeps its name.
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(idx) if idx > 0 => (&name[..idx], &name[idx..]),
        _ => (name, ""),
    }
}

/// Given a directory and a desired cleaned basename, produce a target path that
/// does not collide, honoring the collision policy. Returns `None` when the
/// policy is `Skip` and the target already exists.
///
/// `exists` is injected so this is testable without touching the filesystem.
pub fn resolve_collision<F>(
    dir: &Path,
    cleaned: &str,
    policy: OnCollision,
    original: &Path,
    exists: F,
) -> Option<PathBuf>
where
    F: Fn(&Path) -> bool,
{
    let candidate = dir.join(cleaned);

    // If nothing changes, keep as-is.
    if candidate == original {
        return Some(candidate);
    }
    if !exists(&candidate) {
        return Some(candidate);
    }

    match policy {
        OnCollision::Overwrite => Some(candidate),
        OnCollision::Skip => None,
        OnCollision::Suffix => {
            let (stem, ext) = split_ext(cleaned);
            for n in 1..=usize::MAX {
                let name = format!("{stem}-{n}{ext}");
                let cand = dir.join(&name);
                if cand == original || !exists(&cand) {
                    return Some(cand);
                }
            }
            None
        }
    }
}

/// Compute the rename plan for a single path given its cleaned basename.
/// Returns `None` when the name is unchanged or the collision policy skips it.
pub fn plan_rename(path: &Path, cleaned: &str, policy: OnCollision) -> Option<Plan> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let target = resolve_collision(dir, cleaned, policy, path, |p| p.exists())?;
    if target == path {
        return None;
    }
    Some(Plan {
        from: path.to_path_buf(),
        to: target,
    })
}

/// Execute a plan on disk. When `dry_run` is set, nothing is changed.
pub fn execute(plan: &Plan, dry_run: bool, verbose: bool) -> std::io::Result<()> {
    if dry_run || verbose {
        println!("{} -> {}", plan.from.display(), plan.to.display());
    }
    if !dry_run {
        std::fs::rename(&plan.from, &plan.to)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_ext_basic() {
        assert_eq!(split_ext("file.txt"), ("file", ".txt"));
        assert_eq!(split_ext("archive.tar.gz"), ("archive.tar", ".gz"));
        assert_eq!(split_ext("noext"), ("noext", ""));
        assert_eq!(split_ext(".gitignore"), (".gitignore", ""));
    }

    #[test]
    fn no_collision_returns_candidate() {
        let dir = Path::new("/tmp");
        let out = resolve_collision(
            dir,
            "clean.txt",
            OnCollision::Suffix,
            Path::new("/tmp/dirty.txt"),
            |_| false,
        );
        assert_eq!(out, Some(PathBuf::from("/tmp/clean.txt")));
    }

    #[test]
    fn suffix_avoids_existing() {
        let dir = Path::new("/tmp");
        let taken = ["/tmp/a_b.txt"];
        let out = resolve_collision(
            dir,
            "a_b.txt",
            OnCollision::Suffix,
            Path::new("/tmp/a b.txt"),
            |p| taken.contains(&p.to_str().unwrap()),
        );
        assert_eq!(out, Some(PathBuf::from("/tmp/a_b-1.txt")));
    }

    #[test]
    fn suffix_increments_until_free() {
        let dir = Path::new("/tmp");
        let taken = ["/tmp/a_b.txt", "/tmp/a_b-1.txt", "/tmp/a_b-2.txt"];
        let out = resolve_collision(
            dir,
            "a_b.txt",
            OnCollision::Suffix,
            Path::new("/tmp/a b.txt"),
            |p| taken.contains(&p.to_str().unwrap()),
        );
        assert_eq!(out, Some(PathBuf::from("/tmp/a_b-3.txt")));
    }

    #[test]
    fn skip_returns_none_on_collision() {
        let dir = Path::new("/tmp");
        let out = resolve_collision(
            dir,
            "a_b.txt",
            OnCollision::Skip,
            Path::new("/tmp/a b.txt"),
            |_| true,
        );
        assert_eq!(out, None);
    }

    #[test]
    fn overwrite_returns_candidate_despite_collision() {
        let dir = Path::new("/tmp");
        let out = resolve_collision(
            dir,
            "a_b.txt",
            OnCollision::Overwrite,
            Path::new("/tmp/a b.txt"),
            |_| true,
        );
        assert_eq!(out, Some(PathBuf::from("/tmp/a_b.txt")));
    }

    #[test]
    fn unchanged_name_is_kept() {
        let dir = Path::new("/tmp");
        let out = resolve_collision(
            dir,
            "clean.txt",
            OnCollision::Suffix,
            Path::new("/tmp/clean.txt"),
            |_| true,
        );
        assert_eq!(out, Some(PathBuf::from("/tmp/clean.txt")));
    }
}
