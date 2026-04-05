use std::{
    env,
    ffi::OsStr,
    fs::{File, metadata, set_permissions},
    io::{BufRead, BufWriter, Read, Write},
    os::unix::{ffi::OsStrExt, fs::PermissionsExt as _},
    path::{Path, PathBuf},
    process::exit,
};

use aether::{Cipher, KdfParams, KeyBlock};
use clap::Parser;
use tempfile::NamedTempFile;
use tracing::error;

#[derive(Debug, Parser)]
#[command(
    name = "aether",
    about = "Authenticated encryption tool (XChaCha20-Poly1305 / ChaCha20-Poly1305 / AES-256-GCM)",
    long_about = "Encrypt and decrypt files using authenticated encryption with envelope key wrapping (KEK/DEK).\n\n\
        A key must be provided via one of: --key-file, --key-env, --password-env, or --password-prompt.\n\
        Without --output or --stdout, encrypting FILE produces FILE.aet; decrypting FILE.aet produces FILE.",
    after_help = "EXAMPLES:\n  \
        aether -k secret.key plaintext.txt          Encrypt to plaintext.txt.aet\n  \
        aether -dk secret.key plaintext.txt.aet     Decrypt to plaintext.txt\n  \
        aether -k secret.key -o out.enc input.bin   Encrypt to explicit output path\n  \
        aether -p input.txt                         Encrypt with interactive password\n  \
        aether -dp input.txt.aet                    Decrypt with interactive password\n  \
        echo data | aether -ck secret.key           Encrypt stdin to stdout\n  \
        aether -K KEY_VAR --cipher aes256gcm file   Encrypt with AES-256-GCM\n  \
        aether -i encrypted.aet                     Show file structure metadata"
)]
pub struct Opt {
    /// Write output to stdout instead of a file
    #[arg(short = 'c', long, conflicts_with = "info")]
    pub stdout: bool,

    /// Decrypt (default is encrypt)
    #[arg(short, long, conflicts_with = "info")]
    pub decrypt: bool,

    /// Display encrypted file structure metadata (no key required)
    #[arg(short, long, conflicts_with_all = ["decrypt", "stdout", "output"])]
    pub info: bool,

    /// Output file path (default: INPUT.aet for encrypt, INPUT without .aet for decrypt)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Prompt for a password interactively (confirms on encrypt)
    #[arg(short, long)]
    pub password_prompt: bool,

    /// Read password from the named environment variable
    #[arg(short = 'P', long, value_name = "VAR")]
    pub password_env: Option<String>,

    /// Path to a 32-byte binary key file ("-" for stdin)
    #[arg(short, long, env = "AETHER_KEY_FILE")]
    pub key_file: Option<PathBuf>,

    /// Read base64-encoded key from the named environment variable
    #[arg(short = 'K', long, value_name = "VAR")]
    pub key_env: Option<String>,

    /// AEAD algorithm: aes256gcm, chacha20-poly1305, or xchacha20-poly1305
    #[arg(long, default_value = "xchacha20-poly1305")]
    pub cipher: String,

    /// Format version: 0 (legacy) or 1 (envelope + streaming AEAD)
    #[arg(long, default_value = "1")]
    pub format_version: u8,

    /// Chunk size selector for v1: ciphertext = 8 KiB × 2^N (0=8K, 3=64K, 7=1M, 13=64M)
    #[arg(long, default_value = "7")]
    pub chunk_kind: u8,

    /// Enable zstd per-chunk adaptive compression (v1 only)
    #[arg(long)]
    pub compress: bool,

    /// Input file ("-" or omit for stdin)
    #[arg(value_name = "FILE")]
    pub input: Option<PathBuf>,
}

impl Opt {
    fn key_file_is_stdin(&self) -> bool {
        self.key_file.as_ref().map(|p| Self::path_is_stdin(p)).unwrap_or_default()
    }

    fn input_is_stdin(&self) -> bool {
        self.input.as_ref().map(|p| Self::path_is_stdin(p)).unwrap_or_default()
    }

    fn path_is_stdin(p: &Path) -> bool {
        p.to_string_lossy() == "-"
    }
}

fn load_key<R: Read>(mut r: R) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut buf = Vec::with_capacity(aether::KEY_SIZE);
    r.read_to_end(&mut buf)?;
    Ok(buf)
}

