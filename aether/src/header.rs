use bytes::{Buf, BufMut};
use sha2::{Digest, Sha256};
use std::io::{BufRead, Write};

use crate::algorithm::CipherAlgorithm;
use crate::error::{AetherError, Result};

pub type Integrity = [u8; INTEGRITY_SIZE];

pub const KEY_SIZE: usize = 32;
pub const HEADER_SIZE: usize = 32;
pub(crate) const NONCE_SIZE_STD: usize = 12;
pub(crate) const MAX_NONCE_SIZE: usize = 24;
pub(crate) const COUNTER_SIZE: usize = 8;
pub(crate) const INTEGRITY_SIZE: usize = 16;
pub(crate) const AEAD_TAG_SIZE: usize = 16;
/// Base ciphertext chunk size (chunk_kind=0). Actual = BASE_CHUNK_SIZE << chunk_kind.
const BASE_CHUNK_SIZE: usize = 8192;

pub(crate) const KEY_ID_SIZE: usize = 4;
pub(crate) const ENCRYPTED_DEK_SIZE: usize = KEY_SIZE + AEAD_TAG_SIZE; // 48
const SLOT_SIZE: usize = KEY_ID_SIZE + ENCRYPTED_DEK_SIZE; // 52

/// Payload area of the header: HEADER_SIZE - magic(2) - flags(2) = 28.
const HEADER_PAYLOAD_SIZE: usize = 28;

// ──────────────────────────────────────────────────────────────────────────────
// HeaderFlags — structured view of the 16-bit flags field
// ──────────────────────────────────────────────────────────────────────────────

/// Parsed representation of the 16-bit header flags.
///
/// ```text
/// bits [15:12]  version      format version (0–15)
/// bits [11:8]   reserved     must be 0
/// bits [7:4]    chunk_kind   chunk size selector
/// bits [3:0]    algorithm    AEAD algorithm
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderFlags {
    pub version: u8,
    pub chunk_kind: ChunkKind,
    pub algorithm: CipherAlgorithm,
}

impl HeaderFlags {
    pub fn new(version: u8, chunk_kind: ChunkKind, algorithm: CipherAlgorithm) -> Self {
        Self { version, chunk_kind, algorithm }
    }

    /// Encode to the raw 16-bit flags value.
    pub fn to_bits(self) -> u16 {
        let v = (self.version as u16 & 0x0F) << 12;
        let c = (self.chunk_kind.0 as u16 & 0x0F) << 4;
        let a = self.algorithm.to_bits() & 0x0F;
        v | c | a
    }

