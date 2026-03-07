# treblo

Calculate Git-compatible tree/blob hashes outside of a working directory.

![treblo](images/example.gif)

## Installation

```bash
cargo install --path .
```

## Usage

```bash
# Hash current directory recursively (sha1, git ls-tree compatible format)
treblo

# Hash a specific path
treblo path/to/dir

# Choose hash algorithm
treblo -H sha256 .
treblo -H blake3 .
treblo -H xxhash64 .
treblo -H xxh3-64 .

# JSON Lines output
treblo --json .

# Limit depth
treblo --depth 1 .

# Show only the root hash
treblo --summarize .

# Files (blobs) only, no trees
treblo --blob-only .
```

## Output Format

```
<file_mode> <object_type> <digest>\t<path>
```

Example:

```
100644 blob 8ab686eafeb1f44702738c8b0f24f2567c36da6d	README.md
040000 tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904	src
```

## Options

| Option | Description |
|---|---|
| `-H, --hasher <ALGO>` | Hash algorithm: `sha1` (default) / `sha256` / `blake3` / `xxhash64` / `xxh3-64` |
| `-s, --summarize` | Show only the root hash |
| `-d, --depth <N>` | Limit display depth |
| `-S, --no-self` | Do not show the root directory itself |
| `-j, --json` | Output in JSON Lines format |
| `-b, --blob-only` | Show files (blobs) only, skip trees |
| `-E, --no-error` | Ignore errors and continue |
| `--no-ignore` | Disable `.trebloignore` |
| `--no-ignore-dot` | Disable `.ignore` files |
| `--no-ignore-vcs` | Disable `.gitignore` files |
| `--no-ignore-global` | Disable global gitignore |
| `--no-ignore-exclude` | Disable `.git/info/exclude` |

`.trebloignore` uses the same syntax as `.gitignore` and lets you define treblo-specific exclusion patterns.

## License

Apache License 2.0
