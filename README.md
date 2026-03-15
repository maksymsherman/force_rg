# force_rg

A policy tool that redirects coding agents away from `grep`/`egrep`/`fgrep` commands toward [`rg` (ripgrep)](https://github.com/BurntSushi/ripgrep), with conservative exact rewrites.

## What gets blocked

- `grep`, `egrep`, `fgrep` (all invocations)

## What gets suggested

- Exact rewrites when the flag mapping is clear:
  - `grep -rn pattern .` -> `rg pattern .` (recursive and line numbers are defaults in rg)
  - `grep -ri pattern .` -> `rg -i pattern .`
  - `grep -rl pattern .` -> `rg -l pattern .`
  - `grep -E 'foo|bar' .` -> `rg 'foo|bar' .` (extended regex is default in rg)
- Variant-aware rewrites:
  - `egrep 'foo|bar' file` -> `rg 'foo|bar' file`
  - `fgrep 'literal' file` -> `rg -F 'literal' file`
- Flag preservation for meaningful options:
  - `-i`, `-v`, `-w`, `-l`, `-c`, `-o`, `-A`, `-B`, `-C`, etc. are kept as-is

If a `grep` flag does not have a guaranteed direct `rg` translation, the tool blocks and tells you to translate that flag manually instead of guessing.

## Quick install

```sh
curl -fsSL https://raw.githubusercontent.com/maksymsherman/force_rg/main/install.sh | bash
```

This builds the binary, installs it to `~/.local/bin/`, auto-configures hooks for any detected agents (Claude Code, Gemini CLI), and installs the Codex skill at `~/.codex/skills/force-rg` even if Codex has not been launched yet. Requires Rust/Cargo and `rg`.

The installer compares the built binary against the installed one with SHA-256:

- missing binary -> installs it
- different hash -> updates it
- same hash -> skips the copy unless you force an overwrite

Useful variants:

```sh
curl -fsSL https://raw.githubusercontent.com/maksymsherman/force_rg/main/install.sh | bash -s -- --check-binary-hash
curl -fsSL https://raw.githubusercontent.com/maksymsherman/force_rg/main/install.sh | bash -s -- --check-binary-hash --overwrite-binary
curl -fsSL https://raw.githubusercontent.com/maksymsherman/force_rg/main/install.sh | bash -s -- --dry-run
```

## Inspect before running

If you want to see exactly what code and files are involved before installing, prefer downloading or cloning first instead of piping straight to `bash`.

Review the installer plan without executing anything:

```sh
curl -fsSL https://raw.githubusercontent.com/maksymsherman/force_rg/main/install.sh | bash -s -- --dry-run
```

Review the actual repo files locally:

```sh
git clone https://github.com/maksymsherman/force_rg.git
cd force_rg
sed -n '1,260p' install.sh
sed -n '1,260p' src/main.rs
```

## Manual install

### Claude Code

```sh
git clone https://github.com/maksymsherman/force_rg.git
cd force_rg && cargo build --release
cp target/release/enforce-rg-command ~/.local/bin/
```

Add to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [{
      "matcher": "Bash",
      "hooks": [{"type": "command", "command": "enforce-rg-command --claude-hook-json"}]
    }]
  }
}
```

### Gemini CLI

Same binary, different hook. Add to `~/.gemini/settings.json`:

```json
{
  "hooks": {
    "BeforeTool": [{
      "matcher": "run_shell_command",
      "hooks": [{"type": "command", "command": "enforce-rg-command --gemini-hook-json"}]
    }]
  }
}
```

### Codex

Install as a global skill (recommended; triggers automatically on grep tasks):

```sh
git clone https://github.com/maksymsherman/force_rg.git ~/.codex/skills/force-rg
```

Project-local fallback only:

```sh
curl -fsSL https://raw.githubusercontent.com/maksymsherman/force_rg/main/AGENTS.md -o AGENTS.md
```

## Verify

```sh
enforce-rg-command --command 'rg pattern .'           # exits 0
enforce-rg-command --command 'grep -rn pattern .'     # exits 2, prints exact rg rewrite
enforce-rg-command --command 'fgrep literal file.txt' # exits 2, prints rg -F rewrite
enforce-rg-command --command 'grep -s pattern file.txt' # exits 2, asks for manual flag translation
```

## License

MIT
