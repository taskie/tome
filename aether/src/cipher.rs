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
use chacha20poly1305::ChaCha20Poly1305;
use crypto_bigint::rand_core::RngCore as _;
use typenum::U12;
use zeroize::Zeroize;

use crate::algorithm::CipherAlgorithm;
use crate::error::{AetherError, Result};
use crate::header::{BUFFER_SIZE, CounteredNonce, Header, INTEGRITY_SIZE, Integrity, KEY_SIZE, NONCE_SIZE};

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

    fn encrypt(&self, nonce: &Nonce<U12>, plaintext: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::aead::Aead;
        match self {
            Self::Aes(gcm) => gcm.encrypt(nonce, plaintext),
            Self::ChaCha(cc) => cc.encrypt(nonce, plaintext),
        }
        .map_err(|e| AetherError::Encryption(e.to_string()))
    }

    fn decrypt(&self, nonce: &Nonce<U12>, ciphertext: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::aead::Aead;
        match self {
            Self::Aes(gcm) => gcm.decrypt(nonce, ciphertext),
            Self::ChaCha(cc) => cc.decrypt(nonce, ciphertext),
        }
        .map_err(|e| AetherError::Decryption(e.to_string()))
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

impl Drop for Cipher {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

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

    pub fn with_key_slice(key: &[u8]) -> Result<Cipher> {
        let key: &[u8; KEY_SIZE] =
            key.try_into().map_err(|_| AetherError::InvalidKeyLength { expected: KEY_SIZE, actual: key.len() })?;
        Ok(Cipher::new(key))
    }

    pub fn with_key_slice_algorithm(key: &[u8], algorithm: CipherAlgorithm) -> Result<Cipher> {
        let key: &[u8; KEY_SIZE] =
            key.try_into().map_err(|_| AetherError::InvalidKeyLength { expected: KEY_SIZE, actual: key.len() })?;
        Ok(Cipher::with_algorithm(key, algorithm))
    }

    pub fn with_key_b64(s: &str) -> Result<Cipher> {
        let key = base64::prelude::BASE64_STANDARD.decode(s).map_err(|e| AetherError::Base64(e.to_string()))?;
        Cipher::with_key_slice(&key)
    }

    pub fn with_key_b64_algorithm(s: &str, algorithm: CipherAlgorithm) -> Result<Cipher> {
        let key = base64::prelude::BASE64_STANDARD.decode(s).map_err(|e| AetherError::Base64(e.to_string()))?;
        Cipher::with_key_slice_algorithm(&key, algorithm)
    }

    pub fn with_password(password: &[u8], salt: Option<Integrity>) -> Result<Cipher> {
        Cipher::with_password_algorithm(password, salt, CipherAlgorithm::default())
    }

    pub fn with_password_algorithm(
        password: &[u8],
        salt: Option<Integrity>,
        algorithm: CipherAlgorithm,
    ) -> Result<Cipher> {
        let salt = salt.unwrap_or_else(|| {
            let mut salt = [0u8; INTEGRITY_SIZE];
            OsRng.fill_bytes(&mut salt);
            salt
        });
        let argon2 = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(19 * 1024, 2, 1, Some(32)).map_err(|e| AetherError::Kdf(e.to_string()))?,
        );
        let mut key = [0u8; KEY_SIZE];
        argon2.hash_password_into(password, &salt, &mut key).map_err(|e| AetherError::Kdf(e.to_string()))?;
        Ok(Cipher::new0(&key, algorithm, Some(salt)))
    }

    pub fn encrypt<R: BufRead, W: Write>(&mut self, r: R, mut w: BufWriter<W>) -> Result<()> {
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
            let pos = read_exact_or_eof(&mut r, &mut buf)?;
            if pos == 0 {
                break;
            }
            let nonce = countered_nonce.next();
            let ciphertext = self.aead.encrypt(&nonce, &buf[..pos])?;
            w.write_all(&ciphertext)?;
        }
        Ok(())
    }

    pub fn encrypt_bytes(&mut self, bs: &[u8]) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(NONCE_SIZE + bs.len() + 16);
        let nonce = self.countered_nonce.next();
        let mut enc = self.aead.encrypt(&nonce, bs)?;
        result.append(&mut enc);
        result.extend_from_slice(nonce.as_slice());
        Ok(result)
    }

    pub fn encrypt_file_name(&mut self, s: &OsStr) -> Result<OsString> {
        let bs = s.as_bytes();
        let ciphertext = self.encrypt_bytes(bs)?;
        let b64 = base64::prelude::BASE64_URL_SAFE_NO_PAD.encode(&ciphertext);
        Ok(OsString::from(b64))
    }

    pub fn decrypt<R: BufRead, W: Write>(&mut self, mut r: R, mut w: BufWriter<W>) -> Result<()> {
        let mut header_bytes = [0u8; crate::header::HEADER_SIZE];
        r.read_exact(&mut header_bytes)?;
        let header = Header::from_bytes(&header_bytes)?;
        // Auto-detect algorithm from header flags
        let algo = CipherAlgorithm::from_flags(header.flags)?;
        let aead = AeadInner::new(algo, &self.key);
        let mut countered_nonce = CounteredNonce::new(header.iv);
        let mut tmp_old = Vec::with_capacity(BUFFER_SIZE);
        let mut tmp_new = Vec::with_capacity(BUFFER_SIZE);
        loop {
            let mut buf = [0u8; BUFFER_SIZE];
            let pos = read_exact_or_eof(&mut r, &mut buf)?;
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
            return Err(AetherError::Decryption("data too short for integrity check".into()));
        }
        let (tmp, actual_integrity) = tmp_old.split_at(tmp_old.len() - INTEGRITY_SIZE);
        if header.integrity != actual_integrity {
            return Err(AetherError::IntegrityMismatch);
        }
        w.write_all(tmp)?;
        Ok(())
    }

    pub fn decrypt_bytes(&mut self, bs: &[u8]) -> Result<Vec<u8>> {
        let mut bs = bs.to_vec();
        let nonce = bs.split_off(bs.len() - NONCE_SIZE);
        let nonce = Nonce::from_slice(&nonce);
        let plaintext = self.aead.decrypt(nonce, bs.as_slice())?;
        Ok(plaintext)
    }

    pub fn decrypt_file_name(&mut self, s: &OsStr) -> Result<OsString> {
        let ciphertext = base64::prelude::BASE64_URL_SAFE_NO_PAD
            .decode(s.as_bytes())
            .map_err(|e| AetherError::Base64(e.to_string()))?;
        let plaintext = self.decrypt_bytes(&ciphertext)?;
        Ok(OsString::from_vec(plaintext))
    }
}

