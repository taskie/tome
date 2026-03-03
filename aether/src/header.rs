use aes_gcm::Nonce;
use bytes::{Buf, BufMut};
use std::io::Write;
use typenum::U12;

use crate::algorithm::CipherAlgorithm;
use crate::error::{AetherError, Result};

pub type Integrity = [u8; INTEGRITY_SIZE];

pub const KEY_SIZE: usize = 32;
pub const HEADER_SIZE: usize = 32;
pub(crate) const NONCE_SIZE: usize = 12;
pub(crate) const COUNTER_SIZE: usize = 8;
pub(crate) const INTEGRITY_SIZE: usize = 16;
const AEAD_TAG_SIZE: usize = 16;
/// Base ciphertext chunk size (chunk_kind=0). Actual = BASE_CHUNK_SIZE << chunk_kind.
const BASE_CHUNK_SIZE: usize = 8192;

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

#[derive(Clone)]
pub struct Header {
    magic: u16,
    pub flags: HeaderFlags,
    pub(crate) iv: Nonce<U12>,
    pub integrity: Integrity,
}

impl Header {
    pub fn new(iv: &Nonce<U12>, integrity: Integrity, flags: HeaderFlags) -> Header {
        Header { magic: 0xae71, flags, iv: *iv, integrity }
    }

    /// Create a v0 header (backward-compatible).
    pub fn new_v0(iv: &Nonce<U12>, integrity: Integrity, algorithm: CipherAlgorithm) -> Header {
        let flags = HeaderFlags::new(0, ChunkKind::V0, algorithm);
        Header::new(iv, integrity, flags)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut header = Vec::with_capacity(HEADER_SIZE);
        header.put_u16(self.magic);
        header.put_u16(self.flags.to_bits());
        header.write_all(self.iv.as_ref()).unwrap();
        header.write_all(self.integrity.as_ref()).unwrap();
        assert_eq!(header.len(), HEADER_SIZE);
        header
    }

    pub fn from_bytes(bs: &[u8]) -> Result<Header> {
        if bs.len() != HEADER_SIZE {
            return Err(AetherError::InvalidHeader("wrong length".into()));
        }
        let mut header = bs;
        let magic = header.get_u16();
        if magic != 0xae71 {
            return Err(AetherError::InvalidHeader("bad magic".into()));
        }
        let raw_flags = header.get_u16();
        let flags = HeaderFlags::from_bits(raw_flags)?;
        let mut iv = Nonce::default();
        iv.as_mut_slice().copy_from_slice(&header[..NONCE_SIZE]);
        header.advance(NONCE_SIZE);
        let mut integrity = [0u8; INTEGRITY_SIZE];
        integrity.copy_from_slice(&header[..INTEGRITY_SIZE]);
        header.advance(INTEGRITY_SIZE);
        Ok(Header { magic, flags, iv, integrity })
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// CounteredNonce
// ──────────────────────────────────────────────────────────────────────────────

pub(crate) struct CounteredNonce {
    pub original: Nonce<U12>,
    pub counter: u64,
}

impl CounteredNonce {
    pub fn new(nonce: Nonce<U12>) -> CounteredNonce {
        CounteredNonce { original: nonce, counter: 0 }
    }

    /// Compute the nonce for the current counter value.
    /// If `is_last` is true, XOR the high bit of byte 0 (STREAM last-chunk flag).
    pub fn peek(&self, is_last: bool) -> Nonce<U12> {
        let mut nonce = self.original;
        let xs = nonce.as_mut_slice();
        let ys = self.counter.to_be_bytes();
        for (x, y) in xs[NONCE_SIZE - COUNTER_SIZE..].iter_mut().zip(ys.iter()) {
            *x ^= y;
        }
        if is_last {
            xs[0] ^= 0x80;
        }
        nonce
    }

    /// Return the current nonce (not last) and advance the counter.
    pub fn next(&mut self) -> Nonce<U12> {
        let nonce = self.peek(false);
        self.counter += 1;
        nonce
    }

    /// Return the last-chunk nonce and advance the counter.
    pub fn next_last(&mut self) -> Nonce<U12> {
        let nonce = self.peek(true);
        self.counter += 1;
        nonce
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_gcm::{Aes256Gcm, aead::AeadCore};

    #[test]
    fn header_flags_roundtrip() {
        let nonce = Aes256Gcm::generate_nonce(&mut aes_gcm::aead::OsRng);
        let integrity = [0u8; INTEGRITY_SIZE];

        // v0 AES
        let header = Header::new_v0(&nonce, integrity, CipherAlgorithm::Aes256Gcm);
        assert_eq!(header.flags.to_bits(), 0x0000);
        let parsed = Header::from_bytes(&header.to_bytes()).unwrap();
        assert_eq!(parsed.flags.version, 0);
        assert_eq!(parsed.flags.algorithm, CipherAlgorithm::Aes256Gcm);
        assert_eq!(parsed.flags.chunk_kind, ChunkKind::V0);

        // v0 ChaCha
        let header = Header::new_v0(&nonce, integrity, CipherAlgorithm::ChaCha20Poly1305);
        assert_eq!(header.flags.to_bits(), 0x0001);
        let parsed = Header::from_bytes(&header.to_bytes()).unwrap();
        assert_eq!(parsed.flags.algorithm, CipherAlgorithm::ChaCha20Poly1305);

        // v1 with chunk_kind=7
        let flags = HeaderFlags::new(1, ChunkKind::DEFAULT, CipherAlgorithm::ChaCha20Poly1305);
        assert_eq!(flags.to_bits(), 0x1071);
        let parsed = HeaderFlags::from_bits(flags.to_bits()).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.chunk_kind, ChunkKind::DEFAULT);
        assert_eq!(parsed.algorithm, CipherAlgorithm::ChaCha20Poly1305);
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
        // Set reserved bits [11:8] to non-zero
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
    fn countered_nonce_last_flag_differs() {
        let nonce = Aes256Gcm::generate_nonce(&mut aes_gcm::aead::OsRng);
        let cn = CounteredNonce::new(nonce);
        let normal = cn.peek(false);
        let last = cn.peek(true);
        assert_ne!(normal, last);
        // Only byte 0 should differ (high bit)
        assert_eq!(normal.as_slice()[0] ^ last.as_slice()[0], 0x80);
        assert_eq!(normal.as_slice()[1..], last.as_slice()[1..]);
    }
}