fn execute<R: BufRead, W: Write>(
    cipher: &mut Cipher,
    r: R,
    w: BufWriter<W>,
    opt: &Opt,
) -> Result<(), Box<dyn std::error::Error>> {
    if opt.decrypt {
        cipher.decrypt(r, w)?;
    } else {
        cipher.encrypt(r, w)?;
    }
    Ok(())
}

fn process<R: BufRead, W: Write>(mut r: R, w: BufWriter<W>, opt: &Opt) -> Result<(), Box<dyn std::error::Error>> {
    let algo: aether::CipherAlgorithm =
        opt.cipher.parse().map_err(|e: String| -> Box<dyn std::error::Error> { e.into() })?;
    let chunk_kind = aether::ChunkKind::new(opt.chunk_kind)?;
    let mut cipher = if let Some(key_file) = opt.key_file.as_ref() {
        let key = if Opt::path_is_stdin(key_file) {
            load_key(std::io::stdin().lock())?
        } else {
            let key_file = File::open(key_file)?;
            load_key(key_file)?
        };
        Cipher::with_key_slice_algorithm(&key, algo)?
    } else if let Some(key) = opt.key_env.as_ref().and_then(|name| env::var(name).ok()) {
        Cipher::with_key_b64_algorithm(&key, algo)?
    } else if let Some(password) = opt.password_env.as_ref().and_then(|name| env::var(name).ok()) {
        if opt.decrypt {
            let (header, kdf_params, consumed) = aether::read_kdf_params(&mut r)?;
            let salt = match (&header.flags.version, &kdf_params) {
                (0, _) => header.integrity(),
                (_, KdfParams::Argon2id { salt, .. }) => *salt,
                _ => return Err("encrypted file has no KDF params for password-based decryption".into()),
            };
            let mut cipher = Cipher::with_password_algorithm(password.as_bytes(), Some(salt), algo)?;
            let mut r = consumed[..].chain(r);
            execute(&mut cipher, &mut r, w, opt)?;
            return Ok(());
        } else {
            Cipher::with_password_algorithm(password.as_bytes(), None, algo)?
        }
    } else if opt.password_prompt {
        let password = rpassword::prompt_password("Password: ")?;
        if !opt.decrypt {
            let password2 = rpassword::prompt_password("Password (again): ")?;
            if password != password2 {
                return Err("passwords do not match".into());
            }
        }
        if opt.decrypt {
            let (header, kdf_params, consumed) = aether::read_kdf_params(&mut r)?;
            let salt = match (&header.flags.version, &kdf_params) {
                (0, _) => header.integrity(),
                (_, KdfParams::Argon2id { salt, .. }) => *salt,
                _ => return Err("encrypted file has no KDF params for password-based decryption".into()),
            };
            let mut cipher = Cipher::with_password_algorithm(password.as_bytes(), Some(salt), algo)?;
            let mut r = consumed[..].chain(r);
            execute(&mut cipher, &mut r, w, opt)?;
            return Ok(());
        } else {
            Cipher::with_password_algorithm(password.as_bytes(), None, algo)?
        }
    } else {
        return Err("key is not specified".into());
    };
    cipher.set_format_version(opt.format_version);
    cipher.set_chunk_kind(chunk_kind);
    if opt.compress {
        cipher.set_compression(aether::Compression::Zstd);
    }
    execute(&mut cipher, r, w, opt)?;
    Ok(())
}

const EXT: &[u8] = b".aet";

fn append_ext(s: &Path) -> PathBuf {
    let mut buf = Vec::with_capacity(s.as_os_str().len() + EXT.len());
    buf.extend(s.as_os_str().as_bytes());
    buf.extend(EXT);
    s.with_file_name(OsStr::from_bytes(&buf))
}

fn remove_ext(s: &Path) -> PathBuf {
    if let Some(last) = s.components().next_back() {
        let basename = last.as_os_str().as_bytes();
        if basename.ends_with(EXT) {
            let basename = &basename[..basename.len() - EXT.len()];
            return s.with_file_name(OsStr::from_bytes(basename));
        }
    }
    s.to_owned()
}

