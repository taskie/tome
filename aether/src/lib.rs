use std::{
    ffi::{OsStr, OsString},
    io::{BufRead, BufWriter, Write},
    os::unix::ffi::{OsStrExt as _, OsStringExt},
};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{AeadCore, KeyInit, OsRng},
};
use argon2::Argon2;
use base64::Engine;
use bytes::{Buf, BufMut};
use chacha20poly1305::ChaCha20Poly1305;
use crypto_bigint::rand_core::RngCore as _;
use typenum::U12;

type Integrity = [u8; INTEGRITY_SIZE];

/// Supported AEAD algorithms.  Both use 32-byte keys, 12-byte nonces, and 16-byte tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CipherAlgorithm {
    #[default]
    Aes256Gcm,
    ChaCha20Poly1305,
}

impl CipherAlgorithm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Aes256Gcm => "aes256gcm",
            Self::ChaCha20Poly1305 => "chacha20-poly1305",
        }
    }

    fn flags_bit(self) -> u16 {
        match self {
            Self::Aes256Gcm => 0,
            Self::ChaCha20Poly1305 => 1,
        }
    }

    fn from_flags(flags: u16) -> Result<Self, std::io::Error> {
        match flags & 0x0001 {
            0 => Ok(Self::Aes256Gcm),
            1 => Ok(Self::ChaCha20Poly1305),
            _ => unreachable!(),
        }
    }
}

impl std::fmt::Display for CipherAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for CipherAlgorithm {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "aes256gcm" | "aes-256-gcm" | "aes" => Ok(Self::Aes256Gcm),
            "chacha20-poly1305" | "chacha20poly1305" | "chacha20" => Ok(Self::ChaCha20Poly1305),
            other => Err(format!("unknown cipher algorithm {:?}; expected aes256gcm or chacha20-poly1305", other)),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// AEAD enum dispatch
// ──────────────────────────────────────────────────────────────────────────────

enum AeadInner {
    Aes(Box<Aes256Gcm>),
    ChaCha(ChaCha20Poly1305),
}

impl AeadInner {
    fn new(algo: CipherAlgorithm, key: &[u8; KEY_SIZE]) -> Self {
        match algo {
            CipherAlgorithm::Aes256Gcm => {
                Self::Aes(Box::new(Aes256Gcm::new(aes_gcm::Key::<Aes256Gcm>::from_slice(key))))
            }
            CipherAlgorithm::ChaCha20Poly1305 => {
                Self::ChaCha(ChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(key)))
            }
        }
    }

    fn encrypt(&self, nonce: &Nonce<U12>, plaintext: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        use aes_gcm::aead::Aead;
        match self {
            Self::Aes(gcm) => gcm.encrypt(nonce, plaintext),
            Self::ChaCha(cc) => cc.encrypt(nonce, plaintext),
        }
        .map_err(|e| std::io::Error::other(e.to_string()))
    }

