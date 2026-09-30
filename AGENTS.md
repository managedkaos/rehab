# Agent notes

Conventions for AI agents (and humans) working in this repo. Keep this file
short and practical.

## Pull requests

- Compose PR bodies in a repo-local scratch file and pass it with
  `--body-file`, rather than an inline multiline `--body "$(cat <<'EOF' ...)"`
  heredoc. Heredocs do not reliably auto-approve under the shell allowlist, so
  they trigger a manual prompt.
- Use `./.pr-body.md` for the scratch file. It is gitignored (see
  `.gitignore`), so it won't be committed, and it lives inside the repo so it
  matches the agent's repo-scoped write permissions (no write prompt).

  ```bash
  # Write the body to ./.pr-body.md, then:
  gh pr edit <N> --title "..." --body-file .pr-body.md
  gh pr create --title "..." --body-file .pr-body.md
  ```

- Drop the `WIP:` prefix from the PR title once the branch has real, committed
  changes and is ready for review.

## Commits and pushes

- Only commit when explicitly asked.
- Never push to `main`/`master` directly; push to the feature branch.
- Prefer new commits over `--amend`.

## Verifying changes

- Run `make all` before presenting a change: it runs fmt-check, clippy
  (`-D warnings`), build, and the full test suite.
- Add/adjust tests alongside behavior changes and keep docs (README,
  `docs/`, and the man pages under `man/`) in sync with code.

## Scratch files

- Repo-local scratch files (e.g. `./.pr-body.md`) avoid write prompts because
  the agent's writes are scoped to the repo (`./**`). The system temp dirs
  (`/tmp`, `/private/tmp`) are also permitted for cases that need them.
