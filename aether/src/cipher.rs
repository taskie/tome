use std::{
    ffi::{OsStr, OsString},
    io::{BufRead, BufWriter, Write},
    os::unix::ffi::{OsStrExt as _, OsStringExt},
};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{KeyInit, OsRng},
};
use argon2::Argon2;
use base64::Engine;
use chacha20poly1305::{ChaCha20Poly1305, XChaCha20Poly1305};
use crypto_bigint::rand_core::RngCore as _;
use typenum::{U12, U24};
use zeroize::Zeroize;

use crate::algorithm::CipherAlgorithm;
use crate::error::{AetherError, Result};
use crate::header::{
    ChunkKind, CounteredNonce, ENCRYPTED_DEK_SIZE, HEADER_SIZE, Header, HeaderFlags, INTEGRITY_SIZE, Integrity,
    KEY_SIZE, KdfParams, KeyBlock, KeySlot, MAX_NONCE_SIZE, NONCE_SIZE_STD, compute_key_id,
};

// ──────────────────────────────────────────────────────────────────────────────
// AEAD enum dispatch
// ──────────────────────────────────────────────────────────────────────────────

enum AeadInner {
    Aes(Box<Aes256Gcm>),
    ChaCha(ChaCha20Poly1305),
    XChaCha(XChaCha20Poly1305),
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
            CipherAlgorithm::XChaCha20Poly1305 => {
                Self::XChaCha(XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(key)))
            }
        }
    }

    fn nonce_size(&self) -> usize {
        match self {
            Self::Aes(_) | Self::ChaCha(_) => 12,
            Self::XChaCha(_) => 24,
        }
    }

    /// Encrypt `buf` in place (appends 16-byte AEAD tag). Avoids intermediate `Vec` allocation.
    fn encrypt_in_place(&self, nonce: &[u8; MAX_NONCE_SIZE], buf: &mut Vec<u8>, ad: &[u8]) -> Result<()> {
        use aes_gcm::aead::AeadInPlace as _;
        match self {
            Self::Aes(gcm) => gcm.encrypt_in_place(Nonce::<U12>::from_slice(&nonce[..12]), ad, buf),
            Self::ChaCha(cc) => cc.encrypt_in_place(Nonce::<U12>::from_slice(&nonce[..12]), ad, buf),
            Self::XChaCha(xcc) => xcc.encrypt_in_place(Nonce::<U24>::from_slice(nonce), ad, buf),
        }
        .map_err(|e| AetherError::Encryption(e.to_string()))
    }

    /// Decrypt `buf` in place (verifies and removes 16-byte AEAD tag).
    fn decrypt_in_place(&self, nonce: &[u8; MAX_NONCE_SIZE], buf: &mut Vec<u8>, ad: &[u8]) -> Result<()> {
        use aes_gcm::aead::AeadInPlace as _;
        match self {
            Self::Aes(gcm) => gcm.decrypt_in_place(Nonce::<U12>::from_slice(&nonce[..12]), ad, buf),
            Self::ChaCha(cc) => cc.decrypt_in_place(Nonce::<U12>::from_slice(&nonce[..12]), ad, buf),
            Self::XChaCha(xcc) => xcc.decrypt_in_place(Nonce::<U24>::from_slice(nonce), ad, buf),
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
        let nonce_size = algorithm.nonce_size();
        let mut nonce_bytes = [0u8; MAX_NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes[..nonce_size]);
        let countered_nonce = CounteredNonce::new(&nonce_bytes[..nonce_size]);
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
    /// Only supports 12-byte nonce algorithms (AES-256-GCM, ChaCha20-Poly1305).
    fn encrypt_v0<R: BufRead, W: Write>(&mut self, r: R, mut w: BufWriter<W>) -> Result<()> {
        if self.algorithm.nonce_size() != NONCE_SIZE_STD {
            return Err(AetherError::InvalidHeader(format!("v0 format does not support {}", self.algorithm)));
        }
        let mut iv = [0u8; NONCE_SIZE_STD];
        OsRng.fill_bytes(&mut iv);
        let mut countered_nonce = CounteredNonce::new(&iv);
        let integrity = if let Some(integrity) = self.integrity {
            integrity
        } else {
            let mut integrity = [0u8; INTEGRITY_SIZE];
            OsRng.fill_bytes(&mut integrity);
            integrity
        };
        let header = Header::new_v0(&iv, integrity, self.algorithm).to_bytes();
        w.write_all(&header)?;
        let mut r = r.chain(&integrity[..]);
        let pt_size = ChunkKind::V0.plaintext_size();
        let ct_size = ChunkKind::V0.ciphertext_size();
        let mut read_buf = vec![0u8; pt_size];
        let mut work = Vec::with_capacity(ct_size);
        loop {
            let pos = read_exact_or_eof(&mut r, &mut read_buf)?;
            if pos == 0 {
                break;
            }
            let nonce = countered_nonce.next();
            work.clear();
            work.extend_from_slice(&read_buf[..pos]);
            self.aead.encrypt_in_place(&nonce, &mut work, &[])?;
            w.write_all(&work)?;
        }
        Ok(())
    }

    /// v1 encrypt: envelope encryption (KEK/DEK) + STREAM construction.
    fn encrypt_v1<R: BufRead, W: Write>(&mut self, r: R, mut w: BufWriter<W>) -> Result<()> {
        let nonce_size = self.algorithm.nonce_size();

        // 1. Generate random DEK
        let mut dek = [0u8; KEY_SIZE];
        OsRng.fill_bytes(&mut dek);

        // 2. Build header (version=1, reserved payload tail = zeros)
        let flags = HeaderFlags::new(1, self.chunk_kind, self.algorithm);
        let mut data_iv = [0u8; MAX_NONCE_SIZE];
        OsRng.fill_bytes(&mut data_iv[..nonce_size]);
        let header = Header::new_v1(&data_iv[..nonce_size], flags);
        let header_bytes = header.to_bytes();
        w.write_all(&header_bytes)?;

        // 3. Build Key Block: wrap DEK with KEK (in-place)
        let mut dek_nonce = [0u8; MAX_NONCE_SIZE];
        OsRng.fill_bytes(&mut dek_nonce[..nonce_size]);
        let kek_aead = AeadInner::new(self.algorithm, &self.key);
        let mut encrypted_dek_buf = dek.to_vec();
        kek_aead.encrypt_in_place(&dek_nonce, &mut encrypted_dek_buf, &header_bytes)?;
        let mut encrypted_dek = [0u8; ENCRYPTED_DEK_SIZE];
        encrypted_dek.copy_from_slice(&encrypted_dek_buf);

        let key_id = compute_key_id(&self.key);
        let kdf_params = self.kdf_params.clone().unwrap_or(KdfParams::None);
        let key_block = KeyBlock { kdf_params, dek_nonce, nonce_size, slots: vec![KeySlot { key_id, encrypted_dek }] };
        let key_block_bytes = key_block.to_bytes();
        w.write_all(&key_block_bytes)?;

        // 4. STREAM encrypt with DEK
        let mut first_chunk_ad = Vec::with_capacity(header_bytes.len() + key_block_bytes.len());
        first_chunk_ad.extend_from_slice(&header_bytes);
        first_chunk_ad.extend_from_slice(&key_block_bytes);

        let dek_aead = AeadInner::new(self.algorithm, &dek);
        let countered_nonce = CounteredNonce::new(&data_iv[..nonce_size]);
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
        let ct_size = ChunkKind::V0.ciphertext_size();
        let algo = header.flags.algorithm;
        let aead = AeadInner::new(algo, &self.key);
        let mut countered_nonce = CounteredNonce::new(header.nonce_bytes());
        let mut read_buf = vec![0u8; ct_size];
        let mut work = Vec::with_capacity(ct_size);
        let mut delayed = Vec::new();
        loop {
            let pos = read_exact_or_eof(&mut r, &mut read_buf)?;
            if pos == 0 {
                break;
            }
            let nonce = countered_nonce.next();
            work.clear();
            work.extend_from_slice(&read_buf[..pos]);
            aead.decrypt_in_place(&nonce, &mut work, &[])?;
            if !delayed.is_empty() {
                w.write_all(&delayed)?;
            }
            std::mem::swap(&mut delayed, &mut work);
        }
        if delayed.len() < INTEGRITY_SIZE {
            return Err(AetherError::Decryption("data too short for integrity check".into()));
        }
        let (data, actual_integrity) = delayed.split_at(delayed.len() - INTEGRITY_SIZE);
        if header.integrity() != actual_integrity {
            return Err(AetherError::IntegrityMismatch);
        }
        w.write_all(data)?;
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
        let nonce_size = algo.nonce_size();
        let chunk_kind = header.flags.chunk_kind;

        // 1. Read Key Block
        let (key_block, key_block_bytes) = KeyBlock::from_reader(&mut r, nonce_size)?;

        // 2. Find matching slot and unwrap DEK
        let kek_aead = AeadInner::new(algo, &self.key);
        let key_id = compute_key_id(&self.key);
        let mut dek = self.unwrap_dek(&key_block, &kek_aead, &key_id, header_bytes)?;

        // 3. STREAM decrypt with DEK
        let mut first_chunk_ad = Vec::with_capacity(header_bytes.len() + key_block_bytes.len());
        first_chunk_ad.extend_from_slice(header_bytes);
        first_chunk_ad.extend_from_slice(&key_block_bytes);

        let dek_aead = AeadInner::new(algo, &dek);
        let countered_nonce = CounteredNonce::new(header.nonce_bytes());
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
        dek_nonce: &[u8; MAX_NONCE_SIZE],
        header_bytes: &[u8],
    ) -> Result<[u8; KEY_SIZE]> {
        let mut buf = slot.encrypted_dek.to_vec();
        kek_aead.decrypt_in_place(dek_nonce, &mut buf, header_bytes)?;
        let mut dek = [0u8; KEY_SIZE];
        dek.copy_from_slice(&buf);
        Ok(dek)
    }

    // ── STREAM helpers ───────────────────────────────────────────────────

    /// STREAM encrypt: variable chunk size with last-chunk nonce flag and first-chunk AD.
    ///
    /// Uses `BufRead::fill_buf` for EOF detection instead of a two-buffer lookahead,
    /// and reuses a single work buffer for in-place AEAD to avoid per-chunk allocations.
    fn stream_encrypt<R: BufRead, W: Write>(
        &self,
        mut r: R,
        w: &mut W,
        aead: &AeadInner,
        mut countered_nonce: CounteredNonce,
        first_chunk_ad: &[u8],
    ) -> Result<()> {
        let pt_size = self.chunk_kind.plaintext_size();
        let ct_size = self.chunk_kind.ciphertext_size();
        let mut read_buf = vec![0u8; pt_size];
        let mut work = Vec::with_capacity(ct_size);
        let mut is_first = true;

        loop {
            let pos = read_exact_or_eof(&mut r, &mut read_buf)?;
            if pos == 0 && !is_first {
                break;
            }
            let is_last = pos < pt_size || r.fill_buf()?.is_empty();
            let nonce = if is_last { countered_nonce.next_last() } else { countered_nonce.next() };
            let ad = if is_first {
                is_first = false;
                first_chunk_ad
            } else {
                &[]
            };

            work.clear();
            work.extend_from_slice(&read_buf[..pos]);
            aead.encrypt_in_place(&nonce, &mut work, ad)?;
            w.write_all(&work)?;

            if is_last {
                break;
            }
        }
        Ok(())
    }

    /// STREAM decrypt: variable chunk size with last-chunk detection and first-chunk AD.
    ///
    /// Reuses read and work buffers across iterations. Short chunks (pos < ct_size)
    /// are known to be last, skipping the double-AEAD trial.
    fn stream_decrypt<R: BufRead, W: Write>(
        &self,
        r: &mut R,
        w: &mut W,
        aead: &AeadInner,
        mut countered_nonce: CounteredNonce,
        chunk_kind: ChunkKind,
        first_chunk_ad: &[u8],
    ) -> Result<()> {
        let ct_size = chunk_kind.ciphertext_size();
        let mut read_buf = vec![0u8; ct_size];
        let mut work = Vec::with_capacity(ct_size);
        let mut is_first = true;
        let mut seen_last = false;

        loop {
            let pos = read_exact_or_eof(r, &mut read_buf)?;
            if pos == 0 {
                if !seen_last {
                    return Err(AetherError::Decryption("stream truncated: no last chunk".into()));
                }
                break;
            }

            let ad = if is_first {
                is_first = false;
                first_chunk_ad
            } else {
                &[]
            };

            // Short chunk must be the last chunk — skip the normal-nonce trial.
            if pos < ct_size {
                let nonce = countered_nonce.peek(true);
                countered_nonce.counter += 1;
                work.clear();
                work.extend_from_slice(&read_buf[..pos]);
                aead.decrypt_in_place(&nonce, &mut work, ad)?;
                w.write_all(&work)?;
                seen_last = true;
            } else {
                // Full-size chunk: try normal nonce first.
                let normal_nonce = countered_nonce.peek(false);
                let last_nonce = countered_nonce.peek(true);
                countered_nonce.counter += 1;

                work.clear();
                work.extend_from_slice(&read_buf[..pos]);
                if aead.decrypt_in_place(&normal_nonce, &mut work, ad).is_ok() {
                    w.write_all(&work)?;
                } else {
                    // Retry with last nonce — restore ciphertext from read_buf.
                    work.clear();
                    work.extend_from_slice(&read_buf[..pos]);
                    aead.decrypt_in_place(&last_nonce, &mut work, ad)?;
                    w.write_all(&work)?;
                    seen_last = true;
                }
            }

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
        let nonce_size = self.aead.nonce_size();
        let nonce = self.countered_nonce.next();
        let mut buf = bs.to_vec();
        self.aead.encrypt_in_place(&nonce, &mut buf, &[])?;
        buf.extend_from_slice(&nonce[..nonce_size]);
        Ok(buf)
    }

    pub fn encrypt_file_name(&mut self, s: &OsStr) -> Result<OsString> {
        let bs = s.as_bytes();
        let ciphertext = self.encrypt_bytes(bs)?;
        let b64 = base64::prelude::BASE64_URL_SAFE_NO_PAD.encode(&ciphertext);
        Ok(OsString::from(b64))
    }

    pub fn decrypt_bytes(&mut self, bs: &[u8]) -> Result<Vec<u8>> {
        let nonce_size = self.aead.nonce_size();
        if bs.len() < nonce_size {
            return Err(AetherError::Decryption("data too short for nonce".into()));
        }
        let (ciphertext, nonce_bytes) = bs.split_at(bs.len() - nonce_size);
        let mut nonce = [0u8; MAX_NONCE_SIZE];
        nonce[..nonce_size].copy_from_slice(nonce_bytes);
        let mut buf = ciphertext.to_vec();
        self.aead.decrypt_in_place(&nonce, &mut buf, &[])?;
        Ok(buf)
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
    fn v1_xchacha_small() {
        roundtrip(CipherAlgorithm::XChaCha20Poly1305, b"Hello, world!");
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
    fn v1_xchacha_large() {
        roundtrip(CipherAlgorithm::XChaCha20Poly1305, &vec![0u8; 10240]);
    }

    #[test]
    fn v1_empty() {
        roundtrip(CipherAlgorithm::Aes256Gcm, b"");
    }

    #[test]
    fn v1_xchacha_empty() {
        roundtrip(CipherAlgorithm::XChaCha20Poly1305, b"");
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
    fn v1_xchacha_custom_chunk_kind() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, CipherAlgorithm::XChaCha20Poly1305);
        cipher.set_chunk_kind(ChunkKind::new(3).unwrap()); // 64 KiB
        let plaintext = vec![0xCDu8; 200_000];
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

        // Decrypt with a ChaCha-configured cipher — should auto-detect AES from header
        let mut cipher_chacha = Cipher::with_algorithm(&key, CipherAlgorithm::ChaCha20Poly1305);
        let mut result = Vec::new();
        cipher_chacha.decrypt(&ciphertext[..], BufWriter::new(&mut result)).unwrap();
        assert_eq!(plaintext.as_slice(), &result[..]);
    }

    #[test]
    fn v1_xchacha_cross_algo_decrypt() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, CipherAlgorithm::XChaCha20Poly1305);
        let plaintext = b"xchacha cross-algo";
        let mut ciphertext = Vec::new();
        cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext)).unwrap();

        // Decrypt with AES-configured cipher — should auto-detect XChaCha20 from header
        let mut cipher2 = Cipher::with_algorithm(&key, CipherAlgorithm::Aes256Gcm);
        let mut result = Vec::new();
        cipher2.decrypt(&ciphertext[..], BufWriter::new(&mut result)).unwrap();
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

    #[test]
    fn v0_xchacha_rejected() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, CipherAlgorithm::XChaCha20Poly1305);
        cipher.set_format_version(0);
        let plaintext = b"should fail";
        let mut ciphertext = Vec::new();
        let err = cipher.encrypt(&plaintext[..], BufWriter::new(&mut ciphertext));
        assert!(err.is_err());
    }

    // ── file name tests ──────────────────────────────────────────────────

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
    fn file_name_xchacha() {
        let key = [42u8; KEY_SIZE];
        let mut cipher = Cipher::with_algorithm(&key, CipherAlgorithm::XChaCha20Poly1305);
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

    #[test]
    fn password_xchacha() {
        let password = b"test";
        let integrity = [42u8; INTEGRITY_SIZE];
        let mut cipher =
            Cipher::with_password_algorithm(password, Some(integrity), CipherAlgorithm::XChaCha20Poly1305).unwrap();
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
