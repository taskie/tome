# aether Streaming AEAD Design

## Header Flags Layout (u16)

```
bits [15:12]  version      format version (0–15)
bits [11:8]   reserved     must be 0
bits [7:4]    chunk_kind   chunk size selector
bits [3:0]    algorithm    AEAD algorithm
```

Backward compatibility: existing files with flags `0x0000` (AES) / `0x0001` (ChaCha20) are correctly parsed as version=0, chunk_kind=0.

## Version Definitions

| version | Description |
|---------|-------------|
| 0 | Legacy format. chunk_kind is ignored (always 8 KiB). Integrity suffix appended to plaintext and verified after decryption |
| 1 | Envelope encryption + streaming AEAD. KEK/DEK separation, STREAM construction (last-chunk flag), variable chunk size, header AD authentication |

## algorithm Values

| Value | Algorithm | Nonce Size |
|-------|-----------|------------|
| 0 | AES-256-GCM | 12 bytes |
| 1 | ChaCha20-Poly1305 | 12 bytes |
| 2 | XChaCha20-Poly1305 | 24 bytes |
| 3–15 | Reserved | — |

## chunk_kind Values

Ciphertext chunk size = `8 KiB × 2^chunk_kind`. Plaintext = ciphertext − 16-byte tag.
v0 ignores chunk_kind (always 8 KiB). Effective in v1 and later.

| chunk_kind | Ciphertext Chunk Size | Notes |
|------------|----------------------|-------|
| 0 | 8 KiB | Default (v0 compatible) |
| 1 | 16 KiB | |
| 2 | 32 KiB | |
| 3 | 64 KiB | |
| 4 | 128 KiB | |
| 5 | 256 KiB | |
| 6 | 512 KiB | |
| 7 | 1 MiB | v1 default |
| 8 | 2 MiB | |
| 9 | 4 MiB | |
| 10 | 8 MiB | |
| 11 | 16 MiB | |
| 12 | 32 MiB | |
| 13 | 64 MiB | Large backups |
| 14 | 128 MiB | High memory usage |
| 15 | 256 MiB | High memory usage |

## v1 KEK/DEK Envelope Encryption

### Key Block Layout

The Key Block is placed between the 32-byte header and the encrypted data chunks.

```
Offset  Size  Field
─────────────────────────────────────────────
0       2     key_block_len   — total byte length of Key Block (including this field)
2       1     kdf_id          — 0 = none (raw key), 1 = argon2id
3       1     slot_count      — number of KEK slots (1–255)
4       var   dek_nonce       — nonce for DEK encryption (12 or 24 bytes, matches algorithm)
16      var   kdf_params      — KDF-specific parameters (depends on kdf_id)
var     52×N  slots           — array of KEK slots
```

**kdf_params (kdf_id=0)**: empty (0 bytes)

**kdf_params (kdf_id=1, argon2id)**: salt[16] + m_cost[4] + t_cost[4] + p_cost[4] = 28 bytes

**Slot (52 bytes)**: key_id[4] + encrypted_dek[48]

- `key_id` = first 4 bytes of SHA-256(KEK) for fast slot lookup
- `encrypted_dek` = AEAD(KEK, dek_nonce, DEK, ad=header[0..32]) = 32 + 16 tag

### Header integrity Field

v1 reinterprets the 28-byte payload area (bytes [4..32]) of the header as `nonce[N] + reserved[28-N]`, where N is the algorithm's nonce size (12 or 24). The reserved bytes are zeros. KDF salt is stored in kdf_params within the Key Block.

## v1 STREAM Nonce Construction

```
nonce (N bytes) = IV ⊕ counter_padded
last chunk:       nonce[0] ^= 0x80
```

Counter is XORed into the last 8 bytes of the nonce:
- 12-byte nonce: `IV ⊕ (0x00{4} ‖ counter_u64_BE)`, counter at bytes [4..12]
- 24-byte nonce: `IV ⊕ (0x00{16} ‖ counter_u64_BE)`, counter at bytes [16..24]

- IV: random nonce stored in header (12 or 24 bytes depending on algorithm)
- last-chunk flag: high bit of byte [0] — prevents truncation attacks
- Recommended upper limit: 2^32 chunks to avoid nonce reuse

## v1 Header + Key Block Authentication (Associated Data)

The first data chunk includes both the header and the entire Key Block as AD:

```
chunk_0  = AEAD_encrypt(DEK, nonce_0, plaintext_0, ad = header[0..32] || key_block)
chunk_i  = AEAD_encrypt(DEK, nonce_i, plaintext_i, ad = "")    (i > 0)
```

This ensures that tampering with the header, Key Block, or any slot is detected when decrypting the first chunk.

## v0 → v1 Key Differences

| Aspect | v0 | v1 |
|--------|----|----|
| Key model | Single key for AEAD | KEK → DEK unwrap → AEAD |
| integrity field | Suffix appended to plaintext, verified after decryption | reserved (0x00); salt in Key Block |
| KDF parameters | Hard-coded | Stored in Key Block |
| Last-chunk marker | None (integrity suffix for indirect detection) | Nonce bit flag |
| Chunk size | Fixed 8 KiB | Selectable via chunk_kind |
| Header authentication | None (detected indirectly via AEAD failure) | Explicit: DEK unwrap AD + first chunk AD |
| Key rotation | Re-encrypt all data | Re-wrap DEK with new KEK |

## `encrypt_bytes` / `decrypt_bytes`

File name encryption (`encrypt_bytes` / `decrypt_bytes`) is independent of the streaming format.
Single-chunk AEAD + appended nonce (12 or 24 bytes depending on algorithm). Nonce size is determined by the `Cipher`'s algorithm, not auto-detected.

## Implementation Status

All phases are complete:

1. ~~**Key Block types**~~ — `KeyBlock`, `KeySlot`, `KdfId`, `KdfParams` in `header.rs`
2. ~~**DEK generation + wrapping**~~ — Cipher generates random DEK, wraps with KEK
3. ~~**v1 STREAM encryption**~~ — `CounteredNonce` with `is_last`, first chunk AD = header + key_block
4. ~~**v1 STREAM decryption**~~ — Header → Key Block → unwrap DEK → STREAM decrypt
5. ~~**Password mode migration**~~ — Argon2id params stored in Key Block kdf_params
6. ~~**XChaCha20-Poly1305**~~ — 24-byte nonce support via `[u8; 24]` fixed arrays, header payload reinterpretation
7. ~~**Testing**~~ — v0 compat + v1 roundtrip (AES/ChaCha20/XChaCha20) + cross-algo decrypt + tampering + truncation
7. ~~**Documentation**~~ — Update ARCHITECTURE.md aether section
8. ~~**Parallel encryption/decryption**~~ — crossbeam pipeline: Reader → N workers → Writer with BTreeMap reorder. `-j/--jobs` CLI flag. Early termination via `AtomicBool`.
