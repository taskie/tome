# Central Sync

Multiple machines maintain individual SQLite databases and synchronize to a shared backend (metadata) and S3-compatible store (object content).

## Architecture

```mermaid
graph TB
    subgraph AWS["AWS Central"]
        DDB["DynamoDB<br/><small>metadata</small>"]
        S3["S3 Bucket<br/><small>encrypted objects</small>"]
    end

    subgraph A["Machine A"]
        A_DB["SQLite"]
        A_ST["local store"]
    end

    subgraph B["Machine B"]
        B_DB["SQLite"]
        B_ST["local store"]
    end

    A_DB -- "sync push/pull<br/>(metadata)" --> DDB
    B_DB -- "sync push/pull<br/>(metadata)" --> DDB

    A_ST -- "store push<br/>(objects)" --> S3
    B_ST -- "store push<br/>(objects)" --> S3
    S3 -- "store copy<br/>(objects)" --> A_ST
    S3 -- "store copy<br/>(objects)" --> B_ST

    style DDB fill:#e3f2fd,stroke:#1565c0
    style S3 fill:#fff3e0,stroke:#ef6c00
    style A_DB fill:#f5f5f5
    style B_DB fill:#f5f5f5
    style A_ST fill:#f5f5f5
    style B_ST fill:#f5f5f5
```

## Two-Layer Sync

| Layer | Commands | Content | Destination |
|-------|----------|---------|-------------|
| Metadata | `sync push` / `sync pull` | snapshots, entries, objects (rows), replicas | DynamoDB (or PostgreSQL) |
| Object content | `store push` / `store copy` | encrypted file objects | S3 |

## Sync Modes

`sync push/pull` detects the URL scheme of the peer:

| URL scheme | Mode |
|------------|------|
| `postgres://...` or `sqlite://...` | Direct DB connection (SeaORM) |
| `http://...` or `https://...` | HTTP API (`POST /sync/push`, `GET /sync/pull`) |

## Authentication

HTTP sync peers can require AWS IAM authentication. Set `"auth": "aws-iam"` in the peer config:

```bash
tome sync add my-aws https://xxxxx.lambda-url.us-west-2.on.aws --repo myrepo
tome sync config my-aws auth aws-iam
tome sync config my-aws region us-west-2
```

`tome sync config` follows a git-config-like interface:

```bash
tome sync config <peer> <key> <value>   # set a key
tome sync config <peer> <key>           # get a key
tome sync config <peer> --unset <key>   # remove a key
tome sync config <peer> --list          # list all keys
```

| Config key | Description | Default |
|------------|-------------|---------|
| `auth` | Authentication method (`"aws-iam"` or omit for none) | none |
| `region` | AWS region for SigV4 signing | SDK default (`AWS_REGION`) |
| `service` | AWS service name for SigV4 signing | `"lambda"` |

When `"auth": "aws-iam"` is set, requests are signed with AWS SigV4 using credentials from the default chain (`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`, instance profile, SSO, etc.).

`POST /sync/push` is idempotent: duplicate pushes identified by `(source_machine_id, source_snapshot_id)` return the existing server-side snapshot without re-inserting.

## machine_id Allocation

`POST /machines` allocates an unused `machine_id` (valid range: 0–32767; `machine_id = 0` is reserved for local-only use). `tome init --server <url>` calls this endpoint and persists the result to `~/.config/tome/tome.toml`.
