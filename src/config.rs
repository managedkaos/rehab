//! Configuration: named sequences and defaults loaded from a TOML file.
//!
//! # Format
//!
//! ```toml
//! # Optional: name of the sequence to use when none is given on the CLI.
//! default = "myseq"
//!
//! [sequences.myseq]
//! filters = ["cgi-unescape", "safe", "wipeup", "lower"]
//! separator = "-"          # optional; passed through for consumers
//! on_collision = "skip"    # optional; passed through for consumers
//! ```
//!
//! # Discovery
//!
//! Resolution order (highest priority first):
//!
//! 1. An explicit path (`--config/-f`).
//! 2. `~/.config/rehab/config.toml` (via [`dirs::config_dir`]).
//! 3. Built-in sequences only (no file).
//!
//! Config-defined sequences merge *over* built-ins: a config sequence with the
//! same name as a built-in replaces it, and new names extend the set.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::sequence::{self, Sequence};

/// A single sequence entry as declared in the config file.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SequenceConfig {
    /// Ordered list of filter names making up the sequence.
    pub filters: Vec<String>,

    /// Optional separator override (consumed by callers building filters).
    #[serde(default)]
    pub separator: Option<String>,

    /// Optional collision policy hint (consumed by callers).
    #[serde(default)]
    pub on_collision: Option<String>,
}

/// Journal rotation settings.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
pub struct JournalConfig {
    /// Number of journals to retain when pruning. `None` -> DEFAULT_KEEP.
    #[serde(default)]
    pub keep: Option<usize>,
    /// Prune automatically after each run. Off by default.
    #[serde(default)]
    pub auto_prune: bool,
}

/// The parsed configuration file.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Config {
    /// Name of the default sequence, if the config overrides the built-in one.
    #[serde(default)]
    pub default: Option<String>,

    /// User-defined named sequences.
    #[serde(default)]
    pub sequences: BTreeMap<String, SequenceConfig>,

    /// Journal rotation settings.
    #[serde(default)]
    pub journal: JournalConfig,
}

/// Errors that can arise while loading or resolving configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// The config file could not be read.
    Io(std::io::Error),
    /// The config file was not valid TOML or did not match the schema.
    Parse(toml::de::Error),
    /// A named sequence referenced an unknown filter or an unknown sequence
    /// was requested.
    Resolve(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "failed to read config: {e}"),
            ConfigError::Parse(e) => write!(f, "failed to parse config: {e}"),
            ConfigError::Resolve(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Parse a [`Config`] from a TOML string.
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        toml::from_str(s).map_err(ConfigError::Parse)
    }

    /// Load a [`Config`] from a specific file path.
    pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path).map_err(ConfigError::Io)?;
        Self::from_toml_str(&text)
    }

    /// Discover and load configuration.
    ///
    /// If `explicit` is `Some`, that path is loaded (and its absence is an
    /// error). Otherwise the user config at `~/.config/rehab/config.toml` is
    /// loaded if present; if it does not exist, an empty [`Config`] is returned
    /// so only built-ins apply.
    pub fn discover(explicit: Option<&Path>) -> Result<Self, ConfigError> {
        if let Some(path) = explicit {
            return Self::from_path(path);
        }
        match default_config_path() {
            Some(path) if path.exists() => Self::from_path(&path),
            _ => Ok(Config::default()),
        }
    }

    /// The effective default sequence name: the config's `default` if set,
    /// otherwise the built-in default.
    pub fn default_sequence_name(&self) -> String {
        self.default
            .clone()
            .unwrap_or_else(|| sequence::DEFAULT_SEQUENCE.to_string())
    }

    /// Effective keep count: the configured value or the built-in default.
    pub fn journal_keep(&self) -> usize {
        self.journal.keep.unwrap_or(crate::journal::DEFAULT_KEEP)
    }

    /// Resolve a sequence by name, letting config sequences override built-ins.
    ///
    /// Lookup order: config sequences first (so they can shadow a built-in of
    /// the same name), then built-ins.
    pub fn resolve_sequence(&self, name: &str) -> Result<Sequence, ConfigError> {
        if let Some(cfg) = self.sequences.get(name) {
            let refs: Vec<&str> = cfg.filters.iter().map(String::as_str).collect();
            return Sequence::from_filter_names(name, &refs).map_err(ConfigError::Resolve);
        }
        sequence::resolve(name).map_err(ConfigError::Resolve)
    }

    /// Resolve the effective default sequence, honoring a config `default`.
    pub fn resolve_default(&self) -> Result<Sequence, ConfigError> {
        self.resolve_sequence(&self.default_sequence_name())
    }

    /// List all available sequences as `(name, filter-names)` pairs: built-ins
    /// with any config sequence of the same name overriding it, plus any
    /// config-only sequences. Sorted by name for stable output.
    pub fn list_sequences(&self) -> Vec<(String, Vec<String>)> {
        let mut merged: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, filters) in sequence::list_builtins() {
            merged.insert(
                name.to_string(),
                filters.into_iter().map(String::from).collect(),
            );
        }
        for (name, cfg) in &self.sequences {
            merged.insert(name.clone(), cfg.filters.clone());
        }
        merged.into_iter().collect()
    }
}