fn read_exact_or_eof<R: BufRead>(r: &mut R, buf: &mut [u8]) -> Result<usize> {
    let buf_len = buf.len();
    let mut pos = 0usize;
    loop {
        let n = r.read(&mut buf[pos..])?;
        pos += n;
        if n == 0 || pos == buf_len {
            break;
        }
    }
    Ok(pos)
}

#[cfg(test)]
mod tests {
    use std::io::BufWriter;

    use super::*;
    use crate::header::INTEGRITY_SIZE;

    fn roundtrip(algo: CipherAlgorithm, plaintext: &[u8]) {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, algo);
        let mut ciphertext = Vec::new();
        let bw = BufWriter::new(&mut ciphertext);
        cipher.encrypt(plaintext, bw).unwrap();
        let mut plaintext2 = Vec::new();
        let bw = BufWriter::new(&mut plaintext2);
        cipher.decrypt(&ciphertext[..], bw).unwrap();
        assert_eq!(plaintext, &plaintext2[..]);
    }

    #[test]
    fn aes_small() {
        roundtrip(CipherAlgorithm::Aes256Gcm, b"Hello, world!");
    }

    #[test]
    fn chacha_small() {
        roundtrip(CipherAlgorithm::ChaCha20Poly1305, b"Hello, world!");
    }

    #[test]
    fn aes_large() {
        roundtrip(CipherAlgorithm::Aes256Gcm, &vec![0u8; 10240]);
    }

    #[test]
    fn chacha_large() {
        roundtrip(CipherAlgorithm::ChaCha20Poly1305, &vec![0u8; 10240]);
    }

    #[test]
    fn cross_algo_decrypt_auto_detects() {
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
    fn file_name_aes() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, CipherAlgorithm::Aes256Gcm);
        let s = "hello_world.txt";
        let ciphertext = cipher.encrypt_file_name(s.as_ref()).unwrap();
        let plaintext = cipher.decrypt_file_name(&ciphertext).unwrap();
        assert_eq!(s, plaintext.to_string_lossy());
    }

    #[test]
    fn file_name_chacha() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, CipherAlgorithm::ChaCha20Poly1305);
        let s = "hello_world.txt";
        let ciphertext = cipher.encrypt_file_name(s.as_ref()).unwrap();
        let plaintext = cipher.decrypt_file_name(&ciphertext).unwrap();
        assert_eq!(s, plaintext.to_string_lossy());
    }

    #[test]
    fn password_aes() {
        let password = b"test";
        let integrity = [42u8; INTEGRITY_SIZE];
        let mut cipher =
            Cipher::with_password_algorithm(password, Some(integrity), CipherAlgorithm::Aes256Gcm).unwrap();
        let plaintext = b"Hello, world!";
        let mut ciphertext = Vec::new();
        cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();
        cipher.decrypt(&ciphertext[..], BufWriter::new(&mut Vec::new())).unwrap();
    }

    #[test]
    fn password_chacha() {
        let password = b"test";
        let integrity = [42u8; INTEGRITY_SIZE];
        let mut cipher =
            Cipher::with_password_algorithm(password, Some(integrity), CipherAlgorithm::ChaCha20Poly1305).unwrap();
        let plaintext = b"Hello, world!";
        let mut ciphertext = Vec::new();
        cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();
        let mut result = Vec::new();
        cipher.decrypt(&ciphertext[..], BufWriter::new(&mut result)).unwrap();
        assert_eq!(plaintext.as_slice(), &result[..]);
    }

    #[test]
    fn with_key_slice_rejects_short_key() {
        let result = Cipher::with_key_slice(&[0u8; 16]);
        assert!(result.is_err());
    }

    #[test]
    fn with_key_b64_rejects_invalid() {
        let result = Cipher::with_key_b64("not-valid-base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn legacy_api() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::new(&key);
        let plaintext = b"Hello, world!";
        let mut ciphertext = Vec::new();
        cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();
        let mut plaintext2 = Vec::new();
        cipher.decrypt(&ciphertext[..], BufWriter::new(&mut plaintext2)).unwrap();
        assert_eq!(plaintext.as_slice(), &plaintext2[..]);
    }
}
