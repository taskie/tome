use aes_gcm::Nonce;
use bytes::{Buf, BufMut};
use std::io::Write;
use typenum::U12;

use crate::algorithm::CipherAlgorithm;
use crate::error::{AetherError, Result};

pub type Integrity = [u8; INTEGRITY_SIZE];

pub const KEY_SIZE: usize = 32;
pub const HEADER_SIZE: usize = 32;
pub(crate) const BUFFER_SIZE: usize = 8192;
pub(crate) const NONCE_SIZE: usize = 12;
pub(crate) const COUNTER_SIZE: usize = 8;
pub(crate) const INTEGRITY_SIZE: usize = 16;

#[derive(Clone)]
pub struct Header {
    magic: u16,
    pub flags: u16,
    pub(crate) iv: Nonce<U12>,
    pub integrity: Integrity,
}

impl Header {
    pub fn new(iv: &Nonce<U12>, integrity: Integrity, algorithm: CipherAlgorithm) -> Header {
        Header { magic: 0xae71, flags: algorithm.flags_bit(), iv: *iv, integrity }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut header = Vec::with_capacity(HEADER_SIZE);
        header.put_u16(self.magic);
        header.put_u16(self.flags);
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
        let flags = header.get_u16();
        let mut iv = Nonce::default();
        iv.as_mut_slice().copy_from_slice(&header[..NONCE_SIZE]);
        header.advance(NONCE_SIZE);
        let mut integrity = [0u8; INTEGRITY_SIZE];
        integrity.copy_from_slice(&header[..INTEGRITY_SIZE]);
        header.advance(INTEGRITY_SIZE);
        Ok(Header { magic, flags, iv, integrity })
    }
}

pub(crate) struct CounteredNonce {
    pub original: Nonce<U12>,
    pub counter: u64,
}

impl CounteredNonce {
    pub fn new(nonce: Nonce<U12>) -> CounteredNonce {
        CounteredNonce { original: nonce, counter: 0 }
    }

    pub fn peek(&self) -> Nonce<U12> {
        let mut nonce = self.original;
        let xs = nonce.as_mut_slice();
        let ys = self.counter.to_be_bytes();
        for (x, y) in xs[NONCE_SIZE - COUNTER_SIZE..].iter_mut().zip(ys.iter()) {
            *x ^= y;
        }
        nonce
    }

    pub fn next(&mut self) -> Nonce<U12> {
        let nonce = self.peek();
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

        let header_aes = Header::new(&nonce, integrity, CipherAlgorithm::Aes256Gcm);
        assert_eq!(header_aes.flags, 0);
        let parsed = Header::from_bytes(&header_aes.to_bytes()).unwrap();
        assert_eq!(CipherAlgorithm::from_flags(parsed.flags).unwrap(), CipherAlgorithm::Aes256Gcm);

        let header_cc = Header::new(&nonce, integrity, CipherAlgorithm::ChaCha20Poly1305);
        assert_eq!(header_cc.flags, 1);
        let parsed = Header::from_bytes(&header_cc.to_bytes()).unwrap();
        assert_eq!(CipherAlgorithm::from_flags(parsed.flags).unwrap(), CipherAlgorithm::ChaCha20Poly1305);
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
}
