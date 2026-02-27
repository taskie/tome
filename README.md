# tome

A file change tracking system written in Rust. Scans directories, detects file changes using SHA-256 and xxHash64, and records snapshot history to SQLite or PostgreSQL.

## Features

- **Fast change detection** — three-stage filter (mtime/size → xxHash64 → SHA-256) skips unchanged files
- **Snapshot history** — accumulates snapshots per scan to track what changed and when
- **Storage management** — archives file contents to local / SSH / S3 / encrypted stores
- **DB synchronization** — incrementally pulls snapshot diffs between SQLite and PostgreSQL instances
- **HTTP API** — axum-based REST API for integration with external tools
- **Web UI** — Next.js 15 browser interface

## Build

```bash
# Requires Rust 1.85 or later
cargo build -p tome-cli --release

# Binary is at target/release/tome
```

## Quick Start

```bash
# Scan the current directory (recorded to tome.db)
tome scan

# Specify a directory and repository name
tome scan --repo myproject /path/to/project

# Make changes and scan again
touch newfile.txt
tome scan

# Expose scan results via the API server
tome serve
# → http://127.0.0.1:8080
```

## Command Reference

### Global Options

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--db <path\|URL>` | `TOME_DB` | `tome.db` | SQLite path or PostgreSQL URL |
| `--machine-id <n>` | `TOME_MACHINE_ID` | `0` | Machine ID for Sonyflake ID generation (0–65535) |

SQLite paths are accepted directly and auto-converted to `sqlite://...?mode=rwc`.

---

### tome scan

Scans a directory and records file changes.

```bash
tome scan [OPTIONS] [PATH]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--repo <name>` | `default` | Repository name |
| `[PATH]` | `.` (current directory) | Directory to scan |

```bash
# Examples
tome scan                              # current directory, default repository
tome scan --repo docs /srv/docs        # named repository
tome --db /var/db/tome.db scan ~/data  # explicit DB path
```

---

### tome store

Manages storage backends for file contents.

```bash
tome store <SUBCOMMAND>
```

#### tome store add

```bash
tome store add <NAME> <URL>
```

| Store type | URL format |
|------------|-----------|
| Local      | `file:///path/to/dir` |
| SSH        | `ssh://user@host/path/to/dir` |
| S3-compatible | `s3://bucket/prefix` |

#### tome store list

Lists registered stores.

```bash
tome store list
```

#### tome store push

Uploads blobs from a repository to a store.

```bash
tome store push [--repo <name>] <STORE> [PATH]
```

```bash
# Examples
tome store add backup file:///mnt/backup/tome
tome store push backup                        # push default repository
tome store push --repo docs backup            # push named repository
```

#### tome store copy

Copies blobs between stores (only blobs not already present in the destination).

```bash
tome store copy [--encrypt] [--key-file <path>] <SRC> <DST>
```

```bash
# Examples
tome store copy local s3-backup
tome store copy --encrypt --key-file ~/.config/tome/keys/mykey.key local encrypted-store
```

Encryption uses AES-256-GCM + Argon2id. The key file is a 32-byte binary file.

#### tome store verify

Verifies that blobs in a store are not corrupted.

```bash
tome store verify <STORE>
```

---

### tome sync

Synchronizes snapshot history between different databases.

```bash
tome sync <SUBCOMMAND>
```

#### tome sync add

Registers a sync peer.

```bash
tome sync add [--repo <name>] [--peer-repo <name>] <NAME> <PEER_DB_URL>
```

```bash
# Example: connect local SQLite to a remote PostgreSQL
tome sync add prod postgres://user:pass@db.example.com/tome
```

#### tome sync list

Lists registered peers.

```bash
tome sync list [--repo <name>]
```

#### tome sync pull

Fetches incremental snapshots from a peer and applies them.

```bash
tome sync pull [--repo <name>] <NAME>
```

```bash
tome sync pull prod         # fetch only diffs since last sync
```

---

### tome serve

Starts the HTTP API server.

```bash
tome serve [--addr <host:port>]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--addr` | `127.0.0.1:8080` | Listen address |

```bash
# Examples
tome serve                            # localhost only
tome serve --addr 0.0.0.0:8080        # all interfaces
```

#### API Endpoints

```
GET /health
GET /repositories
GET /repositories/:name
GET /repositories/:name/snapshots
GET /repositories/:name/latest
GET /snapshots/:id/entries
GET /blobs/:digest
```

---

## Web UI (tome-web)

A browser interface built with Next.js 15. Requires `tome serve` to be running.

**Requirements:** Node.js >= 18.18

```bash
cd tome-web

# First-time setup
cp env.local.example .env.local
# Edit TOME_API_URL to match the address of tome serve

npm install
npm run dev
# → http://localhost:3000
```

| Page | URL |
|------|-----|
| Repository list | `/` |
| Snapshot list | `/repositories/<name>` |
| Entry list | `/snapshots/<id>` |

Production build:

```bash
npm run build
npm start
```

---

## Typical Workflows

### Local Backup

```bash
# First run: scan and push to store
tome scan ~/documents
tome store add local-backup file:///mnt/hdd/tome-backup
tome store push local-backup

# Subsequent runs: scan and push only changes
tome scan ~/documents
tome store push local-backup
```

### Browse with API Server and Web UI

```bash
# Start in separate terminals
tome --db tome.db serve --addr 0.0.0.0:8080
# (in the tome-web directory) npm run dev
```

### Sync Between Two Machines

```bash
# Machine A (sender)
tome scan /data
# Make the DB accessible (e.g., via NFS or SQLite over network)

# Machine B (receiver)
tome sync add machineA sqlite:////mnt/nfs/machineA/tome.db
tome sync pull machineA
```

---

## Crate Structure

| Crate | Role |
|-------|------|
| `tome-core` | Hash computation, ID generation, shared models |
| `tome-db` | SeaORM entities, migrations, DB operations |
| `tome-store` | Storage abstraction (Local / SSH / S3 / Encrypted) |
| `tome-server` | axum HTTP API server |
| `tome-cli` | Unified CLI binary |
| `tome-web` | Next.js 15 frontend |
