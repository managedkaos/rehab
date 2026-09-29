# rehab

A modern reimagining of [`detox`](https://github.com/dharple/detox): rename
files and directories to replace problematic characters (spaces, shell
metacharacters, control characters, CGI escapes) with safe, easy-to-type
alternatives. Filters are composable, sequences are configurable, traversal is
parallel, and every run is journaled so it can be undone.

## Usage

```
rehab [OPTIONS] <PATH>...
rehab undo [--journal <FILE>] [-n] [-v]
```

Clean one or more files/directories:

```bash
rehab "bad name.txt"                 # -> bad_name.txt
rehab -r ./messy_dir                 # recurse into a directory
rehab -n -r ./messy_dir              # dry-run: show the plan, change nothing
rehab -s safe-lower ./Photos         # apply a specific sequence
rehab -L                             # list available sequences
```

### Options

| Flag | Description |
|------|-------------|
| `-n`, `--dry-run` | Show the planned renames without modifying the filesystem. Skipped files (unchanged names, or collisions under `--on-collision skip`) are not listed unless `-v` is also given. |
| `-r`, `--recursive` | Recurse into subdirectories (traversal roots that are directories are descended, not renamed; hidden `.` entries are skipped). |
| `-v`, `--verbose` | Report each rename as it happens, and also report files that are skipped. |
| `-s`, `--sequence <NAME>` | Use a named sequence instead of the default. |
| `-j`, `--jobs <N>` | Number of worker threads (default: available parallelism). |
| `--on-collision <POLICY>` | `suffix` (default), `skip`, or `overwrite` when the target name already exists. |
| `-f`, `--config <FILE>` | Use this config file instead of the default discovery. |
| `-L`, `--list-sequences` | List available sequences and exit. |
| `--keep-journals <N>` | Keep only the N most recent journals when pruning (`0` = unlimited). Overrides the config `keep`. |
| `--prune-journals` | Prune old journals after this run (opt-in). |

Naming a directory without `-r` processes its immediate, non-hidden contents
(one level, no descent) — so `rehab .` cleans the current directory like
`detox .`. Use `-r` to recurse into subdirectories.

## Sequences

A *sequence* is an ordered chain of filters applied to each filename component.
Built-ins:

| Sequence | Filters |
|----------|---------|
| `default` | `safe` → `wipeup` → `unicode-clean` |
| `safe` | `safe` |
| `safe-lower` | `safe` → `wipeup` → `lower` |
| `iso8859_1` | `cgi-unescape` → `safe` → `wipeup` → `unicode-clean` |
| `utf8` | `cgi-unescape` → `safe` → `wipeup` → `unicode-clean` |

Filters:

- **safe** — replace spaces and shell-problematic characters (`(){}[]&;|<>*?!'"$` …) with a separator (default `_`).
- **wipeup** — collapse repeated separators and trim leading/trailing separators.
- **lower** — lowercase the name.
- **unicode-clean** — strip Unicode control/format characters.
- **cgi-unescape** — decode `%XX` escapes.

## Configuration

`rehab` reads an optional TOML config from `~/.config/rehab/config.toml` (or the
path given with `-f`). Config sequences override built-ins of the same name, and
you can set a different default:

```toml
default = "myseq"

[sequences.myseq]
filters = ["cgi-unescape", "safe", "lower"]
separator = "-"
on_collision = "skip"
```

> Note: `filters` and `default` are fully applied. The per-sequence `separator`
> and `on_collision` keys are parsed but not yet wired into the pipeline — for
> now the separator is `_` and the collision policy is set with the
> `--on-collision` flag. Wiring these through is tracked as a follow-up.

A complete, commented example lives at
[`docs/config.example.toml`](docs/config.example.toml). Copy it to the default
location for your platform, or point at it directly for a single run:

```bash
# Linux & macOS
cp docs/config.example.toml ~/.config/rehab/config.toml

# Or use it for one run without installing it:
rehab -f docs/config.example.toml -L
```

## Undo

Every non-dry-run writes a timestamped JSONL journal to the platform state
directory (`~/.local/state/rehab/` on Linux, `~/Library/Application Support/rehab/`
on macOS). To reverse the most recent run:

```bash
rehab undo            # revert the most recent run
rehab undo -n         # preview the reversal
rehab undo --journal ~/.local/state/rehab/journal-1234567890.jsonl
```

Undo reverses renames in inverse order and skips entries whose source is missing
or whose target already exists, reporting each skip.

### Journal rotation

Journals accumulate one file per run. `rehab` can list and prune them, and can
optionally prune automatically after a run.

```bash
rehab journals list                  # list saved journals, newest first
rehab journals prune                 # keep the newest 20 (default), delete older
rehab journals prune --keep 5        # keep only the newest 5
rehab journals prune -n --keep 5     # preview what would be removed
rehab journals prune -v --keep 5     # report each removed journal
```

Auto-prune after a run is **opt-in**:

```bash
rehab -r --prune-journals ./dir             # prune after this run (keep = config or 20)
rehab -r --prune-journals --keep-journals 5 ./dir
```

Retention can also be set in config under `[journal]` (see below). Precedence for
the keep count is **flag (`--keep-journals` / `--keep`) > config (`keep`) >
default (20)**. A keep of `0` means unlimited (never prune). Automatic pruning
happens only when `--prune-journals` is passed or `auto_prune = true` in config.

## Development

```bash
make all      # fmt-check, clippy (-D warnings), build, test
make test     # run the test suite
make fmt      # format in place
```

The design and task breakdown live in
[`docs/IMPLEMENTATION_PLAN.md`](docs/IMPLEMENTATION_PLAN.md).
