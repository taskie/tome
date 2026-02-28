# tome

A file change tracking system written in Rust. Scans directories, detects changes via SHA-256 / xxHash64, and records snapshot history to SQLite or PostgreSQL.

## Requirements

- Rust 1.85+
- Node.js 20.9+ (for tome-web only)

## Install

```bash
cargo install --path tome-cli
```

Or build manually:

```bash
cargo build -p tome-cli --release
# Binary: target/release/tome
```

## Quick Start

```bash
# Scan the current directory (saved to tome.db)
tome scan

# Scan with a custom repository name
tome scan --repo myproject /path/to/project

# Start the API server
tome serve
# → http://127.0.0.1:8080
```

## Commands

### Global Options

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--db <path\|URL>` | `TOME_DB` | `tome.db` | SQLite path or PostgreSQL URL |
| `--machine-id <n>` | `TOME_MACHINE_ID` | `0` | Machine ID for ID generation (0–65535) |

### `tome scan [--repo <name>] [<path>]`

Scans a directory and records a snapshot of file changes.

```bash
tome scan                              # current directory
tome scan --repo docs /srv/docs        # named repository
tome --db /var/db/tome.db scan ~/data  # custom DB
```

### `tome store`

Manages storage backends for file contents.

```bash
tome store add <name> <url>              # register a store
tome store list                          # list stores
tome store push [--repo <name>] <store>  # upload blobs to store
tome store copy [--encrypt] [--key-file <path>] <src> <dst>  # copy between stores
tome store verify <store>                # verify blob integrity
```

Store URL formats:

| Type | URL |
|------|-----|
| Local | `file:///path/to/dir` |
| SSH | `ssh://user@host/path` |
| S3-compatible | `s3://bucket/prefix` |

### `tome sync`

Synchronizes snapshot history between databases.

```bash
tome sync add [--repo <name>] <name> <peer-db-url>  # register a peer
tome sync list [--repo <name>]                       # list peers
tome sync pull [--repo <name>] <name>                # pull incremental diffs
```

### `tome serve [--addr <host:port>]`

Starts the HTTP API server (default: `127.0.0.1:8080`).

## Web UI (tome-web)

A Next.js 16 browser interface. Requires `tome serve` to be running.

```bash
cd tome-web
cp env.local.example .env.local   # set TOME_API_URL if needed
npm install
npm run dev
# → http://localhost:3000
```

## Workflows

### Local backup

```bash
tome scan ~/documents
tome store add backup file:///mnt/hdd/backup
tome store push backup
```

### Sync between machines

```bash
# On machine B, pull from A's SQLite over NFS
tome sync add machineA sqlite:////mnt/nfs/machineA/tome.db
tome sync pull machineA
```
