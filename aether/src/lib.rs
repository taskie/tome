use std::{
    ffi::{OsStr, OsString},
    io::{BufRead, BufWriter, Write},
    os::unix::ffi::{OsStrExt as _, OsStringExt},
};

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use argon2::Argon2;
use base64::Engine;
use bytes::{Buf, BufMut};
use crypto_bigint::rand_core::RngCore as _;
use typenum::U12;

type Integrity = [u8; INTEGRITY_SIZE];

pub struct Cipher {
    gcm: Aes256Gcm,
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
    fn new0(key: &Key<Aes256Gcm>, integrity: Option<Integrity>) -> Cipher {
        let gcm = Aes256Gcm::new(key);
        let countered_nonce = CounteredNonce::new(Aes256Gcm::generate_nonce(&mut OsRng));
        Cipher { gcm, countered_nonce, integrity }
    }

    pub fn new(key: &Key<Aes256Gcm>) -> Cipher {
        Cipher::new0(key, None)
    }

    pub fn with_key_slice(key: &[u8]) -> Cipher {
        let key = Key::<Aes256Gcm>::from_slice(key);
        Cipher::new(&key)
    }

    pub fn with_key_b64(s: &str) -> Cipher {
        let key = base64::prelude::BASE64_STANDARD.decode(s).unwrap();
        Cipher::with_key_slice(&key)
    }

    pub fn with_password(password: &[u8], salt: Option<Integrity>) -> Cipher {
        let salt = salt.unwrap_or_else(|| {
            let mut salt = [0u8; INTEGRITY_SIZE];
            OsRng.fill_bytes(&mut salt);
            salt
        });
        // Use Argon2id with a minimum configuration of 19 MiB of memory, an iteration count of 2, and 1 degree of parallelism.
        // See: https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html
        let argon2 = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(19 * 1024, 2, 1, Some(32)).unwrap(),
        );
        let mut key = [0u8; KEY_SIZE];
        argon2.hash_password_into(password, &salt, &mut key).unwrap();
        let key = Key::<Aes256Gcm>::from_slice(&key);
        Cipher::new0(key, Some(salt.clone()))
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
        let header = Header::new(&countered_nonce.peek(), integrity).to_bytes();
        w.write_all(&header)?;
        let mut r = r.chain(&integrity[..]);
        loop {
            let mut buf = [0u8; BUFFER_SIZE - 16];
            let pos = self.read_exact_or_eof(&mut r, &mut buf)?;
            if pos == 0 {
                break;
            }
            let nonce = countered_nonce.next();
            let ciphertext = self
                .gcm
                .encrypt(&nonce, &buf[..pos])
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            w.write_all(&ciphertext)?;
        }
        Ok(())
    }

    pub fn encrypt_bytes(&mut self, bs: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        let mut result = Vec::with_capacity(NONCE_SIZE + bs.len() + 16);
        let nonce = self.countered_nonce.next();
        let mut enc =
            self.gcm.encrypt(&nonce, bs).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
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
        let mut header = [0u8; HEADER_SIZE];
        r.read_exact(&mut header)?;
        let header = Header::from_bytes(&header)?;
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
            let plaintext = self
                .gcm
                .decrypt(&nonce, &buf[..pos])
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
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
        w.write_all(&tmp)?;
        Ok(())
    }

    pub fn decrypt_bytes(&mut self, bs: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        let mut bs = bs.to_vec();
        let nonce = bs.split_off(bs.len() - NONCE_SIZE);
        let nonce = Nonce::from_slice(&nonce);
        let plaintext = self
            .gcm
            .decrypt(&nonce, bs.as_slice())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        Ok(plaintext)
    }

    pub fn decrypt_file_name(&mut self, s: &OsStr) -> Result<OsString, std::io::Error> {
        let ciphertext = base64::prelude::BASE64_URL_SAFE_NO_PAD
            .decode(s.as_bytes())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let plaintext = self.decrypt_bytes(&ciphertext)?;
        Ok(OsString::from_vec(plaintext))
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
    flags: u16,
    iv: Nonce<U12>,
    pub integrity: Integrity,
}

