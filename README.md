# rehab

A modern reimagining of [`detox`](https://github.com/dharple/detox): rename
files and directories to replace problematic characters (spaces, shell
metacharacters, control characters, CGI escapes) with safe, easy-to-type
alternatives.

Filters are composable, sequences are configurable, traversal is
parallel, and every run is journaled so it can be undone.

## Usage

```bash
rehab [OPTIONS] <PATH>...
```

Clean one or more files/directories:

```bash
rehab "bad name.txt"                 # -> bad_name.txt
rehab -r ./messy_dir                 # recurse into a directory
rehab -n -r ./messy_dir              # dry-run: show the plan, change nothing
rehab -s safe-lower ./Photos         # apply a specific sequence
rehab -L                             # list available sequences
```

Undo:

```bash
rehab journals list
rehab undo [--journal <FILE>] [-n] [-v]
```

### Options

| Flag | Description |
| ------ | ------------- |
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

## Filters and Sequences

A *filter* transforms a single file or directory name, such as by replacing
problematic characters or changing its case. Filters can be combined in a
sequence to apply several transformations in order.

Filters:

| Filter | Description |
| ------ | ----------- |
| `safe` | Replace spaces, tabs, line breaks, and shell-problematic characters (such as parentheses, quotes, and dollar signs) with a separator (default `_`). |
| `wipeup` | Collapse repeated separators, remove a separator immediately before the file extension (so `file (1).csv` becomes `file_1.csv`), and trim leading/trailing separators and dots, keeping the collapsed name if trimming would leave it empty. |
| `lower` | Lowercase the name using Unicode-aware lowercasing. |
| `unicode-clean` | Strip Unicode control and format characters, such as zero-width spaces and bidirectional overrides. |
| `cgi-unescape` | Decode `%XX` escapes, including multibyte UTF-8 sequences. Leave invalid escapes as written; keep the original name if decoding produces invalid UTF-8. |

`wipeup` uses only the last dot to define the extension and removes the separator
only when the base before it and the extension are both non-empty. This string
transform applies to every path component, including directory names.

A *sequence* is an ordered chain of filters applied to each filename component.

Built-in sequences:

| Sequence | Filters |
| ---------- | --------- |
| `default` | `safe` → `wipeup` → `unicode-clean` |
| `safe` | `safe` |
| `safe-lower` | `safe` → `wipeup` → `lower` |
| `iso8859_1` | `cgi-unescape` → `safe` → `wipeup` → `unicode-clean` |
| `utf8` | `cgi-unescape` → `safe` → `wipeup` → `unicode-clean` |

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

A complete, commented example lives at
[`docs/config.example.toml`](docs/config.example.toml). Copy it to the default
location for your platform, or point at it directly for a single run:

```bash
# Linux & macOS
cp docs/config.example.toml ~/.config/rehab/config.toml

# Or use it for one run without installing it:
rehab -f docs/config.example.toml -L
```

### Creating a config

`rehab init` writes a commented config representing the built-in defaults to the
standard location, so you have an easy starting point to edit:

```bash
rehab init                 # write ~/.config/rehab/config.toml (refuses if it exists)
rehab init --force         # overwrite an existing config
rehab init -f ./my.toml    # write to a specific path
rehab init -n              # dry-run: print the path and content, write nothing
```

The generated file mirrors rehab's defaults, so with it in place rehab behaves
exactly as it does with no config. It refuses to overwrite an existing file
unless `--force` is given, and the written file is validated before `init`
reports success.

## Undo

Every non-dry-run writes a timestamped JSONL journal to the state directory
(`~/.local/state/rehab/` on both Linux and macOS; `XDG_STATE_HOME` overrides the
base directory). To reverse the most recent run:

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

## Man pages

rehab ships two man pages:

- **`rehab(1)`** — command usage, options, and subcommands. Generated from the
  CLI definition, so it always matches the actual flags.
- **`rehab-config(5)`** — the TOML configuration file format (hand-written).

Both cross-reference each other and the standard encoding references
`ascii(7)`, `iso_8859-1(7)`, `unicode(7)`, and `utf-8(7)`.

Generate and install them with:

```bash
make man                 # regenerate man/rehab.1 (rehab-config.5 is hand-written)
make install-man         # install into $(PREFIX)/share/man/man{1,5}
make uninstall-man       # remove them
```

The generator lives behind an opt-in `gen-man` Cargo feature so normal builds
and `cargo install` don't compile the documentation tooling. `make man` enables
it for you; to run it directly:

```bash
cargo run --features gen-man --bin gen-man -- man
```

`make install` runs `install-man` automatically (and `make uninstall` runs
`uninstall-man`). After installing, view them with:

```bash
man rehab
man rehab-config
```

If the pages aren't found, ensure the man directory (default
`~/.local/share/man`) is on your `MANPATH`. The generated `man/rehab.1` is not
checked into git; run `make man` to (re)create it. You can also preview a page
without installing:

```bash
make man && man ./man/rehab.1
man ./man/rehab-config.5
```

> The four `(7)` reference pages are standard Linux man-pages entries; rehab
> only cross-references them and does not ship copies. They may not be present
> by default on macOS.
