use std::{env, ffi::OsStr, io::stdout, path::PathBuf};

use byteorder::{BigEndian, LittleEndian, WriteBytesExt};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::Sha256;
use std::io::{Error, Write};
use structopt::{clap, StructOpt};
use treblo::{hex::to_hex_string, walk, walk::Hasher};
use twox_hash::XxHash64;

#[derive(Debug, StructOpt)]
#[structopt(name = "treblo")]
#[structopt(long_version(option_env!("LONG_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))))]
#[structopt(setting(clap::AppSettings::ColoredHelp))]
pub struct Opt {
    #[structopt(name = "PATHS")]
    paths: Vec<PathBuf>,

    #[structopt(short, long)]
    summarize: bool,

    #[structopt(short, long)]
    depth: Option<usize>,

    #[structopt(short = "S", long = "no-self", parse(from_flag = std::ops::Not::not))]
    show_self: bool,

    #[structopt(short, long)]
    json: bool,

    #[structopt(short = "H", long, default_value = "sha1")]
    hasher: String,

    #[structopt(short, long)]
    blob_only: bool,

    #[structopt(short = "E", long)]
    no_error: bool,

    #[structopt(long = "no-ignore", parse(from_flag = std::ops::Not::not))]
    ignore: bool,

    #[structopt(long = "no-ignore-dot", parse(from_flag = std::ops::Not::not))]
    ignore_dot: bool,

    #[structopt(long = "no-ignore-vcs", parse(from_flag = std::ops::Not::not))]
    ignore_vcs: bool,

    #[structopt(long = "no-ignore-global", parse(from_flag = std::ops::Not::not))]
    ignore_global: bool,

    #[structopt(long = "no-ignore-exclude", parse(from_flag = std::ops::Not::not))]
    ignore_exclude: bool,
}

#[derive(Serialize, Deserialize)]
struct Record<'a> {
    file_mode: i32,
    object_type: &'a str,
    digest: &'a str,
    path: &'a str,
}

struct Sha256Holder(Sha256);

impl Write for Sha256Holder {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> Result<(), Error> {
        self.0.flush()
    }
}

impl Hasher for Sha256Holder {
    fn result_vec(&mut self) -> Vec<u8> {
        use sha2::Digest;
        self.0.clone().finalize().to_vec()
    }
}

struct XxHash64Holder {
    hash: XxHash64,
    little_endian: bool,
}

impl Write for XxHash64Holder {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        use std::hash::Hasher;
        Hasher::write(&mut self.hash, buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

impl Hasher for XxHash64Holder {
    fn result_vec(&mut self) -> Vec<u8> {
        use std::hash::Hasher;
        let mut w = vec![];
        let x = Hasher::finish(&self.hash);
        if self.little_endian {
            w.write_u64::<LittleEndian>(x).unwrap();
        } else {
            w.write_u64::<BigEndian>(x).unwrap();
        }
        w
    }
}

fn main() {
    let opt = Opt::from_args();
    let path_is_default: bool = opt.paths.is_empty();
    let base_paths: Vec<PathBuf> = if path_is_default { vec![PathBuf::from(".")] } else { opt.paths.clone() };
    for base_path in base_paths.iter() {
        let w = {
            let mut wb = ignore::WalkBuilder::new(base_path);
            wb.hidden(false)
                .ignore(opt.ignore_dot)
                .git_global(opt.ignore_vcs && opt.ignore_global)
                .git_ignore(opt.ignore_vcs)
                .git_exclude(opt.ignore_vcs && opt.ignore_exclude);
            if opt.ignore_vcs {
                wb.filter_entry(|p| p.file_name() != OsStr::new(".git"));
            }
            if opt.ignore {
                wb.add_custom_ignore_filename(".trebloignore");
            }
            wb.build()
        };
        let tw = walk::TrebloWalk {
            hasher_supplier: match opt.hasher.as_str() {
                "sha1" => {
                    use sha1::Digest;
                    || Box::new(Sha1::new())
                }
                "sha256" => {
                    use sha2::Digest;
                    || Box::new(Sha256Holder(Sha256::new()))
                }
                "xxhash64" => || Box::new(XxHash64Holder { hash: XxHash64::default(), little_endian: false }),
                _ => panic!("unknown hasher: {}", opt.hasher),
            },
            blob_only: opt.blob_only,
            no_error: opt.no_error,
        };
        tw.walk(base_path, w, &mut |p, e, is_tree| {
            if opt.blob_only && is_tree {
                return;
            }
            let object_type = if is_tree { "tree" } else { "blob" };
            let path = if path_is_default { p.strip_prefix(&base_path).unwrap() } else { p };
            let path = if path.to_str().map_or(false, |p| p.is_empty()) { base_path.as_ref() } else { path };
            let depth = path.iter().count();
            if !opt.show_self && !opt.summarize && is_tree && p == base_path {
                return;
            }
            let depth_ok = if opt.summarize {
                false
            } else if let Some(d) = opt.depth {
                depth <= d
            } else {
                true
            };
            if depth_ok || p == base_path {
                if opt.json {
                    let mut record_json = {
                        let digest = to_hex_string(e.digest.as_slice());
                        let path = path.display().to_string();
                        let record = Record {
                            file_mode: e.file_mode.as_i32(),
                            object_type,
                            digest: digest.as_str(),
                            path: path.as_str(),
                        };
                        serde_json::to_vec(&record).unwrap()
                    };
                    record_json.push('\n' as u8);
                    let out = stdout();
                    let mut lock = out.lock();
                    lock.write_all(&record_json).unwrap();
                    lock.flush().unwrap();
                } else {
                    println!(
                        "{:06o} {} {}\t{}",
                        e.file_mode.as_i32(),
                        object_type,
                        to_hex_string(e.digest.as_slice()),
                        path.display()
                    )
                }
            }
        });
    }
}
