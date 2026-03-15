# ADR-002: Content-Addressable Storage

**Status:** Accepted  
**Date:** 2023-09-01  

## Context

tome stores file blobs for backup and restore. Duplicate files across repositories and snapshots should not consume additional storage.

## Decision

Use content-addressable storage: blobs are identified by their cryptographic digest (SHA-256 or BLAKE3). The storage path is derived from the hex digest:

```
objects/<hex[0:2]>/<hex[2:4]>/<full-hex>
```

The `blobs` table records `(id, digest, fast_digest, size)` where `digest` is unique. The `replicas` table tracks which stores hold which blob.

## Consequences

- Identical files are stored only once (deduplication is automatic).
- Deletion requires reference counting — GC must check that no entries reference a blob before removing it.
- The digest algorithm is fixed per repository after the first scan.
