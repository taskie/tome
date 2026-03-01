# tome

> **tome** /ˈtoʊm/ — *a large, heavy book, especially one dealing with a serious topic.*
>
> A chronicle of every file — recorded in hashes, preserved in snapshots.

A file change tracking system written in Rust. Scans directories, detects changes via content hashing (SHA-256 or BLAKE3 + xxHash64), and records snapshot history to SQLite or PostgreSQL.

## Use Cases

| Use case | How tome helps |
|----------|---------------|
| **Change tracking** | Record exactly which files changed, when, and by how much — browse history via CLI or Web UI |
| **Backup with deduplication** | Push file blobs to local / SSH / S3 storage; identical content is stored only once |
| **Bit-rot detection** | `tome verify` re-hashes every file and flags content that has silently changed |
| **Cross-directory comparison** | `tome diff` and the web `/diff` page compare file states between repositories side-by-side |

tome is a **local-first personal tool**. It does not require a central server. SQLite is a first-class citizen; the `tome serve` HTTP API and web UI are optional.

## Quick Start

**Prerequisites:** Rust 1.85+

```bash
# Install
cargo install --path tome-cli

# Scan the current directory — creates tome.db
tome scan

# See what changed between two snapshots
tome diff <snapshot_id_before> <snapshot_id_after>

# Browse snapshots and file history in the browser
tome serve
# → http://127.0.0.1:8080
```

## Typical Workflows

### Track changes over time

```bash
# First scan — records all files as "added"
tome scan --repo myproject /path/to/project

# Make some changes, then re-scan — detects modifications and deletions
tome scan --repo myproject /path/to/project

# Show what changed between two snapshots
tome diff <snap1_id> <snap2_id>
tome diff <snap1_id> <snap2_id> --stat        # with file sizes
tome diff <snap1_id> <snap2_id> --name-only   # file names only
```

### Back up to an external drive or NAS

```bash
# Scan files and record their state
tome scan ~/documents

# Register a local store and upload file content
tome store add backup file:///mnt/hdd/backup
tome store push backup

# Restore a file from a historical snapshot
tome restore --snapshot <snap_id> --store backup ./restored/
```

### Encrypted backup to S3

```bash
# Generate a 32-byte key
dd if=/dev/urandom bs=32 count=1 of=~/.config/tome/keys/main.key

# Register a local staging store and an S3 destination
tome store add local  file:///data/local-store
tome store add remote s3://my-bucket/tome

# Push locally, then copy with encryption to S3
tome store push local
tome store copy --encrypt --key-file ~/.config/tome/keys/main.key local remote
```

### Sync history to a central server

```bash
# Register this machine with a central tome server
tome init --server https://sync.example.com

# Register a sync peer (HTTP mode — no direct DB access required)
tome sync add central "https://sync.example.com" --repo default

# Push snapshot metadata to the central server
tome sync push central

# On another machine: pull snapshot history
tome sync pull central
```

Direct PostgreSQL access is also supported (for LAN / VPN environments):

```bash
tome sync add lan "postgres://user:pass@db.lan/tome" --repo default
tome sync push lan
```

### Detect bit-rot

```bash
# Record expected hashes
tome scan --repo archive /mnt/nas/archive

# Later: re-hash every file and compare against stored digests
tome verify --repo archive /mnt/nas/archive
# OK         docs/report.pdf
# MODIFIED   photos/2023/IMG_001.jpg  ← silent corruption detected
# MISSING    videos/movie.mkv
```

### Compare two directories

```bash
# Scan both directories as separate repositories
tome scan --repo left  /path/to/left
tome scan --repo right /path/to/right

# Web UI cross-repo diff
tome serve
# → http://127.0.0.1:8080/diff?repo1=left&repo2=right
```

## Configuration