/// The default user config path: `~/.config/rehab/config.toml`.
pub fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rehab").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn empty_config_uses_builtin_default() {
        let cfg = Config::default();
        assert_eq!(cfg.default_sequence_name(), "default");
        let seq = cfg.resolve_default().unwrap();
        assert_eq!(seq.filter_names(), vec!["safe", "wipeup", "unicode-clean"]);
    }

    #[test]
    fn journal_config_defaults() {
        let cfg = Config::default();
        assert_eq!(cfg.journal_keep(), 20);
        assert!(!cfg.journal.auto_prune);
    }

    #[test]
    fn journal_config_parses() {
        let cfg = Config::from_toml_str("[journal]\nkeep = 5\nauto_prune = true\n").unwrap();
        assert_eq!(cfg.journal_keep(), 5);
        assert!(cfg.journal.auto_prune);
    }

    #[test]
    fn parses_custom_sequence_and_default() {
        let toml = r#"
            default = "myseq"

            [sequences.myseq]
            filters = ["cgi-unescape", "safe", "lower"]
            separator = "-"
            on_collision = "skip"
        "#;
        let cfg = Config::from_toml_str(toml).unwrap();
        assert_eq!(cfg.default, Some("myseq".to_string()));
        let myseq = cfg.sequences.get("myseq").unwrap();
        assert_eq!(myseq.filters, vec!["cgi-unescape", "safe", "lower"]);
        assert_eq!(myseq.separator.as_deref(), Some("-"));
        assert_eq!(myseq.on_collision.as_deref(), Some("skip"));
    }

    #[test]
    fn config_default_overrides_builtin() {
        let toml = r#"
            default = "myseq"
            [sequences.myseq]
            filters = ["safe", "lower"]
        "#;
        let cfg = Config::from_toml_str(toml).unwrap();
        assert_eq!(cfg.default_sequence_name(), "myseq");
        let seq = cfg.resolve_default().unwrap();
        assert_eq!(seq.name(), "myseq");
        assert_eq!(seq.filter_names(), vec!["safe", "lower"]);
    }

    #[test]
    fn config_sequence_shadows_builtin_of_same_name() {
        // Redefine the built-in "safe" to include lower.
        let toml = r#"
            [sequences.safe]
            filters = ["safe", "lower"]
        "#;
        let cfg = Config::from_toml_str(toml).unwrap();
        let seq = cfg.resolve_sequence("safe").unwrap();
        assert_eq!(seq.filter_names(), vec!["safe", "lower"]);
    }

    #[test]
    fn builtins_still_resolve_when_not_overridden() {
        let cfg = Config::from_toml_str("[sequences.extra]\nfilters=[\"safe\"]\n").unwrap();
        let seq = cfg.resolve_sequence("safe-lower").unwrap();
        assert_eq!(seq.filter_names(), vec!["safe", "wipeup", "lower"]);
    }

    #[test]
    fn unknown_filter_in_config_errors() {
        let toml = r#"
            [sequences.bad]
            filters = ["safe", "does-not-exist"]
        "#;
        let cfg = Config::from_toml_str(toml).unwrap();
        let err = cfg.resolve_sequence("bad").unwrap_err();
        assert!(err.to_string().contains("unknown filter: does-not-exist"));
    }

    #[test]
    fn unknown_sequence_errors() {
        let cfg = Config::default();
        let err = cfg.resolve_sequence("nope").unwrap_err();
        assert!(err.to_string().contains("unknown sequence: nope"));
    }

    #[test]
    fn malformed_toml_errors_clearly() {
        let err = Config::from_toml_str("this is = = not toml").unwrap_err();
        assert!(matches!(err, ConfigError::Parse(_)));
        assert!(err.to_string().contains("failed to parse config"));
    }

    #[test]
    fn list_sequences_merges_builtins_and_config() {
        let toml = r#"
            [sequences.myseq]
            filters = ["safe", "lower"]
            [sequences.safe]
            filters = ["safe", "wipeup"]
        "#;
        let cfg = Config::from_toml_str(toml).unwrap();
        let list = cfg.list_sequences();
        let names: Vec<&str> = list.iter().map(|(n, _)| n.as_str()).collect();
        // Built-in names plus the config-only "myseq".
        assert!(names.contains(&"default"));
        assert!(names.contains(&"myseq"));
        // "safe" is overridden by config.
        let safe = list.iter().find(|(n, _)| n == "safe").unwrap();
        assert_eq!(safe.1, vec!["safe", "wipeup"]);
    }

    #[test]
    fn discover_explicit_path_loads_file() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            "default = \"myseq\"\n[sequences.myseq]\nfilters = [\"safe\"]\n"
        )
        .unwrap();
        let cfg = Config::discover(Some(f.path())).unwrap();
        assert_eq!(cfg.default_sequence_name(), "myseq");
    }

    #[test]
    fn discover_missing_explicit_path_errors() {
        let path = Path::new("/nonexistent/rehab/config.toml");
        let err = Config::discover(Some(path)).unwrap_err();
        assert!(matches!(err, ConfigError::Io(_)));
    }

    #[test]
    fn default_config_path_ends_with_expected_suffix() {
        if let Some(p) = default_config_path() {
            assert!(p.ends_with("rehab/config.toml"));
        }
    }
}