impl Header {
    pub fn new(iv: &Nonce<U12>, integrity: Integrity) -> Header {
        Header { magic: 0xae71, flags: 0, iv: *iv, integrity }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut header = Vec::with_capacity(HEADER_SIZE);
        // 2: 0..2
        header.put_u16(self.magic);
        // 2: 2..4
        header.put_u16(self.flags);
        // 12: 4..16
        header.write_all(self.iv.as_ref()).unwrap();
        // 16: 16..32
        header.write_all(self.integrity.as_ref()).unwrap();
        assert_eq!(header.len(), HEADER_SIZE);
        header
    }

    pub fn from_bytes(bs: &[u8]) -> Result<Header, std::io::Error> {
        if bs.len() != HEADER_SIZE {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid header (len)"));
        }
        let mut header = &bs[..];
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
        let mut nonce = self.original.clone();
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

    use aes_gcm::{Aes256Gcm, Key};

    use crate::{INTEGRITY_SIZE, KEY_SIZE};

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

    #[test]
    fn test() {
        let key: &[u8] = &[42; KEY_SIZE];
        let key = Key::<Aes256Gcm>::from_slice(key);
        let mut cipher = Cipher::new(&key);
        let plaintext = b"Hello, world!";
        let mut ciphertext = Vec::new();
        let bw = BufWriter::new(&mut ciphertext);
        cipher.encrypt(&plaintext[..], bw).unwrap();
        xxd(&ciphertext, 0, ciphertext.len());
        let mut plaintext2 = Vec::new();
        let bw = BufWriter::new(&mut plaintext2);
        cipher.decrypt(&ciphertext[..], bw).unwrap();
        assert_eq!(plaintext, &plaintext2[..]);
    }

    #[test]
    fn test_large() {
        let key: &[u8] = &[42; KEY_SIZE];
        let key = Key::<Aes256Gcm>::from_slice(key);
        let mut cipher = Cipher::new(&key);
        let mut plaintext = Vec::with_capacity(8096);
        for _i in 0..10240 {
            plaintext.push(0u8);
        }
        let mut ciphertext = Vec::new();
        let bw = BufWriter::new(&mut ciphertext);
        cipher.encrypt(&plaintext[..], bw).unwrap();
        xxd(&ciphertext, 0, 32);
        println!("...");
        xxd(&ciphertext, ciphertext.len() - 32, ciphertext.len());
        let mut plaintext2 = Vec::new();
        let bw = BufWriter::new(&mut plaintext2);
        cipher.decrypt(&ciphertext[..], bw).unwrap();
        assert_eq!(plaintext, &plaintext2[..]);
    }

    #[test]
    fn test_file_name() {
        let key: &[u8] = &[42; KEY_SIZE];
        let key = Key::<Aes256Gcm>::from_slice(key);
        let mut cipher = Cipher::new(&key);
        let s = "hello_world.txt";
        let ciphertext = cipher.encrypt_file_name(s.as_ref()).unwrap();
        println!("{}", ciphertext.to_string_lossy());
        let plaintext = cipher.decrypt_file_name(&ciphertext).unwrap();
        assert_eq!(s, plaintext.to_string_lossy());
    }

    #[test]
    fn test_password() {
        let password = b"test";
        let integrity = [42u8; INTEGRITY_SIZE];
        let mut cipher = Cipher::with_password(password, Some(integrity));
        let plaintext = b"Hello, world!";
        let mut ciphertext = Vec::new();
        let bw = BufWriter::new(&mut ciphertext);
        cipher.encrypt(&plaintext[..], bw).unwrap();
        xxd(&ciphertext, 0, ciphertext.len());
        let mut plaintext2 = Vec::new();
        let bw = BufWriter::new(&mut plaintext2);
        cipher.decrypt(&ciphertext[..], bw).unwrap();
        assert_eq!(plaintext, &plaintext2[..]);
    }
}