fn auto_ext(s: &Path, decrypt: bool) -> PathBuf {
    if decrypt { remove_ext(s) } else { append_ext(s) }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn info<R: BufRead>(mut r: R) -> Result<(), Box<dyn std::error::Error>> {
    let mut header_bytes = [0u8; aether::HEADER_SIZE];
    r.read_exact(&mut header_bytes)?;
    let header = aether::Header::from_bytes(&header_bytes)?;
    let flags = header.flags;

    println!("Format version:    {}", flags.version);
    println!("Algorithm:         {}", flags.algorithm);
    println!("Nonce size:        {} bytes", flags.algorithm.nonce_size());
    println!("Nonce:             {}", hex(header.nonce_bytes()));
    let ct_size = flags.chunk_kind.ciphertext_size();
    let human = if ct_size >= 1024 * 1024 {
        format!("{} MiB", ct_size / (1024 * 1024))
    } else {
        format!("{} KiB", ct_size / 1024)
    };
    println!("Chunk kind:        {} (chunk = {})", flags.chunk_kind.value(), human);
    println!("Compression:       {}", flags.compression);

    if flags.version == 0 {
        println!("Integrity:         {}", hex(&header.integrity()));
    } else if flags.version == 1 {
        let nonce_size = flags.algorithm.nonce_size();
        let (key_block, _raw) = KeyBlock::from_reader(&mut r, nonce_size)?;
        let kdf = &key_block.kdf_params;
        match kdf {
            KdfParams::None => println!("KDF:               none (raw key)"),
            KdfParams::Argon2id { salt, m_cost, t_cost, p_cost } => {
                println!("KDF:               argon2id");
                println!("  salt:            {}", hex(salt));
                println!("  m_cost:          {} KiB", m_cost);
                println!("  t_cost:          {}", t_cost);
                println!("  p_cost:          {}", p_cost);
            }
        }
        println!("DEK nonce:         {}", hex(&key_block.dek_nonce[..nonce_size]));
        println!("Slot count:        {}", key_block.slots.len());
        for (i, slot) in key_block.slots.iter().enumerate() {
            println!("  slot[{}] key_id:  {}", i, hex(&slot.key_id));
        }
    }

    Ok(())
}

fn main_with_error() -> Result<i32, Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let opt = Opt::parse();

    if opt.info {
        if let Some(input) = opt.input.as_ref() {
            if Opt::path_is_stdin(input) {
                let stdin = std::io::stdin();
                info(stdin.lock())?;
            } else {
                let f = File::open(input)?;
                info(std::io::BufReader::new(f))?;
            }
        } else {
            let stdin = std::io::stdin();
            info(stdin.lock())?;
        }
        return Ok(0);
    }

    if opt.key_file_is_stdin() && opt.input_is_stdin() {
        return Err("key and input are both stdin".into());
    }

    if opt.input.is_none() || opt.input_is_stdin() {
        let stdin = std::io::stdin();
        let r = stdin.lock();
        if opt.stdout || opt.output.is_none() {
            let w = std::io::stdout();
            let w = w.lock();
            let w = BufWriter::new(w);
            process(r, w, &opt)?;
        } else if let Some(output) = opt.output.as_ref() {
            let tempfile = NamedTempFile::new_in(output.parent().unwrap())?;
            {
                let f = tempfile.reopen()?;
                let w = BufWriter::new(f);
                process(r, w, &opt)?;
            }
            tempfile.persist(output)?;
            let mut perms = metadata(output)?.permissions();
            perms.set_mode(0o644);
            set_permissions(output, perms)?;
        }
    } else if let Some(input) = opt.input.as_ref() {
        let r = File::open(input)?;
        let r = std::io::BufReader::new(r);
        if opt.stdout {
            let w = std::io::stdout();
            let w = w.lock();
            let w = BufWriter::new(w);
            process(r, w, &opt)?;
        } else {
            let output = opt.output.clone().unwrap_or_else(|| auto_ext(input, opt.decrypt));
            if input == &output {
                return Err("input and output are the same".into());
            }
            let tempfile = NamedTempFile::new_in(output.parent().unwrap())?;
            {
                let f = tempfile.reopen()?;
                let w = BufWriter::new(f);
                process(r, w, &opt)?;
            }
            tempfile.persist(&output)?;
            let input_perms = metadata(input)?.permissions();
            let mut perms = metadata(&output)?.permissions();
            perms.set_mode(input_perms.mode());
            set_permissions(&output, perms)?;
        }
    }
    Ok(0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match main_with_error() {
        Ok(code) => exit(code),
        Err(e) => {
            error!("{}", e);
            exit(1)
        }
    }
}
