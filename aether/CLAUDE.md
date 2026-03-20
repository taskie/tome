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

| Value | Algorithm |
|-------|-----------|
| 0 | AES-256-GCM |
| 1 | ChaCha20-Poly1305 |
| 2–15 | Reserved (AES-256-GCM-SIV, XChaCha20, etc.) |

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
4       12    dek_nonce       — nonce for DEK encryption (independent from data IV)
16      var   kdf_params      — KDF-specific parameters (depends on kdf_id)
var     52×N  slots           — array of KEK slots
```

**kdf_params (kdf_id=0)**: empty (0 bytes)

**kdf_params (kdf_id=1, argon2id)**: salt[16] + m_cost[4] + t_cost[4] + p_cost[4] = 28 bytes

**Slot (52 bytes)**: key_id[4] + encrypted_dek[48]

- `key_id` = first 4 bytes of SHA-256(KEK) for fast slot lookup
- `encrypted_dek` = AEAD(KEK, dek_nonce, DEK, ad=header[0..32]) = 32 + 16 tag

### Header integrity Field

v1 sets the header integrity field to all zeros (reserved). KDF salt is stored in kdf_params within the Key Block instead.

## v1 STREAM Nonce Construction

```
nonce (12 bytes) = IV ⊕ (0x00{4} ‖ counter_u64_BE)
last chunk:        nonce[0] ^= 0x80
```

- IV: random 12 bytes stored in header
- counter: chunk number (0-based), XORed into bytes [4..12]
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
Single-chunk AEAD + appended nonce; unaffected by v1 changes.

## Implementation Phases

1. **Key Block types** — Add `KeyBlock`, `KeySlot`, `KdfId`, `KdfParams` to `header.rs` with serialize/deserialize
2. **DEK generation + wrapping** — Cipher generates random DEK, encrypts it with KEK, builds Key Block
3. **v1 STREAM encryption** — `CounteredNonce` with `is_last`, first chunk AD = header + key_block, encrypt with DEK
4. **v1 STREAM decryption** — Read header → Key Block → unwrap DEK → STREAM decrypt with version dispatch
5. **Password mode migration** — `with_password` stores Argon2id params in Key Block kdf_params instead of header integrity
6. **Testing** — v0 backward compat + v1 roundtrip + multi-slot + chunk_kind variants + truncation + header/key_block tampering
7. **Documentation** — Update ARCHITECTURE.md aether section
