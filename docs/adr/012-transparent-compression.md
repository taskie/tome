# ADR-012: Transparent Compression with zstd

**Status:** Proposed  
**Date:** 2026-04-05  

## Context

tome stores file blobs in content-addressable stores (local, SSH, S3) and optionally
encrypts them with aether. Compression is not currently applied at any layer.
Adding zstd compression could significantly reduce storage and transfer costs,
especially for text-heavy repositories. However, re-compressing already-compressed
data (JPEG, MP4, ZIP, `.gz`, `.zst`) wastes CPU and can even increase size.

This ADR evaluates where to add compression and how to handle incompressible data.

## Where to compress: three candidate layers

### Option A: In aether (encrypt layer)

```
plaintext → compress → encrypt → store
store → decrypt → decompress → plaintext
```

**Pros:**
- Compression is applied before encryption, which is the only correct order.
  Encrypted data is indistinguishable from random and cannot be compressed.
- The aether header already has 4 reserved bits (`[11:8]`) available for a
  compression flag — no format change needed beyond claiming one bit.
- Per-chunk compression is natural: aether already processes data in chunks
  (128 KiB default). Each chunk can be independently compressed.
- Single implementation serves both `aether-cli` and `tome-store`'s
  `EncryptedStorage`.

**Cons:**
- Couples compression to encryption. Unencrypted `tome store push` would not
  benefit.
- Compression ratio metadata (needed for skip-if-incompressible) adds complexity
  to the chunk format.

### Option B: In tome-store (storage layer)

```
plaintext → compress → upload
download → decompress → plaintext
```

Or with encryption:

```
plaintext → compress → encrypt → upload
download → decrypt → decompress → plaintext
```

**Pros:**
- Works for both encrypted and unencrypted stores.
- Store-level concern — transparent to aether.
- Could use a separate file extension (`.zst`) or store metadata to indicate
  compression.

**Cons:**
- If combined with encryption, the ordering (compress then encrypt) must be
  enforced. Currently `EncryptedStorage` wraps `Storage` and calls
  `aether::Cipher::encrypt` on the entire file. Adding a compress step
  before encryption means either:
  - A new `CompressedEncryptedStorage` wrapper (layering complexity), or
  - The compress step leaks into `EncryptedStorage::upload`.
- Two separate codepaths for "compressed+encrypted" vs "compressed only".

### Option C: In aether, per-chunk with adaptive skip

```
for each chunk:
  compressed = zstd_compress(plaintext_chunk)
  if len(compressed) < len(plaintext_chunk) * threshold:
    write compressed chunk (flag = compressed)
  else:
    write plaintext chunk (flag = uncompressed)
  encrypt chunk
```

**Pros:**
- All the benefits of Option A.
- Automatically skips compression for incompressible chunks (JPEG, encrypted
  data, random bytes) — no need for file-type detection or user hints.
- Handles mixed files: a tar archive containing both text and binary will
  have its text chunks compressed and binary chunks left as-is.
- Per-chunk decisions are metadata-free: a single bit per chunk in the
  ciphertext stream is sufficient.

**Cons:**
- Slightly more complex chunk format.
- Decompression must be attempted for every chunk marked as compressed
  (but zstd decompression is fast — typically 1+ GiB/s).
- Encrypted chunks become variable-size, which complicates parallel
  decryption (already variable due to the last chunk, but now any chunk
  can be smaller).

## Decision

**Option C: per-chunk adaptive compression in aether.**

Rationale:

1. **Compress-then-encrypt is the only correct order.** Putting compression
   in the encryption layer enforces this invariant by construction.

2. **Per-chunk adaptivity eliminates false compression.** There is no need
   for file-type heuristics, extension lists, or user configuration. If a
   chunk doesn't compress well, it is stored verbatim.

3. **Single implementation.** Both `aether-cli` and `tome-store` benefit
   automatically.

4. **Header bits are available.** The reserved bits `[11:8]` can encode a
   compression method without a format version bump.

### Compression flag encoding

Use one of the reserved header flag bits to indicate that compression is enabled
for this stream:

