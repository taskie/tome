use std::{
    ffi::{OsStr, OsString},
    io::{BufRead, BufWriter, Write},
    os::unix::ffi::{OsStrExt as _, OsStringExt},
};

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::Engine;
use bytes::{Buf, BufMut};
use crypto_bigint::rand_core::RngCore as _;
use typenum::U12;

pub struct Cipher {
    gcm: Aes256Gcm,
    countered_nonce: CounteredNonce,
}

pub const KEY_SIZE: usize = 32;
const BUFSIZE: usize = 8192;
const NONCE_SIZE: usize = 12;
const INTEGRITY_SIZE: usize = 8;

impl Cipher {
    pub fn new(key: &Key<Aes256Gcm>) -> Cipher {
        let gcm = Aes256Gcm::new(key);
        let countered_nonce = CounteredNonce::new(Aes256Gcm::generate_nonce(&mut OsRng));
        Cipher { gcm, countered_nonce }
    }

    pub fn with_key_slice(key: &[u8]) -> Cipher {
        let key = Key::<Aes256Gcm>::from_slice(key);
        Cipher::new(&key)
    }

    pub fn with_key_b64(s: &str) -> Cipher {
        let key = base64::prelude::BASE64_STANDARD.decode(s).unwrap();
        Cipher::with_key_slice(&key)
    }

    pub fn encrypt<R: BufRead, W: Write>(&mut self, r: R, mut w: BufWriter<W>) -> Result<(), std::io::Error> {
        let mut countered_nonce = CounteredNonce::new(Aes256Gcm::generate_nonce(&mut OsRng));
        let integrity = OsRng.next_u64();
        let header = self.make_header(&countered_nonce.peek(), integrity);
        let integrity = integrity.to_be_bytes();
        w.write_all(&header)?;
        let mut r = r.chain(&integrity[..]);
        loop {
            let mut buf = [0u8; BUFSIZE - 16];
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
        let mut header = [0u8; 24];
        r.read_exact(&mut header)?;
        let (iv, expected_integrity) = self.load_header(&header)?;
        let mut countered_nonce = CounteredNonce::new(iv);
        let mut tmp_old = Vec::with_capacity(BUFSIZE);
        let mut tmp_new = Vec::with_capacity(BUFSIZE);
        loop {
            let mut buf = [0u8; BUFSIZE];
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
        if expected_integrity != u64::from_be_bytes(actual_integrity.try_into().unwrap()) {
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
            if n == 0 || pos == BUFSIZE {
                break;
            }
        }
        Ok(pos)
    }

    fn make_header(&self, iv: &Nonce<U12>, integrity: u64) -> Vec<u8> {
        let mut header = Vec::new();
        // 2: 0..2
        header.put_u16(0xae71);
        // 2: 2..4
        header.put_u16(0x0000);
        // 12: 4..16
        header.write_all(&iv).unwrap();
        // 8: 16..24
        header.put_u64(integrity);
        header
    }

    fn load_header(&self, header: &[u8]) -> Result<(Nonce<U12>, u64), std::io::Error> {
        let mut header = &header[..];
        let magic = header.get_u16();
        if magic != 0xae71 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid header"));
        }
        let _unused = header.get_u16();
        let mut iv = Nonce::default();
        iv.as_mut_slice().copy_from_slice(&header[..NONCE_SIZE]);
        header.advance(NONCE_SIZE);
        let integrity = header.get_u64();
        Ok((iv, integrity))
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
            xs[i + NONCE_SIZE - INTEGRITY_SIZE] ^= ys[i];
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

    use crate::KEY_SIZE;

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
        xxd(&ciphertext, 0, 32);
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
}