    fn decrypt(&self, nonce: &Nonce<U12>, ciphertext: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        use aes_gcm::aead::Aead;
        match self {
            Self::Aes(gcm) => gcm.decrypt(nonce, ciphertext),
            Self::ChaCha(cc) => cc.decrypt(nonce, ciphertext),
        }
        .map_err(|e| std::io::Error::other(e.to_string()))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Cipher
// ──────────────────────────────────────────────────────────────────────────────

pub struct Cipher {
    aead: AeadInner,
    algorithm: CipherAlgorithm,
    key: [u8; KEY_SIZE],
    countered_nonce: CounteredNonce,
    integrity: Option<Integrity>,
}

pub const KEY_SIZE: usize = 32;
pub const HEADER_SIZE: usize = 32;
const BUFFER_SIZE: usize = 8192;
const NONCE_SIZE: usize = 12;
const COUNTER_SIZE: usize = 8;
const INTEGRITY_SIZE: usize = 16;

impl Cipher {
    fn new0(key: &[u8; KEY_SIZE], algorithm: CipherAlgorithm, integrity: Option<Integrity>) -> Cipher {
        let aead = AeadInner::new(algorithm, key);
        let countered_nonce = CounteredNonce::new(Aes256Gcm::generate_nonce(&mut OsRng));
        Cipher { aead, algorithm, key: *key, countered_nonce, integrity }
    }

    pub fn new(key: &[u8; KEY_SIZE]) -> Cipher {
        Cipher::new0(key, CipherAlgorithm::default(), None)
    }

    pub fn with_algorithm(key: &[u8; KEY_SIZE], algorithm: CipherAlgorithm) -> Cipher {
        Cipher::new0(key, algorithm, None)
    }

    pub fn with_key_slice(key: &[u8]) -> Cipher {
        let key: &[u8; KEY_SIZE] = key.try_into().expect("key must be 32 bytes");
        Cipher::new(key)
    }

    pub fn with_key_slice_algorithm(key: &[u8], algorithm: CipherAlgorithm) -> Cipher {
        let key: &[u8; KEY_SIZE] = key.try_into().expect("key must be 32 bytes");
        Cipher::with_algorithm(key, algorithm)
    }

    pub fn with_key_b64(s: &str) -> Cipher {
        let key = base64::prelude::BASE64_STANDARD.decode(s).unwrap();
        Cipher::with_key_slice(&key)
    }

    pub fn with_key_b64_algorithm(s: &str, algorithm: CipherAlgorithm) -> Cipher {
        let key = base64::prelude::BASE64_STANDARD.decode(s).unwrap();
        Cipher::with_key_slice_algorithm(&key, algorithm)
    }

    pub fn with_password(password: &[u8], salt: Option<Integrity>) -> Cipher {
        Cipher::with_password_algorithm(password, salt, CipherAlgorithm::default())
    }

    pub fn with_password_algorithm(password: &[u8], salt: Option<Integrity>, algorithm: CipherAlgorithm) -> Cipher {
        let salt = salt.unwrap_or_else(|| {
            let mut salt = [0u8; INTEGRITY_SIZE];
            OsRng.fill_bytes(&mut salt);
            salt
        });
        let argon2 = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(19 * 1024, 2, 1, Some(32)).unwrap(),
        );
        let mut key = [0u8; KEY_SIZE];
        argon2.hash_password_into(password, &salt, &mut key).unwrap();
        Cipher::new0(&key, algorithm, Some(salt))
    }

    pub fn encrypt<R: BufRead, W: Write>(&mut self, r: R, mut w: BufWriter<W>) -> Result<(), std::io::Error> {
        let mut countered_nonce = CounteredNonce::new(Aes256Gcm::generate_nonce(&mut OsRng));
        let integrity = if let Some(integrity) = self.integrity {
            integrity
        } else {
            let mut integrity = [0u8; INTEGRITY_SIZE];
            OsRng.fill_bytes(&mut integrity);
            integrity
        };
        let header = Header::new(&countered_nonce.peek(), integrity, self.algorithm).to_bytes();
        w.write_all(&header)?;
        let mut r = r.chain(&integrity[..]);
        loop {
            let mut buf = [0u8; BUFFER_SIZE - 16];
            let pos = self.read_exact_or_eof(&mut r, &mut buf)?;
            if pos == 0 {
                break;
            }
            let nonce = countered_nonce.next();
            let ciphertext = self.aead.encrypt(&nonce, &buf[..pos])?;
            w.write_all(&ciphertext)?;
        }
        Ok(())
    }

    pub fn encrypt_bytes(&mut self, bs: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        let mut result = Vec::with_capacity(NONCE_SIZE + bs.len() + 16);
        let nonce = self.countered_nonce.next();
        let mut enc = self.aead.encrypt(&nonce, bs)?;
        result.append(&mut enc);
        result.extend_from_slice(nonce.as_slice());
        Ok(result)
    }

    pub fn encrypt_file_name(&mut self, s: &OsStr) -> Result<OsString, std::io::Error> {
        let bs = s.as_bytes();
        let ciphertext = self.encrypt_bytes(bs)?;
        let b64 = base64::prelude::BASE64_URL_SAFE_NO_PAD.encode(&ciphertext);
        Ok(OsString::from(b64))
    }

    pub fn decrypt<R: BufRead, W: Write>(&mut self, mut r: R, mut w: BufWriter<W>) -> Result<(), std::io::Error> {
        let mut header_bytes = [0u8; HEADER_SIZE];
        r.read_exact(&mut header_bytes)?;
        let header = Header::from_bytes(&header_bytes)?;
        // Auto-detect algorithm from header flags
        let algo = CipherAlgorithm::from_flags(header.flags)?;
        let key = self.raw_key();
        let aead = AeadInner::new(algo, &key);
        let mut countered_nonce = CounteredNonce::new(header.iv);
        let mut tmp_old = Vec::with_capacity(BUFFER_SIZE);
        let mut tmp_new = Vec::with_capacity(BUFFER_SIZE);
        loop {
            let mut buf = [0u8; BUFFER_SIZE];
            let pos = self.read_exact_or_eof(&mut r, &mut buf)?;
            if pos == 0 {
                break;
            }
            let nonce = countered_nonce.next();
            let plaintext = aead.decrypt(&nonce, &buf[..pos])?;
            if !tmp_old.is_empty() {
                w.write_all(&tmp_old)?;
            }
            tmp_old.clear();
            tmp_old.append(&mut tmp_new);
            tmp_new.extend_from_slice(&plaintext);
        }
        tmp_old.append(&mut tmp_new);
        if tmp_old.len() < INTEGRITY_SIZE {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid data"));
        }
        let (tmp, actual_integrity) = tmp_old.split_at(tmp_old.len() - INTEGRITY_SIZE);
        if header.integrity != actual_integrity {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid integrity"));
        }
        w.write_all(tmp)?;
        Ok(())
    }

    pub fn decrypt_bytes(&mut self, bs: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        let mut bs = bs.to_vec();
        let nonce = bs.split_off(bs.len() - NONCE_SIZE);
        let nonce = Nonce::from_slice(&nonce);
        let plaintext = self.aead.decrypt(nonce, bs.as_slice())?;
        Ok(plaintext)
    }

    pub fn decrypt_file_name(&mut self, s: &OsStr) -> Result<OsString, std::io::Error> {
        let ciphertext = base64::prelude::BASE64_URL_SAFE_NO_PAD
            .decode(s.as_bytes())
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let plaintext = self.decrypt_bytes(&ciphertext)?;
        Ok(OsString::from_vec(plaintext))
    }

    /// Extract the raw 32-byte key for re-creating AEAD with a different algorithm.
    fn raw_key(&self) -> [u8; KEY_SIZE] {
        self.key
    }

    fn read_exact_or_eof<R: BufRead>(&self, r: &mut R, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let mut pos = 0usize;
        loop {
            let n = r.read(&mut buf[pos..])?;
            pos += n;
            if n == 0 || pos == BUFFER_SIZE {
                break;
            }
        }
        Ok(pos)
    }
}

#[derive(Clone)]
pub struct Header {
    magic: u16,
    pub flags: u16,
    iv: Nonce<U12>,
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

