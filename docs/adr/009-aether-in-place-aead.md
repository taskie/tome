# ADR-009: In-Place AEAD and Buffer Reuse in aether

**Status:** Accepted
**Date:** 2026-03

## Context

Profiling aether's streaming encryption/decryption revealed two primary sources of overhead beyond the core AEAD computation:

1. **Per-chunk heap allocation** — Every loop iteration allocated a fresh `Vec<u8>` for the read buffer (`vec![0u8; chunk_size]`). With chunk_kind=4 (128 KiB) and a 100 MiB file, this produced ~800 allocations per operation.

2. **Intermediate `Vec` from AEAD** — The `Aead::encrypt` / `decrypt` trait methods return a newly allocated `Vec<u8>` containing the result. This doubled the per-chunk allocation count and added a full memcpy of the ciphertext/plaintext.

Benchmarks showed chunk_kind=4 (128 KiB) was optimal, but performance degraded sharply at chunk_kind >= 5 (256 KiB+). The allocation + copy cost scales with chunk size, suggesting it was a significant contributor.

## Decision

### 1. Replace `Aead` with `AeadInPlace`

Use the `AeadInPlace` trait (from the RustCrypto `aead` crate) instead of the `Aead` trait. `AeadInPlace::encrypt_in_place` appends the 16-byte AEAD tag directly to the buffer; `decrypt_in_place` verifies and removes it. Both operate on a caller-provided `Vec<u8>`, eliminating the library-internal allocation.

The four `AeadInner` methods (`encrypt`, `decrypt`, `encrypt_ad`, `decrypt_ad`) were consolidated into two (`encrypt_in_place`, `decrypt_in_place`), both taking an `ad: &[u8]` parameter (empty slice when no associated data is needed).

### 2. Reuse buffers across chunks

Pre-allocate `read_buf` and `work` buffers before the loop and reuse them via `clear()` + `extend_from_slice()`. This reduces heap allocations from O(N_chunks) to O(1).

### 3. EOF detection via `fill_buf()`

`stream_encrypt` previously used a two-buffer lookahead: read the next full chunk to determine if the current chunk is the last. This required two `Vec<u8>` read buffers and a copy between them.

Replaced with `BufRead::fill_buf()` after each read: if the internal buffer is empty (EOF), the current chunk is the last. This eliminates one buffer and one memcpy per chunk.

### 4. Short-chunk optimization in `stream_decrypt`

If `pos < ct_size` (the read returned fewer bytes than a full ciphertext chunk), the chunk is necessarily the last. Skip the normal-nonce AEAD trial and decrypt directly with the last-chunk nonce. This avoids a wasted AEAD computation for every file whose size is not an exact multiple of the chunk size (the common case).

For the rare case where the last chunk is exactly full-size, the existing two-nonce trial-and-fallback remains, with the ciphertext restored from `read_buf` on retry.

## Consequences

- **Zero per-chunk allocations** in the streaming hot path (after initial buffer setup).
- **Simpler `AeadInner` API**: 2 methods instead of 4, with associated data always explicit.
- `stream_decrypt` retry on full-size last chunks requires a memcpy from `read_buf` to `work` (same cost as before, occurs at most once per file).
- `decrypt_v0` delayed-write simplified from `tmp_old`/`tmp_new` pair to a single `delayed` buffer with `swap`.
