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
use crate::header::{
    ChunkKind, CounteredNonce, ENCRYPTED_DEK_SIZE, HEADER_SIZE, Header, HeaderFlags, INTEGRITY_SIZE, Integrity,
    KEY_SIZE, KdfParams, KeyBlock, KeySlot, NONCE_SIZE, compute_key_id,
};

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

    fn encrypt_ad(&self, nonce: &Nonce<U12>, plaintext: &[u8], ad: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::aead::{Aead, Payload};
        let payload = Payload { msg: plaintext, aad: ad };
        match self {
            Self::Aes(gcm) => gcm.encrypt(nonce, payload),
            Self::ChaCha(cc) => cc.encrypt(nonce, payload),
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

    fn decrypt_ad(&self, nonce: &Nonce<U12>, ciphertext: &[u8], ad: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::aead::{Aead, Payload};
        let payload = Payload { msg: ciphertext, aad: ad };
        match self {
            Self::Aes(gcm) => gcm.decrypt(nonce, payload),
            Self::ChaCha(cc) => cc.decrypt(nonce, payload),
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
    /// Format version for encryption (0 or 1). Decryption auto-detects from header.
    format_version: u8,
    /// Chunk kind for v1 encryption.
    chunk_kind: ChunkKind,
    /// KDF parameters for v1 envelope encryption (password mode).
    kdf_params: Option<KdfParams>,
}

impl Drop for Cipher {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

impl Cipher {
    fn new0(
        key: &[u8; KEY_SIZE],
        algorithm: CipherAlgorithm,
        integrity: Option<Integrity>,
        format_version: u8,
        chunk_kind: ChunkKind,
        kdf_params: Option<KdfParams>,
    ) -> Cipher {
        let aead = AeadInner::new(algorithm, key);
        let countered_nonce = CounteredNonce::new(Aes256Gcm::generate_nonce(&mut OsRng));
        Cipher { aead, algorithm, key: *key, countered_nonce, integrity, format_version, chunk_kind, kdf_params }
    }

    pub fn new(key: &[u8; KEY_SIZE]) -> Cipher {
        Cipher::new0(key, CipherAlgorithm::default(), None, 1, ChunkKind::DEFAULT, None)
    }

    pub fn with_algorithm(key: &[u8; KEY_SIZE], algorithm: CipherAlgorithm) -> Cipher {
        Cipher::new0(key, algorithm, None, 1, ChunkKind::DEFAULT, None)
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
        let m_cost = 19 * 1024;
        let t_cost = 2u32;
        let p_cost = 1u32;
        let argon2 = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(m_cost, t_cost, p_cost, Some(32)).map_err(|e| AetherError::Kdf(e.to_string()))?,
        );
        let mut key = [0u8; KEY_SIZE];
        argon2.hash_password_into(password, &salt, &mut key).map_err(|e| AetherError::Kdf(e.to_string()))?;
        let kdf_params = KdfParams::Argon2id { salt, m_cost, t_cost, p_cost };
        Ok(Cipher::new0(&key, algorithm, Some(salt), 1, ChunkKind::DEFAULT, Some(kdf_params)))
    }

    /// Set the format version for encryption (0 = legacy, 1 = envelope + streaming AEAD).
    pub fn set_format_version(&mut self, version: u8) {
        self.format_version = version;
    }

    /// Set the chunk kind for v1 encryption.
    pub fn set_chunk_kind(&mut self, chunk_kind: ChunkKind) {
        self.chunk_kind = chunk_kind;
    }

    // ── Streaming encrypt/decrypt ────────────────────────────────────────

    pub fn encrypt<R: BufRead, W: Write>(&mut self, r: R, w: BufWriter<W>) -> Result<()> {
        match self.format_version {
            0 => self.encrypt_v0(r, w),
            1 => self.encrypt_v1(r, w),
            v => Err(AetherError::InvalidHeader(format!("unsupported format version: {v}"))),
        }
    }

    pub fn decrypt<R: BufRead, W: Write>(&mut self, r: R, w: BufWriter<W>) -> Result<()> {
        self.decrypt_auto(r, w)
    }

    /// v0 encrypt: integrity suffix appended to plaintext, fixed 8 KiB chunks.
    fn encrypt_v0<R: BufRead, W: Write>(&mut self, r: R, mut w: BufWriter<W>) -> Result<()> {
        let buf_size = ChunkKind::V0.ciphertext_size();
        let mut countered_nonce = CounteredNonce::new(Aes256Gcm::generate_nonce(&mut OsRng));
        let integrity = if let Some(integrity) = self.integrity {
            integrity
        } else {
            let mut integrity = [0u8; INTEGRITY_SIZE];
            OsRng.fill_bytes(&mut integrity);
            integrity
        };
        let header = Header::new_v0(&countered_nonce.peek(false), integrity, self.algorithm).to_bytes();
        w.write_all(&header)?;
        let mut r = r.chain(&integrity[..]);
        let pt_size = ChunkKind::V0.plaintext_size();
        loop {
            let mut buf = vec![0u8; pt_size];
            let pos = read_exact_or_eof(&mut r, &mut buf)?;
            if pos == 0 {
                break;
            }
            let nonce = countered_nonce.next();
            let ciphertext = self.aead.encrypt(&nonce, &buf[..pos])?;
            debug_assert!(ciphertext.len() <= buf_size);
            w.write_all(&ciphertext)?;
        }
        Ok(())
    }

    /// v1 encrypt: envelope encryption (KEK/DEK) + STREAM construction.
    fn encrypt_v1<R: BufRead, W: Write>(&mut self, r: R, mut w: BufWriter<W>) -> Result<()> {
        // 1. Generate random DEK
        let mut dek = [0u8; KEY_SIZE];
        OsRng.fill_bytes(&mut dek);

        // 2. Build header (version=1, reserved integrity = zeros)
        let flags = HeaderFlags::new(1, self.chunk_kind, self.algorithm);
        let data_iv = Aes256Gcm::generate_nonce(&mut OsRng);
        let header = Header::new(&data_iv, [0u8; INTEGRITY_SIZE], flags);
        let header_bytes = header.to_bytes();
        w.write_all(&header_bytes)?;

        // 3. Build Key Block: wrap DEK with KEK
        let dek_nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let kek_aead = AeadInner::new(self.algorithm, &self.key);
        let encrypted_dek_vec = kek_aead.encrypt_ad(&dek_nonce, &dek, &header_bytes)?;
        let mut encrypted_dek = [0u8; ENCRYPTED_DEK_SIZE];
        encrypted_dek.copy_from_slice(&encrypted_dek_vec);

        let key_id = compute_key_id(&self.key);
        let kdf_params = self.kdf_params.clone().unwrap_or(KdfParams::None);
        let key_block = KeyBlock { kdf_params, dek_nonce, slots: vec![KeySlot { key_id, encrypted_dek }] };
        let key_block_bytes = key_block.to_bytes();
        w.write_all(&key_block_bytes)?;

        // 4. STREAM encrypt with DEK
        let mut first_chunk_ad = Vec::with_capacity(header_bytes.len() + key_block_bytes.len());
        first_chunk_ad.extend_from_slice(&header_bytes);
        first_chunk_ad.extend_from_slice(&key_block_bytes);

        let dek_aead = AeadInner::new(self.algorithm, &dek);
        let countered_nonce = CounteredNonce::new(data_iv);
        self.stream_encrypt(r, &mut w, &dek_aead, countered_nonce, &first_chunk_ad)?;

        dek.zeroize();
        Ok(())
    }

    /// Auto-detect version from header and dispatch to v0 or v1 decrypt.
    fn decrypt_auto<R: BufRead, W: Write>(&mut self, mut r: R, w: BufWriter<W>) -> Result<()> {
        let mut header_bytes = [0u8; HEADER_SIZE];
        r.read_exact(&mut header_bytes)?;
        let header = Header::from_bytes(&header_bytes)?;
        match header.flags.version {
            0 => self.decrypt_v0(&header, &header_bytes, r, w),
            1 => self.decrypt_v1(&header, &header_bytes, r, w),
            v => Err(AetherError::InvalidHeader(format!("unsupported format version: {v}"))),
        }
    }

    /// v0 decrypt: integrity suffix verified at end of plaintext.
    fn decrypt_v0<R: BufRead, W: Write>(
        &self,
        header: &Header,
        _header_bytes: &[u8],
        mut r: R,
        mut w: BufWriter<W>,
    ) -> Result<()> {
        let buf_size = ChunkKind::V0.ciphertext_size();
        let algo = header.flags.algorithm;
        let aead = AeadInner::new(algo, &self.key);
        let mut countered_nonce = CounteredNonce::new(header.iv);
        let mut tmp_old = Vec::with_capacity(buf_size);
        let mut tmp_new = Vec::with_capacity(buf_size);
        loop {
            let mut buf = vec![0u8; buf_size];
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

    /// v1 decrypt: read Key Block, unwrap DEK, then STREAM decrypt.
    fn decrypt_v1<R: BufRead, W: Write>(
        &self,
        header: &Header,
        header_bytes: &[u8],
        mut r: R,
        mut w: BufWriter<W>,
    ) -> Result<()> {
        let algo = header.flags.algorithm;
        let chunk_kind = header.flags.chunk_kind;

        // 1. Read Key Block
        let (key_block, key_block_bytes) = KeyBlock::from_reader(&mut r)?;

        // 2. Derive KEK if password mode
        let kek = &self.key;

        // 3. Find matching slot and unwrap DEK
        let kek_aead = AeadInner::new(algo, kek);
        let key_id = compute_key_id(kek);
        let mut dek = self.unwrap_dek(&key_block, &kek_aead, &key_id, header_bytes)?;

        // 4. STREAM decrypt with DEK
        let mut first_chunk_ad = Vec::with_capacity(header_bytes.len() + key_block_bytes.len());
        first_chunk_ad.extend_from_slice(header_bytes);
        first_chunk_ad.extend_from_slice(&key_block_bytes);

        let dek_aead = AeadInner::new(algo, &dek);
        let countered_nonce = CounteredNonce::new(header.iv);
        self.stream_decrypt(&mut r, &mut w, &dek_aead, countered_nonce, chunk_kind, &first_chunk_ad)?;

        dek.zeroize();
        Ok(())
    }

    /// Try to unwrap DEK from Key Block slots.
    /// Slots with matching key_id are tried first, then the rest.
    fn unwrap_dek(
        &self,
        key_block: &KeyBlock,
        kek_aead: &AeadInner,
        key_id: &[u8; 4],
        header_bytes: &[u8],
    ) -> Result<[u8; KEY_SIZE]> {
        let matched = key_block.slots.iter().filter(|s| s.key_id == *key_id);
        let unmatched = key_block.slots.iter().filter(|s| s.key_id != *key_id);
        for slot in matched.chain(unmatched) {
            if let Ok(dek) = self.try_unwrap_slot(slot, kek_aead, &key_block.dek_nonce, header_bytes) {
                return Ok(dek);
            }
        }
        Err(AetherError::Decryption("no matching KEK slot found".into()))
    }

    fn try_unwrap_slot(
        &self,
        slot: &KeySlot,
        kek_aead: &AeadInner,
        dek_nonce: &Nonce<U12>,
        header_bytes: &[u8],
    ) -> Result<[u8; KEY_SIZE]> {
        let dek_vec = kek_aead.decrypt_ad(dek_nonce, &slot.encrypted_dek, header_bytes)?;
        let mut dek = [0u8; KEY_SIZE];
        dek.copy_from_slice(&dek_vec);
        Ok(dek)
    }

    // ── STREAM helpers ───────────────────────────────────────────────────

    /// STREAM encrypt: variable chunk size with last-chunk nonce flag and first-chunk AD.
    fn stream_encrypt<R: BufRead, W: Write>(
        &self,
        mut r: R,
        w: &mut W,
        aead: &AeadInner,
        mut countered_nonce: CounteredNonce,
        first_chunk_ad: &[u8],
    ) -> Result<()> {
        let pt_size = self.chunk_kind.plaintext_size();
        let mut buf = vec![0u8; pt_size];
        let mut pending_pos = read_exact_or_eof(&mut r, &mut buf)?;
        let mut is_first = true;

        loop {
            let mut next_buf = vec![0u8; pt_size];
            let next_pos = read_exact_or_eof(&mut r, &mut next_buf)?;
            let is_last = next_pos == 0;

            let nonce = if is_last { countered_nonce.next_last() } else { countered_nonce.next() };
            let ciphertext = if is_first {
                is_first = false;
                aead.encrypt_ad(&nonce, &buf[..pending_pos], first_chunk_ad)?
            } else {
                aead.encrypt(&nonce, &buf[..pending_pos])?
            };
            w.write_all(&ciphertext)?;

            if is_last {
                break;
            }
            buf.copy_from_slice(&next_buf);
            pending_pos = next_pos;
        }
        Ok(())
    }

    /// STREAM decrypt: variable chunk size with last-chunk detection and first-chunk AD.
    fn stream_decrypt<R: BufRead, W: Write>(
        &self,
        r: &mut R,
        w: &mut W,
        aead: &AeadInner,
        mut countered_nonce: CounteredNonce,
        chunk_kind: ChunkKind,
        first_chunk_ad: &[u8],
    ) -> Result<()> {
        let buf_size = chunk_kind.ciphertext_size();
        let mut is_first = true;
        let mut seen_last = false;

        loop {
            let mut buf = vec![0u8; buf_size];
            let pos = read_exact_or_eof(r, &mut buf)?;
            if pos == 0 {
                if !seen_last {
                    return Err(AetherError::Decryption("stream truncated: no last chunk".into()));
                }
                break;
            }

            let normal_nonce = countered_nonce.peek(false);
            let last_nonce = countered_nonce.peek(true);
            countered_nonce.counter += 1;

            let plaintext = if is_first {
                is_first = false;
                if let Ok(pt) = aead.decrypt_ad(&normal_nonce, &buf[..pos], first_chunk_ad) {
                    pt
                } else {
                    seen_last = true;
                    aead.decrypt_ad(&last_nonce, &buf[..pos], first_chunk_ad)?
                }
            } else if let Ok(pt) = aead.decrypt(&normal_nonce, &buf[..pos]) {
                pt
            } else {
                seen_last = true;
                aead.decrypt(&last_nonce, &buf[..pos])?
            };
            w.write_all(&plaintext)?;

            if seen_last {
                let mut trail = [0u8; 1];
                let n = r.read(&mut trail)?;
                if n > 0 {
                    return Err(AetherError::Decryption("data after last chunk".into()));
                }
                break;
            }
        }
        Ok(())
    }

    // ── Byte-level encrypt/decrypt (file names, not streaming) ───────────

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
        // v1 default (envelope encryption)
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, algo);
        let mut ciphertext = Vec::new();
        cipher.encrypt(plaintext, BufWriter::new(&mut ciphertext)).unwrap();
        let mut plaintext2 = Vec::new();
        cipher.decrypt(&ciphertext[..], BufWriter::new(&mut plaintext2)).unwrap();
        assert_eq!(plaintext, &plaintext2[..]);
    }

    fn roundtrip_v0(algo: CipherAlgorithm, plaintext: &[u8]) {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, algo);
        cipher.set_format_version(0);
        let mut ciphertext = Vec::new();
        cipher.encrypt(plaintext, BufWriter::new(&mut ciphertext)).unwrap();
        let mut plaintext2 = Vec::new();
        cipher.decrypt(&ciphertext[..], BufWriter::new(&mut plaintext2)).unwrap();
        assert_eq!(plaintext, &plaintext2[..]);
    }

    // ── v1 tests (envelope + STREAM) ─────────────────────────────────────

    #[test]
    fn v1_aes_small() {
        roundtrip(CipherAlgorithm::Aes256Gcm, b"Hello, world!");
    }

    #[test]
    fn v1_chacha_small() {
        roundtrip(CipherAlgorithm::ChaCha20Poly1305, b"Hello, world!");
    }

    #[test]
    fn v1_aes_large() {
        roundtrip(CipherAlgorithm::Aes256Gcm, &vec![0u8; 10240]);
    }

    #[test]
    fn v1_chacha_large() {
        roundtrip(CipherAlgorithm::ChaCha20Poly1305, &vec![0u8; 10240]);
    }

    #[test]
    fn v1_empty() {
        roundtrip(CipherAlgorithm::Aes256Gcm, b"");
    }

    #[test]
    fn v1_exact_chunk_boundary() {
        let pt = vec![0xABu8; ChunkKind::DEFAULT.plaintext_size()];
        roundtrip(CipherAlgorithm::Aes256Gcm, &pt);
    }

    #[test]
    fn v1_custom_chunk_kind() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, CipherAlgorithm::Aes256Gcm);
        cipher.set_chunk_kind(ChunkKind::new(3).unwrap()); // 64 KiB
        let plaintext = vec![0xCDu8; 200_000]; // ~3 chunks
        let mut ciphertext = Vec::new();
        cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();
        let mut result = Vec::new();
        cipher.decrypt(&ciphertext[..], BufWriter::new(&mut result)).unwrap();
        assert_eq!(plaintext, result);
    }

    #[test]
    fn v1_cross_algo_decrypt_auto_detects() {
        let key = [42u8; KEY_SIZE];
        let mut cipher_aes = Cipher::with_algorithm(&key, CipherAlgorithm::Aes256Gcm);
        let plaintext = b"cross-algo test";
        let mut ciphertext = Vec::new();
        cipher_aes.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();

        let mut cipher_chacha = Cipher::with_algorithm(&key, CipherAlgorithm::ChaCha20Poly1305);
        let mut result = Vec::new();
        cipher_chacha.decrypt(&ciphertext[..], BufWriter::new(&mut result)).unwrap();
        assert_eq!(plaintext.as_slice(), &result[..]);
    }

    #[test]
    fn v1_truncation_detected() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, CipherAlgorithm::Aes256Gcm);
        cipher.set_chunk_kind(ChunkKind::V0); // 8 KiB for smaller ciphertext
        let plaintext = vec![0u8; 20_000]; // ~3 chunks
        let mut ciphertext = Vec::new();
        cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();

        // Truncate to remove last chunk
        let truncated = &ciphertext[..ciphertext.len() - 100];
        let mut result = Vec::new();
        let err = cipher.decrypt(truncated, BufWriter::new(&mut result));
        assert!(err.is_err());
    }

    #[test]
    fn v1_header_tampering_detected() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, CipherAlgorithm::Aes256Gcm);
        let plaintext = b"tamper test";
        let mut ciphertext = Vec::new();
        cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();

        // Tamper with IV byte in the header
        ciphertext[5] ^= 0x01;
        let mut result = Vec::new();
        let err = cipher.decrypt(&ciphertext[..], BufWriter::new(&mut result));
        assert!(err.is_err());
    }

    #[test]
    fn v1_key_block_tampering_detected() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, CipherAlgorithm::Aes256Gcm);
        let plaintext = b"key block tamper test";
        let mut ciphertext = Vec::new();
        cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();

        // Tamper with a byte inside the key block (after 32-byte header)
        ciphertext[HEADER_SIZE + 10] ^= 0x01;
        let mut result = Vec::new();
        let err = cipher.decrypt(&ciphertext[..], BufWriter::new(&mut result));
        assert!(err.is_err());
    }

    #[test]
    fn v1_wrong_key_rejected() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, CipherAlgorithm::Aes256Gcm);
        let plaintext = b"wrong key test";
        let mut ciphertext = Vec::new();
        cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();

        let wrong_key = [99u8; KEY_SIZE];
        let mut cipher2 = Cipher::with_algorithm(&wrong_key, CipherAlgorithm::Aes256Gcm);
        let mut result = Vec::new();
        let err = cipher2.decrypt(&ciphertext[..], BufWriter::new(&mut result));
        assert!(err.is_err());
    }

    // ── v0 backward compat tests ─────────────────────────────────────────

    #[test]
    fn v0_aes_small() {
        roundtrip_v0(CipherAlgorithm::Aes256Gcm, b"Hello, world!");
    }

    #[test]
    fn v0_chacha_small() {
        roundtrip_v0(CipherAlgorithm::ChaCha20Poly1305, b"Hello, world!");
    }

    #[test]
    fn v0_aes_large() {
        roundtrip_v0(CipherAlgorithm::Aes256Gcm, &vec![0u8; 10240]);
    }

    #[test]
    fn v0_chacha_large() {
        roundtrip_v0(CipherAlgorithm::ChaCha20Poly1305, &vec![0u8; 10240]);
    }

    // ── file name tests (version-independent) ────────────────────────────

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

    // ── password tests ───────────────────────────────────────────────────

    #[test]
    fn password_aes() {
        let password = b"test";
        let integrity = [42u8; INTEGRITY_SIZE];
        let mut cipher =
            Cipher::with_password_algorithm(password, Some(integrity), CipherAlgorithm::Aes256Gcm).unwrap();
        let plaintext = b"Hello, world!";
        let mut ciphertext = Vec::new();
        cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();
        let mut result = Vec::new();
        cipher.decrypt(&ciphertext[..], BufWriter::new(&mut result)).unwrap();
        assert_eq!(plaintext.as_slice(), &result[..]);
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

    // ── error path tests ─────────────────────────────────────────────────

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
}