    pub fn from_bytes(bs: &[u8]) -> Result<Header, std::io::Error> {
        if bs.len() != HEADER_SIZE {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid header (len)"));
        }
        let mut header = bs;
        let magic = header.get_u16();
        if magic != 0xae71 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid header (magic)"));
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

struct CounteredNonce {
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
        for i in 0..ys.len() {
            xs[i + NONCE_SIZE - COUNTER_SIZE] ^= ys[i];
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
    use std::io::BufWriter;

    use crate::{CipherAlgorithm, INTEGRITY_SIZE, KEY_SIZE};

    use super::Cipher;

    fn xxd(buf: &[u8], start: usize, end: usize) {
        for i in start..end.min(buf.len()) {
            if (i - start) % 16 == 0 && i - start != 0 {
                println!();
            }
            print!("{:02x} ", buf[i]);
        }
        println!();
    }

    fn roundtrip(algo: CipherAlgorithm, plaintext: &[u8]) {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, algo);
        let mut ciphertext = Vec::new();
        let bw = BufWriter::new(&mut ciphertext);
        cipher.encrypt(&plaintext[..], bw).unwrap();
        let mut plaintext2 = Vec::new();
        let bw = BufWriter::new(&mut plaintext2);
        cipher.decrypt(&ciphertext[..], bw).unwrap();
        assert_eq!(plaintext, &plaintext2[..]);
    }

    #[test]
    fn test_aes_small() {
        roundtrip(CipherAlgorithm::Aes256Gcm, b"Hello, world!");
    }

    #[test]
    fn test_chacha_small() {
        roundtrip(CipherAlgorithm::ChaCha20Poly1305, b"Hello, world!");
    }

    #[test]
    fn test_aes_large() {
        roundtrip(CipherAlgorithm::Aes256Gcm, &vec![0u8; 10240]);
    }

    #[test]
    fn test_chacha_large() {
        roundtrip(CipherAlgorithm::ChaCha20Poly1305, &vec![0u8; 10240]);
    }

    #[test]
    fn test_cross_algo_decrypt_fails() {
        // Encrypt with AES, try to decrypt with ChaCha — should fail (integrity mismatch)
        // But since decrypt auto-detects from header flags, this actually works correctly.
        let key = [42u8; KEY_SIZE];
        let mut cipher_aes = Cipher::with_algorithm(&key, CipherAlgorithm::Aes256Gcm);
        let plaintext = b"cross-algo test";
        let mut ciphertext = Vec::new();
        cipher_aes.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();

        // Decrypt with a ChaCha cipher — should auto-detect AES from header
        let mut cipher_chacha = Cipher::with_algorithm(&key, CipherAlgorithm::ChaCha20Poly1305);
        let mut result = Vec::new();
        cipher_chacha.decrypt(&ciphertext[..], BufWriter::new(&mut result)).unwrap();
        assert_eq!(plaintext.as_slice(), &result[..]);
    }

    #[test]
    fn test_file_name_aes() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, CipherAlgorithm::Aes256Gcm);
        let s = "hello_world.txt";
        let ciphertext = cipher.encrypt_file_name(s.as_ref()).unwrap();
        let plaintext = cipher.decrypt_file_name(&ciphertext).unwrap();
        assert_eq!(s, plaintext.to_string_lossy());
    }