    /// Decode from a raw 16-bit flags value.
    pub fn from_bits(bits: u16) -> Result<Self> {
        let version = ((bits >> 12) & 0x0F) as u8;
        let reserved = ((bits >> 8) & 0x0F) as u8;
        if reserved != 0 {
            return Err(AetherError::InvalidHeader(format!("reserved bits not zero: {reserved:#x}")));
        }
        let chunk_kind = ChunkKind::new(((bits >> 4) & 0x0F) as u8)?;
        let algorithm = CipherAlgorithm::from_bits(bits & 0x0F)?;
        Ok(Self { version, chunk_kind, algorithm })
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// ChunkKind — chunk size selector (4-bit, 0–15)
// ──────────────────────────────────────────────────────────────────────────────

/// Chunk size selector. Ciphertext chunk size = `8 KiB × 2^value`.
///
/// | value | ciphertext size |
/// |-------|----------------|
/// | 0     | 8 KiB          |
/// | 7     | 1 MiB (default)|
/// | 13    | 64 MiB         |
/// | 15    | 256 MiB        |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkKind(u8);

impl ChunkKind {
    /// v0 chunk kind (always 8 KiB, chunk_kind=0).
    pub const V0: ChunkKind = ChunkKind(0);
    /// Default for v1: chunk_kind=7 (1 MiB).
    pub const DEFAULT: ChunkKind = ChunkKind(7);

    pub fn new(value: u8) -> Result<Self> {
        if value > 15 {
            return Err(AetherError::InvalidHeader(format!("chunk_kind out of range: {value}")));
        }
        Ok(ChunkKind(value))
    }

    pub fn value(self) -> u8 {
        self.0
    }

    /// Ciphertext chunk size in bytes (plaintext + 16-byte AEAD tag).
    pub fn ciphertext_size(self) -> usize {
        BASE_CHUNK_SIZE << self.0
    }

    /// Maximum plaintext bytes per chunk (ciphertext - tag).
    pub fn plaintext_size(self) -> usize {
        self.ciphertext_size() - AEAD_TAG_SIZE
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Header
// ──────────────────────────────────────────────────────────────────────────────

/// 32-byte file header.
///
/// Layout of `payload` depends on version and algorithm:
///   - v0: `IV[12] + integrity[16]`
///   - v1 (12-byte nonce): `nonce[12] + reserved[16]`
///   - v1 (24-byte nonce): `nonce[24] + reserved[4]`
#[derive(Clone)]
pub struct Header {
    magic: u16,
    pub flags: HeaderFlags,
    payload: [u8; HEADER_PAYLOAD_SIZE],
}

impl Header {
    /// Create a v1 header. `nonce` length must match `flags.algorithm.nonce_size()`.
    pub fn new_v1(nonce: &[u8], flags: HeaderFlags) -> Header {
        debug_assert_eq!(nonce.len(), flags.algorithm.nonce_size());
        let mut payload = [0u8; HEADER_PAYLOAD_SIZE];
        payload[..nonce.len()].copy_from_slice(nonce);
        Header { magic: 0xae71, flags, payload }
    }

    /// Create a v0 header (backward-compatible, 12-byte nonce only).
    pub fn new_v0(iv: &[u8; NONCE_SIZE_STD], integrity: Integrity, algorithm: CipherAlgorithm) -> Header {
        debug_assert_eq!(algorithm.nonce_size(), NONCE_SIZE_STD);
        let flags = HeaderFlags::new(0, ChunkKind::V0, algorithm);
        let mut payload = [0u8; HEADER_PAYLOAD_SIZE];
        payload[..NONCE_SIZE_STD].copy_from_slice(iv);
        payload[NONCE_SIZE_STD..NONCE_SIZE_STD + INTEGRITY_SIZE].copy_from_slice(&integrity);
        Header { magic: 0xae71, flags, payload }
    }

    /// Get the nonce bytes. Length depends on algorithm.
    pub fn nonce_bytes(&self) -> &[u8] {
        &self.payload[..self.flags.algorithm.nonce_size()]
    }

    /// Get the integrity field (bytes [12..28] of payload). Only meaningful for v0.
    pub fn integrity(&self) -> Integrity {
        let mut integrity = [0u8; INTEGRITY_SIZE];
        integrity.copy_from_slice(&self.payload[NONCE_SIZE_STD..NONCE_SIZE_STD + INTEGRITY_SIZE]);
        integrity
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut header = Vec::with_capacity(HEADER_SIZE);
        header.put_u16(self.magic);
        header.put_u16(self.flags.to_bits());
        header.write_all(&self.payload).unwrap();
        assert_eq!(header.len(), HEADER_SIZE);
        header
    }

    pub fn from_bytes(bs: &[u8]) -> Result<Header> {
        if bs.len() != HEADER_SIZE {
            return Err(AetherError::InvalidHeader("wrong length".into()));
        }
        let mut cursor = bs;
        let magic = cursor.get_u16();
        if magic != 0xae71 {
            return Err(AetherError::InvalidHeader("bad magic".into()));
        }
        let raw_flags = cursor.get_u16();
        let flags = HeaderFlags::from_bits(raw_flags)?;
        let mut payload = [0u8; HEADER_PAYLOAD_SIZE];
        payload.copy_from_slice(&cursor[..HEADER_PAYLOAD_SIZE]);
        Ok(Header { magic, flags, payload })
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// CounteredNonce
// ──────────────────────────────────────────────────────────────────────────────

/// Nonce with an embedded counter for STREAM construction.
///
/// Stores a fixed `[u8; 24]` buffer. For 12-byte algorithms, only `[0..12]` is used.
/// Counter is XORed into the last `COUNTER_SIZE` bytes of the active nonce area.
/// Last-chunk flag: `nonce[0] ^= 0x80`.
pub(crate) struct CounteredNonce {
    original: [u8; MAX_NONCE_SIZE],
    nonce_size: usize,
    pub counter: u64,
}

impl CounteredNonce {
    pub fn new(nonce: &[u8]) -> CounteredNonce {
        let nonce_size = nonce.len();
        debug_assert!(nonce_size == 12 || nonce_size == 24);
        let mut original = [0u8; MAX_NONCE_SIZE];
        original[..nonce_size].copy_from_slice(nonce);
        CounteredNonce { original, nonce_size, counter: 0 }
    }

    /// Compute the nonce for the current counter value.
    /// If `is_last` is true, XOR the high bit of byte 0 (STREAM last-chunk flag).
    pub fn peek(&self, is_last: bool) -> [u8; MAX_NONCE_SIZE] {
        let mut nonce = self.original;
        let counter_start = self.nonce_size - COUNTER_SIZE;
        let ys = self.counter.to_be_bytes();
        for (x, y) in nonce[counter_start..self.nonce_size].iter_mut().zip(ys.iter()) {
            *x ^= y;
        }
        if is_last {
            nonce[0] ^= 0x80;
        }
        nonce
    }

    /// Return the current nonce (not last) and advance the counter.
    pub fn next(&mut self) -> [u8; MAX_NONCE_SIZE] {
        let nonce = self.peek(false);
        self.counter += 1;
        nonce
    }

    /// Return the last-chunk nonce and advance the counter.
    pub fn next_last(&mut self) -> [u8; MAX_NONCE_SIZE] {
        let nonce = self.peek(true);
        self.counter += 1;
        nonce
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// KdfId — key derivation function identifier
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KdfId {
    None = 0,
    Argon2id = 1,
}

impl KdfId {
    pub fn from_u8(v: u8) -> Result<Self> {
        match v {
            0 => Ok(Self::None),
            1 => Ok(Self::Argon2id),
            _ => Err(AetherError::InvalidHeader(format!("unknown kdf_id: {v}"))),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// KdfParams — KDF-specific parameters stored in the Key Block
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KdfParams {
    None,
    Argon2id { salt: [u8; INTEGRITY_SIZE], m_cost: u32, t_cost: u32, p_cost: u32 },
}

impl KdfParams {
    pub fn kdf_id(&self) -> KdfId {
        match self {
            Self::None => KdfId::None,
            Self::Argon2id { .. } => KdfId::Argon2id,
        }
    }

    fn serialized_size(&self) -> usize {
        match self {
            Self::None => 0,
            Self::Argon2id { .. } => INTEGRITY_SIZE + 4 + 4 + 4, // salt(16) + m_cost(4) + t_cost(4) + p_cost(4)
        }
    }

    fn write_to<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        match self {
            Self::None => {}
            Self::Argon2id { salt, m_cost, t_cost, p_cost } => {
                w.write_all(salt)?;
                w.write_all(&m_cost.to_be_bytes())?;
                w.write_all(&t_cost.to_be_bytes())?;
                w.write_all(&p_cost.to_be_bytes())?;
            }
        }
        Ok(())
    }

    fn read_from(kdf_id: KdfId, r: &mut &[u8]) -> Result<Self> {
        match kdf_id {
            KdfId::None => Ok(Self::None),
            KdfId::Argon2id => {
                if r.len() < INTEGRITY_SIZE + 12 {
                    return Err(AetherError::InvalidHeader("kdf_params too short for argon2id".into()));
                }
                let mut salt = [0u8; INTEGRITY_SIZE];
                salt.copy_from_slice(&r[..INTEGRITY_SIZE]);
                r.advance(INTEGRITY_SIZE);
                let m_cost = r.get_u32();
                let t_cost = r.get_u32();
                let p_cost = r.get_u32();
                Ok(Self::Argon2id { salt, m_cost, t_cost, p_cost })
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// KeySlot — a single KEK slot in the Key Block
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct KeySlot {
    pub key_id: [u8; KEY_ID_SIZE],
    pub encrypted_dek: [u8; ENCRYPTED_DEK_SIZE],
}

impl KeySlot {
    fn write_to<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        w.write_all(&self.key_id)?;
        w.write_all(&self.encrypted_dek)?;
        Ok(())
    }

    fn read_from(r: &mut &[u8]) -> Result<Self> {
        if r.len() < SLOT_SIZE {
            return Err(AetherError::InvalidHeader("slot data too short".into()));
        }
        let mut key_id = [0u8; KEY_ID_SIZE];
        key_id.copy_from_slice(&r[..KEY_ID_SIZE]);
        r.advance(KEY_ID_SIZE);
        let mut encrypted_dek = [0u8; ENCRYPTED_DEK_SIZE];
        encrypted_dek.copy_from_slice(&r[..ENCRYPTED_DEK_SIZE]);
        r.advance(ENCRYPTED_DEK_SIZE);
        Ok(Self { key_id, encrypted_dek })
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// KeyBlock — envelope encryption metadata between header and data chunks
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct KeyBlock {
    pub kdf_params: KdfParams,
    pub dek_nonce: [u8; MAX_NONCE_SIZE],
    pub nonce_size: usize,
    pub slots: Vec<KeySlot>,
}

impl KeyBlock {
    pub fn serialized_size(&self) -> usize {
        // key_block_len(2) + kdf_id(1) + slot_count(1) + dek_nonce(nonce_size) + kdf_params + slots
        4 + self.nonce_size + self.kdf_params.serialized_size() + SLOT_SIZE * self.slots.len()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let size = self.serialized_size();
        let mut buf = Vec::with_capacity(size);
        buf.put_u16(size as u16);
        buf.push(self.kdf_params.kdf_id() as u8);
        buf.push(self.slots.len() as u8);
        buf.write_all(&self.dek_nonce[..self.nonce_size]).unwrap();
        self.kdf_params.write_to(&mut buf).unwrap();
        for slot in &self.slots {
            slot.write_to(&mut buf).unwrap();
        }
        debug_assert_eq!(buf.len(), size);
        buf
    }

    /// Read a Key Block from a reader. `nonce_size` is determined by the algorithm in the header.
    /// Returns the parsed block and its raw bytes (for AD use).
    pub fn from_reader<R: BufRead>(r: &mut R, nonce_size: usize) -> Result<(Self, Vec<u8>)> {
        let mut len_buf = [0u8; 2];
        r.read_exact(&mut len_buf)?;
        let key_block_len = u16::from_be_bytes(len_buf) as usize;
        let min_size = 4 + nonce_size;
        if key_block_len < min_size {
            return Err(AetherError::InvalidHeader("key_block_len too small".into()));
        }

        let mut remaining = vec![0u8; key_block_len - 2];
        r.read_exact(&mut remaining)?;

        // Reconstruct full bytes for AD
        let mut full_bytes = Vec::with_capacity(key_block_len);
        full_bytes.extend_from_slice(&len_buf);
        full_bytes.extend_from_slice(&remaining);

        let mut cursor: &[u8] = &remaining;
        let kdf_id = KdfId::from_u8(cursor.get_u8())?;
        let slot_count = cursor.get_u8();

        if cursor.len() < nonce_size {
            return Err(AetherError::InvalidHeader("key block too short for dek_nonce".into()));
        }
        let mut dek_nonce = [0u8; MAX_NONCE_SIZE];
        dek_nonce[..nonce_size].copy_from_slice(&cursor[..nonce_size]);
        cursor.advance(nonce_size);

        let kdf_params = KdfParams::read_from(kdf_id, &mut cursor)?;

        let mut slots = Vec::with_capacity(slot_count as usize);
        for _ in 0..slot_count {
            slots.push(KeySlot::read_from(&mut cursor)?);
        }

        Ok((Self { kdf_params, dek_nonce, nonce_size, slots }, full_bytes))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Utility
// ──────────────────────────────────────────────────────────────────────────────

/// Compute key_id = SHA-256(key)[0..4] for fast slot lookup.
pub(crate) fn compute_key_id(key: &[u8; KEY_SIZE]) -> [u8; KEY_ID_SIZE] {
    let hash = Sha256::digest(key);
    let mut id = [0u8; KEY_ID_SIZE];
    id.copy_from_slice(&hash[..KEY_ID_SIZE]);
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto_bigint::rand_core::RngCore;

    fn rng() -> aes_gcm::aead::OsRng {
        aes_gcm::aead::OsRng
    }

    #[test]
    fn header_flags_roundtrip() {
        let mut iv = [0u8; NONCE_SIZE_STD];
        rng().fill_bytes(&mut iv);
        let integrity = [0u8; INTEGRITY_SIZE];

        // v0 AES
        let header = Header::new_v0(&iv, integrity, CipherAlgorithm::Aes256Gcm);
        assert_eq!(header.flags.to_bits(), 0x0000);
        let parsed = Header::from_bytes(&header.to_bytes()).unwrap();
        assert_eq!(parsed.flags.version, 0);
        assert_eq!(parsed.flags.algorithm, CipherAlgorithm::Aes256Gcm);
        assert_eq!(parsed.flags.chunk_kind, ChunkKind::V0);

        // v0 ChaCha
        let header = Header::new_v0(&iv, integrity, CipherAlgorithm::ChaCha20Poly1305);
        assert_eq!(header.flags.to_bits(), 0x0001);
        let parsed = Header::from_bytes(&header.to_bytes()).unwrap();
        assert_eq!(parsed.flags.algorithm, CipherAlgorithm::ChaCha20Poly1305);

        // v1 with chunk_kind=7 ChaCha
        let flags = HeaderFlags::new(1, ChunkKind::DEFAULT, CipherAlgorithm::ChaCha20Poly1305);
        assert_eq!(flags.to_bits(), 0x1071);
        let parsed = HeaderFlags::from_bits(flags.to_bits()).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.chunk_kind, ChunkKind::DEFAULT);
        assert_eq!(parsed.algorithm, CipherAlgorithm::ChaCha20Poly1305);

        // v1 with XChaCha20 (algorithm=2)
        let flags = HeaderFlags::new(1, ChunkKind::DEFAULT, CipherAlgorithm::XChaCha20Poly1305);
        assert_eq!(flags.to_bits(), 0x1072);
        let parsed = HeaderFlags::from_bits(flags.to_bits()).unwrap();
        assert_eq!(parsed.algorithm, CipherAlgorithm::XChaCha20Poly1305);
    }

    #[test]
    fn header_v1_extended_nonce_roundtrip() {
        let mut nonce = [0u8; MAX_NONCE_SIZE];
        rng().fill_bytes(&mut nonce);
        let flags = HeaderFlags::new(1, ChunkKind::DEFAULT, CipherAlgorithm::XChaCha20Poly1305);
        let header = Header::new_v1(&nonce, flags);
        let bytes = header.to_bytes();
        assert_eq!(bytes.len(), HEADER_SIZE);

        let parsed = Header::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.nonce_bytes(), &nonce[..]);
        // Remaining 4 bytes should be zero
        assert_eq!(parsed.payload[MAX_NONCE_SIZE..], [0u8; 4]);
    }

    #[test]
    fn header_rejects_bad_magic() {
        let mut bytes = [0u8; HEADER_SIZE];
        bytes[0] = 0xFF;
        bytes[1] = 0xFF;
        assert!(Header::from_bytes(&bytes).is_err());
    }

    #[test]
    fn header_rejects_wrong_length() {
        assert!(Header::from_bytes(&[0u8; 16]).is_err());
    }

    #[test]
    fn header_rejects_reserved_bits() {
        let flags_raw: u16 = 0x0100; // reserved = 1
        assert!(HeaderFlags::from_bits(flags_raw).is_err());
    }

    #[test]
    fn chunk_kind_sizes() {
        assert_eq!(ChunkKind::new(0).unwrap().ciphertext_size(), 8192);
        assert_eq!(ChunkKind::new(0).unwrap().plaintext_size(), 8192 - 16);
        assert_eq!(ChunkKind::new(7).unwrap().ciphertext_size(), 1024 * 1024);
        assert_eq!(ChunkKind::new(13).unwrap().ciphertext_size(), 64 * 1024 * 1024);
        assert_eq!(ChunkKind::new(15).unwrap().ciphertext_size(), 256 * 1024 * 1024);
        assert!(ChunkKind::new(16).is_err());
    }

    #[test]
    fn countered_nonce_12_last_flag_differs() {
        let mut iv = [0u8; NONCE_SIZE_STD];
        rng().fill_bytes(&mut iv);
        let cn = CounteredNonce::new(&iv);
        let normal = cn.peek(false);
        let last = cn.peek(true);
        assert_ne!(normal, last);
        assert_eq!(normal[0] ^ last[0], 0x80);
        assert_eq!(normal[1..NONCE_SIZE_STD], last[1..NONCE_SIZE_STD]);
    }

    #[test]
    fn countered_nonce_24_last_flag_differs() {
        let mut iv = [0u8; MAX_NONCE_SIZE];
        rng().fill_bytes(&mut iv);
        let cn = CounteredNonce::new(&iv);
        let normal = cn.peek(false);
        let last = cn.peek(true);
        assert_ne!(normal, last);
        assert_eq!(normal[0] ^ last[0], 0x80);
        assert_eq!(normal[1..MAX_NONCE_SIZE], last[1..MAX_NONCE_SIZE]);
    }

    #[test]
    fn countered_nonce_24_counter_xors_into_tail() {
        let iv = [0u8; MAX_NONCE_SIZE];
        let mut cn = CounteredNonce::new(&iv);
        let n0 = cn.next();
        let n1 = cn.next();
        // Counter 0: nonce should be all zeros (0 XOR 0)
        assert_eq!(n0, [0u8; MAX_NONCE_SIZE]);
        // Counter 1: last 8 bytes should have counter=1
        assert_eq!(&n1[..16], &[0u8; 16]);
        assert_eq!(n1[23], 1);
    }
}
