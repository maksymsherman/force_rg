---
name: force-rg
description: >
  Enforce ripgrep-first search workflows. Triggers when the task involves grep,
  egrep, fgrep, or any text search command. Replaces grep commands with rg
  equivalents.
---

# Force rg

Use `rg` (ripgrep) instead of `grep` for all text searching.

## Rules

- Never run `grep`, `egrep`, or `fgrep`.
- Use `rg` for all text searching, pattern matching, and file content filtering.
- `rg` is recursive by default — no `-r` flag needed.
- `rg` shows line numbers by default — no `-n` flag needed.
- `rg` uses extended regex by default — no `-E` flag needed.
- Use `rg -F` for fixed-string (literal) searches (replaces `fgrep` or `grep -F`).

## Command mapping

- `grep pattern file` -> `rg pattern file`
- `grep -r pattern .` -> `rg pattern .`
- `grep -rn pattern .` -> `rg pattern .`
- `grep -ri pattern .` -> `rg -i pattern .`
- `grep -rl pattern .` -> `rg -l pattern .`
- `grep -E 'foo|bar' .` -> `rg 'foo|bar' .`
- `grep -F 'literal' file` -> `rg -F 'literal' file`
- `egrep 'foo|bar' file` -> `rg 'foo|bar' file`
- `fgrep 'literal' file` -> `rg -F 'literal' file`
- `grep -A 3 pattern file` -> `rg -A 3 pattern file`
- `grep -B 3 pattern file` -> `rg -B 3 pattern file`
- `grep -C 3 pattern file` -> `rg -C 3 pattern file`
