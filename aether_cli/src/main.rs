use std::{
    env,
    error::Error,
    ffi::OsStr,
    fs::File,
    io::{BufRead, BufWriter, Read, Write},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::exit,
};

use aether::Cipher;
use log::error;
use structopt::StructOpt;
use tempfile::NamedTempFile;

#[derive(Debug, StructOpt)]
#[structopt(name = "aether")]
#[structopt(long_version(option_env!("LONG_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))))]
pub struct Opt {
    #[structopt(short = "c", long)]
    pub stdout: bool,

    #[structopt(short, long)]
    pub decrypt: bool,

    #[structopt(short, long)]
    pub output: Option<PathBuf>,

    #[structopt(short, long, env = "AETHER_KEY_FILE")]
    pub key_file: Option<PathBuf>,

    #[structopt(short = "K", long)]
    pub key_env: Option<String>,

    #[structopt(name = "INPUT")]
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

fn load_key<R: Read>(mut r: R) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut buf = Vec::with_capacity(aether::KEY_SIZE);
    r.read_to_end(&mut buf)?;
    Ok(buf)
}

fn process<R: BufRead, W: Write>(r: R, w: BufWriter<W>, opt: &Opt) -> Result<(), Box<dyn Error>> {
    let mut cipher = if let Some(key_file) = opt.key_file.as_ref() {
        let key = if Opt::path_is_stdin(key_file) {
            load_key(std::io::stdin().lock())?
        } else {
            let key_file = File::open(key_file)?;
            load_key(key_file)?
        };
        Cipher::with_key_slice(&key)
    } else if let Some(key) = opt.key_env.as_ref().and_then(|name| env::var(name).ok()) {
        Cipher::with_key_b64(&key)
    } else {
        return Err("key is not specified".into());
    };
    if opt.decrypt {
        cipher.decrypt(r, w)?;
    } else {
        cipher.encrypt(r, w)?;
    }
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
    if let Some(last) = s.components().last() {
        let basename = last.as_os_str().as_bytes();
        if basename.ends_with(EXT) {
            let basename = &basename[..basename.len() - EXT.len()];
            return s.with_file_name(OsStr::from_bytes(basename));
        }
    }
    s.to_owned()
}

fn auto_ext(s: &Path, decrypt: bool) -> PathBuf {
    if decrypt {
        remove_ext(s)
    } else {
        append_ext(s)
    }
}

fn main_with_error() -> Result<i32, Box<dyn Error>> {
    env_logger::init();
    let opt = Opt::from_args();

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
        } else {
            let output = opt.output.as_ref().unwrap();
            let tempfile = NamedTempFile::new_in(output.parent().unwrap())?;
            {
                let f = tempfile.reopen()?;
                let w = BufWriter::new(f);
                process(r, w, &opt)?;
            }
            tempfile.persist(output)?;
        }
    } else {
        let input = opt.input.as_ref().unwrap();
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
        }
    }
    Ok(0)
}

fn main() -> Result<(), Box<dyn Error>> {
    match main_with_error() {
        Ok(code) => exit(code),
        Err(e) => {
            error!("{}", e);
            exit(1)
        }
    }
}