```
bits [11:8]   compression    0 = none, 1 = zstd (per-chunk adaptive)
```

When this bit is set, each plaintext chunk is processed as:

```
compressed = zstd_compress(plaintext, level=3)
if compressed.len() < plaintext.len():
  chunk_data = 0x01 || compressed       (1 byte prefix + compressed)
else:
  chunk_data = 0x00 || plaintext        (1 byte prefix + original)
encrypt(chunk_data) → ciphertext chunk
```

The 1-byte prefix (`0x00` = raw, `0x01` = zstd) is inside the encrypted
envelope, so it is authenticated by AEAD and invisible to an observer.

On decryption:

```
decrypt(ciphertext chunk) → chunk_data
if chunk_data[0] == 0x01:
  plaintext = zstd_decompress(chunk_data[1..])
else:
  plaintext = chunk_data[1..]
```

### Threshold

The compression threshold is implicit: if the compressed output is not shorter
than the input, the raw path is taken. No configurable ratio parameter is needed.

A stricter threshold (e.g., compressed < 95% of original) could avoid marginal
compression that wastes CPU on decompression. This can be added later as a
tuning parameter if benchmarks show it matters.

### zstd compression level

Default: level 3 (zstd's default). This gives a good balance of speed and ratio.
A `--compression-level` flag can be added later for users who want to trade CPU
for ratio (level 1 = fast, level 19 = maximum compression).

### Backward compatibility

- Files without the compression bit are read exactly as today (no decompression).
- Files with the compression bit require the reader to understand per-chunk
  decompression. Older readers that reject non-zero reserved bits will refuse
  to read compressed files with a clear error ("reserved bits not zero").
- This is acceptable: compression is opt-in, and old files remain readable.

### Impact on parallel encryption/decryption

Compressed chunks are variable-size, which slightly complicates the parallel
pipeline. However, the existing parallel implementation already handles the
variable-size last chunk. The approach:

- **Encrypt:** compression happens in the worker before encryption. Each worker
  compresses its chunk independently (zstd is stateless per-chunk at this
  granularity).
- **Decrypt:** after decryption, the worker checks the prefix byte and
  decompresses if needed. Decompression is fast (>1 GiB/s) and does not
  dominate the pipeline.

### What about per-file compression?

An alternative is to compress the entire plaintext stream before chunking:

```
plaintext → zstd_stream_compress → chunk → encrypt
```

This achieves better compression ratios (cross-chunk context) but has drawbacks:

- Cannot skip incompressible regions — the entire file is either compressed
  or not.
- Streaming decompression must buffer across chunk boundaries.
- No parallelism benefit: the compressor is a single sequential stream.

Per-chunk compression sacrifices some ratio for simplicity and parallelism.
For tome's use case (file backups with diverse content), per-chunk adaptive
is the better trade-off.

## Implementation plan

1. **Add `zstd` dependency to aether** (`zstd = "0.13"` or the `zstd-safe` bindings).
2. **Claim reserved bit 8** for compression: `bits [11:8]` value `1` = zstd.
   Update `HeaderFlags` to include a `compression: Compression` field.
   Relax the "reserved must be zero" check to accept `0` or `1`.
3. **Modify `stream_encrypt`** to optionally compress each chunk before
   encryption, prefixing the 1-byte compression marker.
4. **Modify `stream_decrypt`** to check the prefix byte after decryption
   and decompress if needed.
5. **Add `--compress` / `--no-compress` flags** to `aether-cli` and
   `tome store push`.
6. **Benchmark** on representative workloads (text, binary, mixed) to
   validate the adaptive threshold and choose the default compression level.

## Consequences

- Storage cost drops significantly for text-heavy repos (typically 3–5x with zstd).
- Binary-heavy repos (photos, videos) see no penalty — incompressible chunks
  are stored verbatim with only a 1-byte overhead per chunk.
- The aether format gains a well-defined extension point for future compression
  algorithms (bits `[11:8]` values 2–15 are available).
- Older readers cleanly reject compressed files ("reserved bits not zero")
  rather than silently producing corrupt output.
