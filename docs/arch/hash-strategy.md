# Hash Strategy

Change detection uses a three-stage filter to minimize I/O:

```mermaid
flowchart LR
    A["mtime / size<br/>changed?"] -- Yes --> B["xxHash64<br/>changed?"]
    A -- No --> SKIP["Skip ⏭"]
    B -- Yes --> C["SHA-256<br/><small>(or BLAKE3)</small>"]
    B -- No --> SKIP
    C --> RECORD["Record new<br/>snapshot entry"]

    style SKIP fill:#e8f5e9,stroke:#4caf50
    style RECORD fill:#e3f2fd,stroke:#2196f3
```

Both hashes are computed in a single pass through the file in `treblo/src/hash.rs::hash_file()` (re-exported via `tome-core::hash`).

The digest algorithm is configured per repository via `repositories.config["digest_algorithm"]` (default: `"sha256"`). Use `tome scan --digest-algorithm blake3` when creating a new repository. The algorithm cannot be changed after the first scan (digest consistency).