Settings can be specified via CLI arguments, environment variables, or `tome.toml` config files.
Priority (highest wins): **CLI > env > `./tome.toml` > `~/.config/tome/tome.toml` > defaults**.

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
tome scan                                        # current directory, repo "default"
tome scan --repo docs /srv/docs                  # named repository
tome scan --no-ignore ~/data                     # ignore .gitignore rules
tome scan --message "after deploy"               # annotate the snapshot
tome scan --digest-algorithm blake3              # use BLAKE3 (set once at repo creation)
tome --db /var/db/tome.db scan ~/data            # custom DB path
```

### `tome diff <snap1> <snap2> [OPTIONS]`

Compare two snapshots and print changed files.

```bash
tome diff 123456 789012                # A/M/D status per file
tome diff 123456 789012 --name-only    # file names only
tome diff 123456 789012 --stat         # with file sizes and summary
tome diff 123456 789012 --prefix src/  # limit to files under src/
```

### `tome verify [OPTIONS] [PATH]`

Re-hash every file and compare against stored digests (bit-rot detection).

```bash
tome verify                            # default repo, path from last snapshot metadata
tome verify --repo myproject           # specific repo
tome verify --quiet /srv/data          # suppress OK lines, only show problems
```

### `tome restore [OPTIONS] <DEST>`

Restore files from a historical snapshot to a local directory.

```bash
tome restore --snapshot 123456 ./restored
tome restore --snapshot 123456 --store backup --prefix src/ ./restored
```

### `tome store <COMMAND>`

Manage storage backends for file blobs.

```bash
tome store add <name> <url>                                    # register a store
tome store set <name> --url <new-url>                          # update store URL
tome store rm <name> [--force]                                 # remove a store
tome store list                                                 # list stores
tome store push [--repo <name>] [<store>] [<path>]             # upload blobs
tome store copy [--encrypt] [--key-file <path>] [--cipher <alg>] <src> <dst>
tome store verify <store>                                       # verify integrity
```

Supported URL schemes: `file:///path`, `ssh://user@host/path`, `s3://bucket/prefix`

Cipher options for `--cipher`: `aes256gcm` (default), `chacha20-poly1305`

### `tome gc [OPTIONS]`

Remove unreferenced blobs and prune old snapshots.

```bash
tome gc --dry-run                           # report only, no changes
tome gc --keep 10                           # keep 10 most recent snapshots per repo
tome gc --keep-days 30 --repo myproject     # prune snapshots older than 30 days
```

### `tome tag <COMMAND>`

Key-value metadata on blobs.

```bash
tome tag set <digest> <key> [value]    # set a tag
tome tag delete <digest> <key>         # remove a tag
tome tag list <digest>                 # list tags for a blob
tome tag search <key> [value]          # find blobs by tag
```

### `tome sync <COMMAND>`

Synchronize snapshot history with a peer database or HTTP server.

Peers can be specified as a **PostgreSQL URL** (`postgres://...`) for direct DB access,
or an **HTTP URL** (`http://...` / `https://...`) to sync via the `tome serve` API.

```bash
tome sync add [--repo <name>] <name> <peer-url>   # register a sync peer
tome sync add --peer-repo docs central postgres://central/tome  # remote repo name differs
tome sync set <name> [--peer-url <url>] [--peer-repo <name>]   # update peer settings
tome sync rm <name> [--repo <name>]                             # remove a sync peer
tome sync list                                      # list peers
tome sync pull <name>                               # pull incremental snapshots from peer
tome sync push <name> [--machine-id <N>]            # push local snapshots to peer
```

### `tome init`

Register this machine with a central `tome serve` instance.

```bash
tome init --server https://sync.example.com
tome init --server https://sync.example.com --name my-laptop --description "dev machine"
tome init --server https://sync.example.com --force   # overwrite existing machine_id
```

### `tome serve [--addr <host:port>]`

Start the HTTP API and web UI server (default: `127.0.0.1:8080`).

## Web UI

A Next.js 16 browser interface. Requires `tome serve` to be running.

```bash
cd tome-web
cp env.local.example .env.local   # set TOME_API_URL if needed
npm install
npm run dev
# → http://localhost:3000
```

Pages: repository list · snapshot list · current files · snapshot diff · file history · blob detail · cross-repository diff · stores · tags · sync peers

## Security

tome binds to `127.0.0.1:8080` by default (local access only). For remote access:

| Scenario | Recommended approach |
|----------|---------------------|
| LAN access | Bind to LAN IP, restrict via firewall |
| Remote access | WireGuard / Tailscale VPN tunnel |
| Cloud deployment | Cloudflare Access or AWS ALB + OIDC in front of `tome serve` |

tome-server and tome-web do not implement application-level authentication.

## Architecture

```
tome-core/     Hash computation (SHA-256 / BLAKE3 / xxHash64), ID generation, shared models
tome-db/       SeaORM entities, migrations, query operations (ops/ modules)
tome-store/    Storage abstraction (local / SSH / S3 / encrypted)
tome-server/   HTTP API server (axum, routes/ modules)
tome-cli/      Unified CLI (scan / store / sync / diff / restore / tag / verify / gc / serve)
tome-web/      Next.js 16 web frontend
aether/        AEAD authenticated encryption (AES-256-GCM / ChaCha20-Poly1305 + Argon2id)
treblo/        Hash algorithms (xxHash64 / SHA-256 / BLAKE3) and file-tree walk utilities
```

Legacy crates (`ichno`, `ichnome`, etc.) are archived under `obsolete/`.

For detailed design documentation — DB schema, hash strategy, HTTP API reference — see [ARCHITECTURE.md](ARCHITECTURE.md).
