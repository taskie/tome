# tome

> **tome** /ˈtoʊm/ — *a large, heavy book, especially one dealing with a serious topic.*
>
> A chronicle of every file — recorded in hashes, preserved in snapshots.

A file change tracking system written in Rust. Scans directories, detects changes via content hashing (SHA-256 or BLAKE3) and xxHash64, and records snapshot history to SQLite or PostgreSQL.

## Getting Started

**Prerequisites:** Rust 1.85+ (Node.js 20.9+ for the web UI)

```bash
# Install
cargo install --path tome-cli

# Scan the current directory — creates tome.db
tome scan

# Scan a specific directory with a named repository
tome scan --repo myproject /path/to/project

# Browse the results
tome serve
# → http://127.0.0.1:8080
```

## Configuration

Settings can be specified via CLI arguments, environment variables, or `tome.toml` config files. Priority (highest wins): CLI > env > `./tome.toml` > `~/.config/tome/tome.toml` > defaults.

```toml
# ~/.config/tome/tome.toml or ./tome.toml
db = "tome.db"
machine_id = 0

[scan]
repo = "default"

[store]
default_store = "backup"
key_file = "~/.config/tome/keys/main.key"

[serve]
addr = "127.0.0.1:8080"
```

## CLI Reference

```
tome [OPTIONS] <COMMAND>

Options:
  --db <PATH|URL>         SQLite path or PostgreSQL URL  [env: TOME_DB]  [default: tome.db]
  --machine-id <N>        Machine ID for ID generation (0–65535)  [env: TOME_MACHINE_ID]  [default: 0]
```

### `tome scan [OPTIONS] [PATH]`

Scan a directory and record a snapshot of file changes.

```bash
tome scan                                        # current directory
tome scan --repo docs /srv/docs                  # named repository
tome scan --no-ignore ~/data                     # ignore .gitignore rules
tome scan --message "after deploy"               # annotate the snapshot
tome scan --digest-algorithm blake3              # use BLAKE3 instead of SHA-256
tome --db /var/db/tome.db scan ~/data            # custom DB path
```

### `tome store <COMMAND>`

Manage storage backends for file contents.

```bash
tome store add <name> <url>              # register a store
tome store list                          # list stores
tome store push [--repo <name>] <store>  # upload blobs to store
tome store copy [--encrypt] [--key-file <path>] <src> <dst>  # copy between stores
tome store verify <store>                # verify blob integrity
```

Supported URL schemes: `file:///path`, `ssh://user@host/path`, `s3://bucket/prefix`

### `tome sync <COMMAND>`

Synchronize snapshot history between databases.

```bash
tome sync add [--repo <name>] <name> <peer-db-url>  # register a peer
tome sync list [--repo <name>]                       # list peers
tome sync pull [--repo <name>] <name>                # pull incremental diffs
```

### `tome serve [--addr <host:port>]`

Start the HTTP API server (default: `127.0.0.1:8080`).

### `tome diff <snapshot1> <snapshot2> [OPTIONS]`

Compare two snapshots and show file changes.

```bash
tome diff 123456 789012                # show full diff
tome diff 123456 789012 --name-only    # filenames only
tome diff 123456 789012 --stat         # summary with sizes
```

### `tome restore --snapshot <id> <dest>`

Restore files from a historical snapshot to a local directory.

```bash
tome restore --snapshot 123456 ./restored
tome restore --snapshot 123456 --store backup --prefix src/ ./restored
```

### `tome tag <COMMAND>`

Manage key-value tags on blobs.

```bash
tome tag set <digest> <key> [value]    # set a tag
tome tag delete <digest> <key>         # delete a tag
tome tag list <digest>                 # list tags on a blob
tome tag search <key> [value]          # find blobs by tag
```

### `tome verify [OPTIONS]`

Verify scanned files against the cached state (bit-rot detection).

```bash
tome verify                            # verify default repo
tome verify --repo myproject           # verify specific repo
```

### `tome gc [OPTIONS]`

Garbage collect unreferenced blobs and old snapshots.

```bash
tome gc --dry-run                      # report only
tome gc --keep 10                      # keep 10 most recent snapshots
tome gc --keep-days 30 --repo myproject
```

## Web UI

A Next.js 16 browser interface. Requires `tome serve` to be running.

```bash
cd tome-web
cp env.local.example .env.local   # set TOME_API_URL if needed
npm install
npm run dev
# → http://localhost:3000
```

## Examples

### Back up to an external drive

```bash
tome scan ~/documents
tome store add backup file:///mnt/hdd/backup
tome store push backup
```

### Sync between machines over NFS

```bash
# On machine B, pull snapshots from machine A's database
tome sync add machineA sqlite:////mnt/nfs/machineA/tome.db
tome sync pull machineA
```

### Encrypted remote backup

```bash
tome store add local file:///data/store
tome store add remote s3://my-bucket/tome
tome store push local
tome store copy --encrypt --key-file ~/.config/tome/keys/main.key local remote
```

## Architecture

```
tome-core/     Hash computation (SHA-256 / BLAKE3 / xxHash64), ID generation, shared models
tome-db/       SeaORM entities, migrations, query operations
tome-store/    Storage abstraction (local / SSH / S3 / encrypted)
tome-server/   HTTP API server (axum)
tome-cli/      Unified CLI (scan / store / sync / diff / restore / tag / verify / gc / serve)
tome-web/      Next.js 16 web frontend
aether/        AES-256-GCM authenticated encryption library (internal)
treblo/        File-tree walk and hex utilities (internal)
```

Legacy crates (`ichno`, `ichnome`, etc.) are archived under `obsolete/`.

For detailed design documentation — DB schema, hash strategy, HTTP API reference, and known design issues — see [ARCHITECTURE.md](ARCHITECTURE.md).
