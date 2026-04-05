# ADR-011: Configuration Hierarchy and `--repo` Default

**Status:** Accepted  
**Date:** 2026-04-05  

## Context

tome uses a two-tier configuration file system: a global `~/.config/tome/tome.toml`
and a project-local `./tome.toml`. Several CLI flags (`--db`, `--machine-id`) already
fall back to config file values, but `--repo` does not — it is hardcoded to `"default"`
in all 21 commands that accept it.

Meanwhile, the config file already defines `[scan] repo`, but the field is dead:
no command reads it as a fallback.

This ADR compares tome's model to git's, identifies gaps, and proposes a minimal
improvement.

## Git's configuration hierarchy

Git resolves configuration from seven levels (highest priority first):

| Level | Location | Scope |
|-------|----------|-------|
| 1 | `-c key=value` | Single command invocation |
| 2 | `GIT_CONFIG_*` env vars | Shell session |
| 3 | `.git/config.worktree` | Per-worktree (multi-worktree setups) |
| 4 | `.git/config` | Per-repository |
| 5 | `~/.gitconfig` | Per-user (global) |
| 6 | `/etc/gitconfig` | System-wide |
| 7 | Compiled-in defaults | Built-in |

Key properties of git's model:

- **Config discovery is directory-based.** Running `git log` anywhere inside a repo
  finds `.git/config` by walking up from the current directory.
- **All settings share the same resolution path.** There is no "this setting reads
  from config but that setting doesn't" inconsistency.
- **Repository identity is implicit.** git does not need `--repo` because the working
  directory *is* the repository. `git -C /path` changes the working directory, which
  changes the repo.

## tome's current model

```
Priority (highest wins):
  CLI arg > env var > ./tome.toml > ~/.config/tome/tome.toml > built-in default
```

| Setting | CLI → config fallback? | Note |
|---------|----------------------|------|
| `--db` | Yes | `cli.db.or(cfg.db).unwrap_or("tome.db")` |
| `--machine-id` | Yes | `cli.machine_id.or(cfg.machine_id).unwrap_or(0)` |
| `--repo` | **No** | Hardcoded `default_value = "default"` in all 21 commands |
| `serve --addr` | Yes | Falls back to `cfg.serve.addr` |
| `watch --quiet-period` | Yes | Falls back to `cfg.watch.quiet_period` |

### Why tome has `--repo` but git doesn't

tome is a *multi-repository-in-one-database* tool. A single `tome.db` can track
multiple named repositories (e.g., `default`, `docs`, `photos`). This is fundamentally
different from git, where one `.git` = one repository.

The `--repo` flag selects which repository inside the database to operate on. The
analogy in git would be something like `git --namespace`, except it's a first-class
concept in tome.

### Why directory-based config discovery is not needed (yet)

git finds `.git/config` by walking up the directory tree. tome could similarly search
for `tome.toml` in parent directories. However:

- tome already searches exactly one project-local path: `./tome.toml`.
- tome's database (`tome.db`) is the primary per-project anchor, not a config file.
- Walking up directories would add complexity for marginal benefit — users typically
  run `tome` from the project root where `tome.db` lives.

If tome later supports *nested* or *subdirectory-scoped* repositories (e.g., `photos/`
and `docs/` are separate repos under one root), parent-directory walking could become
useful. For now, the flat `./tome.toml` is sufficient.

### Where tome's config model falls short

The real gap is not hierarchy depth but **consistency**: `--db` and `--machine-id`
respect the config file, but `--repo` ignores it. This means:

```toml
# tome.toml
[scan]
repo = "photos"
```

…has no effect on `tome log`, `tome diff`, `tome status`, `tome files`, etc.
Users must pass `--repo photos` to *every* command, which defeats the purpose
of having a config file.

## Decision

### 1. Promote `[scan] repo` to a top-level `repo` key

Currently `repo` is nested under `[scan]`, implying it only applies to scanning.
In practice, it should apply to all commands. Add a top-level `repo` field:

```toml
repo = "photos"      # top-level: applies to all commands

[scan]
repo = "photos"      # still supported (backward compatible)
```

Resolution: `CLI --repo > top-level repo > [scan] repo > "default"`.

### 2. Apply the config fallback in main.rs, pass it to commands

Instead of modifying all 21 command Args structs, resolve the effective repo
name in `main.rs` and inject it into each command's args before dispatch.
This keeps the config fallback in one place.

However, clap's `default_value` makes this tricky: we cannot distinguish
"user passed `--repo default`" from "user omitted `--repo` and clap filled in
the default". The pragmatic solution: **remove `default_value` from all `--repo`
args** and make them `Option<String>`. Then in `main.rs` (or a shared helper),
resolve `None` → config → `"default"`.

### 3. Do not add parent-directory walking

The current two-tier model (global + `./tome.toml`) is sufficient. Adding
parent-directory search would add complexity without clear benefit for the
current use cases.

### 4. Document `TOME_REPO` environment variable

For scripting convenience, add `TOME_REPO` env var support at the `--repo` level.
Resolution becomes: `CLI --repo > TOME_REPO > config repo > "default"`.

## Consequences

- Users can set `repo = "photos"` once in `tome.toml` and all commands will use it.
- The config file becomes more useful for non-default repository workflows.
- The `Option<String>` change to `--repo` is internal; the CLI surface is unchanged
  (users who omit `--repo` still get `"default"` as before).
- `TOME_REPO` enables per-session overrides without modifying the config file.