    #[test]
    fn test_file_name_chacha() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, CipherAlgorithm::ChaCha20Poly1305);
        let s = "hello_world.txt";
        let ciphertext = cipher.encrypt_file_name(s.as_ref()).unwrap();
        let plaintext = cipher.decrypt_file_name(&ciphertext).unwrap();
        assert_eq!(s, plaintext.to_string_lossy());
    }

    #[test]
    fn test_password_aes() {
        let password = b"test";
        let integrity = [42u8; INTEGRITY_SIZE];
        let mut cipher = Cipher::with_password_algorithm(password, Some(integrity), CipherAlgorithm::Aes256Gcm);
        let plaintext = b"Hello, world!";
        let mut ciphertext = Vec::new();
        cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();
        cipher.decrypt(&ciphertext[..], BufWriter::new(&mut Vec::new())).unwrap();
    }

    #[test]
    fn test_password_chacha() {
        let password = b"test";
        let integrity = [42u8; INTEGRITY_SIZE];
        let mut cipher = Cipher::with_password_algorithm(password, Some(integrity), CipherAlgorithm::ChaCha20Poly1305);
        let plaintext = b"Hello, world!";
        let mut ciphertext = Vec::new();
        cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();
        let mut result = Vec::new();
        cipher.decrypt(&ciphertext[..], BufWriter::new(&mut result)).unwrap();
        assert_eq!(plaintext.as_slice(), &result[..]);
    }

    #[test]
    fn test_header_flags_roundtrip() {
        use crate::Header;
        use aes_gcm::{Aes256Gcm, aead::AeadCore};
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

    // Legacy: keep a test with the old Key<Aes256Gcm> API pattern for backward compat
    #[test]
    fn test_legacy_api() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::new(&key);
        let plaintext = b"Hello, world!";
        let mut ciphertext = Vec::new();
        cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();
        xxd(&ciphertext, 0, ciphertext.len());
        let mut plaintext2 = Vec::new();
        cipher.decrypt(&ciphertext[..], BufWriter::new(&mut plaintext2)).unwrap();
        assert_eq!(plaintext.as_slice(), &plaintext2[..]);
    }
}
