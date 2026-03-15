# Force rg

Use `rg` (ripgrep) instead of `grep` for all text searching. ripgrep is faster, respects `.gitignore`, and uses modern regex by default.

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

## When to use ast-grep instead of rg

Use `rg` when text is enough — hunting strings, TODOs, log lines, config values, or non-code assets. Use `ast-grep` when structure matters — it parses code and matches AST nodes, so results ignore comments/strings and understand syntax.

- **Refactors/codemods**: rename APIs, change import forms, rewrite call sites or variable kinds -> `ast-grep`
- **Policy checks**: enforce patterns across a repo (scan with rules + test) -> `ast-grep`
- **Recon**: find strings, TODOs, log lines, config values -> `rg`
- **Pre-filter**: narrow candidate files before a precise pass -> `rg`

Combine for speed + precision: `rg` to shortlist files, then `ast-grep` to match/modify with precision.

```sh
# Structured code match (ignores comments/strings):
ast-grep run -l TypeScript -p 'import $X from "$P"'

# Codemod (only real var declarations become let):
ast-grep run -l JavaScript -p 'var $A = $B' -r 'let $A = $B' -U

# Quick textual hunt:
rg -n 'console\.log\(' -t js

# Combine speed + precision:
rg -l -t ts 'useQuery\(' | xargs ast-grep run -l TypeScript -p 'useQuery($A)' -r 'useSuspenseQuery($A)' -U
```

**Rule of thumb**: need correctness or you'll apply changes -> start with `ast-grep`. Need raw speed or you're just hunting text -> start with `rg`.
