use std::{ffi::OsStr, io::stdout, path::PathBuf};

use byteorder::{BigEndian, LittleEndian, WriteBytesExt};
use clap::Parser;
use serde::Serialize;
use sha1::Sha1;
use sha2::Sha256;
use std::io::{Error, Write};
use treblo::{
    hex::to_hex_string,
    mode::{HashAlgorithm, HashMode},
    native::{EntryKind, HashConfig, TreeResult, WalkOptions, compute_root_hash},
    walk,
    walk::Hasher,
};
use twox_hash::{XxHash3_64, XxHash64};

// ──────────────────────────────────────────────────────────────────────────────
// Git-mode hasher wrappers
// ──────────────────────────────────────────────────────────────────────────────

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

struct Blake3Holder(blake3::Hasher);

impl Write for Blake3Holder {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        self.0.update(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
impl Hasher for Blake3Holder {
    fn result_vec(&mut self) -> Vec<u8> {
        self.0.finalize().as_bytes().to_vec()
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

struct XxHash3_64Holder {
    hash: XxHash3_64,
    little_endian: bool,
}
impl Write for XxHash3_64Holder {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        use std::hash::Hasher;
        Hasher::write(&mut self.hash, buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
impl Hasher for XxHash3_64Holder {
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

// ──────────────────────────────────────────────────────────────────────────────
// CLI
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(name = "treblo", about = "File tree hash computation")]
pub struct Opt {
    #[arg(value_name = "PATHS")]
    paths: Vec<PathBuf>,

    /// Hash mode: git (default) | native
    #[arg(short = 'm', long = "mode", default_value = "git")]
    mode: String,

    /// Hash algorithm (default: native=blake3, git=sha1)
    #[arg(short = 'a', long = "algorithm")]
    algorithm: Option<String>,

    /// Show subtree hashes (native: direct children; git: all entries)
    #[arg(short, long)]
    verbose: bool,

    /// JSON output
    #[arg(short, long)]
    json: bool,

    // ── Native-mode options ──────────────────────────────────────────────────
    /// Exclude empty directories from the tree hash
    #[arg(long = "no-empty-dirs")]
    no_empty_dirs: bool,

    // ── Git-mode options ─────────────────────────────────────────────────────
    /// Show only the root hash (git mode)
    #[arg(short, long)]
    summarize: bool,

    /// Maximum path depth to display (git mode)
    #[arg(short, long)]
    depth: Option<usize>,

    /// Include the root path itself in output (git mode)
    #[arg(short = 'S', long = "no-self", action = clap::ArgAction::SetFalse)]
    show_self: bool,

    /// Hash file content only, skip tree hashing (git mode)
    #[arg(short, long)]
    blob_only: bool,

    /// Continue on errors instead of panicking (git mode)
    #[arg(short = 'E', long)]
    no_error: bool,

    // ── Ignore options (both modes) ──────────────────────────────────────────
    /// Disable .trebloignore pattern filtering
    #[arg(long = "no-ignore", action = clap::ArgAction::SetFalse)]
    ignore: bool,

    /// Disable dot-file ignore patterns
    #[arg(long = "no-ignore-dot", action = clap::ArgAction::SetFalse)]
    ignore_dot: bool,

    /// Disable VCS ignore patterns (.gitignore)
    #[arg(long = "no-ignore-vcs", action = clap::ArgAction::SetFalse)]
    ignore_vcs: bool,

    /// Disable global ignore patterns
    #[arg(long = "no-ignore-global", action = clap::ArgAction::SetFalse)]
    ignore_global: bool,

    /// Disable .git/info/exclude patterns
    #[arg(long = "no-ignore-exclude", action = clap::ArgAction::SetFalse)]
    ignore_exclude: bool,
}

// ──────────────────────────────────────────────────────────────────────────────
// JSON output types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct GitRecord<'a> {
    file_mode: i32,
    object_type: &'a str,
    digest: &'a str,
    path: &'a str,
}

#[derive(Serialize)]
struct NativeOutput {
    root: String,
    mode: String,
    algorithm: String,
    root_hash: String,
    nodes: Vec<NodeJson>,
    stats: StatsJson,
}

#[derive(Serialize)]
struct NodeJson {
    path: String,
    hash: String,
    children: Vec<EntryJson>,
}

#[derive(Serialize)]
struct EntryJson {
    kind: String,
    name: String,
    hash: String,
}

#[derive(Serialize)]
struct StatsJson {
    files: usize,
    dirs: usize,
}

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn resolve_algorithm(opt: &Opt, mode: HashMode) -> HashAlgorithm {
    if let Some(alg) = &opt.algorithm {
        alg.parse::<HashAlgorithm>().unwrap_or_else(|e| {
            eprintln!("treblo: {}", e);
            std::process::exit(1);
        })
    } else {
        HashAlgorithm::default_for(mode)
    }
}

fn base_paths(opt: &Opt) -> Vec<PathBuf> {
    if opt.paths.is_empty() { vec![PathBuf::from(".")] } else { opt.paths.clone() }
}

// ──────────────────────────────────────────────────────────────────────────────
// Native mode
// ──────────────────────────────────────────────────────────────────────────────

fn build_native_json(
    base_path: &std::path::Path,
    result: &TreeResult,
    mode: HashMode,
    alg: HashAlgorithm,
) -> NativeOutput {
    // Reverse nodes so root appears first in JSON output.
    let nodes: Vec<NodeJson> = result
        .nodes
        .iter()
        .rev()
        .map(|node| NodeJson {
            path: node.path.clone(),
            hash: to_hex_string(&node.hash),
            children: node
                .children
                .iter()
                .map(|child| EntryJson {
                    kind: match child.kind {
                        EntryKind::File => "file".to_string(),
                        EntryKind::Directory => "directory".to_string(),
                    },
                    name: child.name.clone(),
                    hash: to_hex_string(&child.hash),
                })
                .collect(),
        })
        .collect();

    NativeOutput {
        root: base_path.display().to_string(),
        mode: mode.to_string(),
        algorithm: alg.to_string(),
        root_hash: to_hex_string(&result.root_hash),
        nodes,
        stats: StatsJson { files: result.file_count, dirs: result.dir_count },
    }
}

fn run_native(opt: &Opt) {
    let mode = HashMode::Native;
    let algorithm = resolve_algorithm(opt, mode);

    let config = HashConfig::new(mode).with_algorithm(algorithm).unwrap_or_else(|e| {
        eprintln!("treblo: {}", e);
        std::process::exit(1);
    });

    let walk_options =
        WalkOptions { no_ignore: !opt.ignore, follow_symlinks: false, include_empty_dirs: !opt.no_empty_dirs };

    for base_path in base_paths(opt) {
        let result = compute_root_hash(&base_path, &config, &walk_options).unwrap_or_else(|e| {
            eprintln!("treblo: {}: {}", base_path.display(), e);
            std::process::exit(1);
        });

        if opt.json {
            let output = build_native_json(&base_path, &result, mode, algorithm);
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        } else {
            // Git-compatible output format.
            // nodes are in bottom-up order (deepest dir first, root last).
            for node in &result.nodes {
                let is_root = node.path.is_empty();
                let node_dir = if is_root { base_path.clone() } else { base_path.join(&node.path) };

                if !opt.summarize {
                    // Emit file children.
                    for child in &node.children {
                        if matches!(child.kind, EntryKind::File) {
                            println!(
                                "100644 blob {}\t{}",
                                to_hex_string(&child.hash),
                                node_dir.join(&child.name).display()
                            );
                        }
                    }
                }

                // Emit this directory.
                let emit_tree = if opt.summarize {
                    is_root
                } else if is_root {
                    opt.show_self
                } else {
                    true
                };
                if emit_tree {
                    println!("040000 tree {}\t{}", to_hex_string(&node.hash), node_dir.display());
                }
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Git mode
// ──────────────────────────────────────────────────────────────────────────────

fn run_git(opt: &Opt) {
    let algorithm = resolve_algorithm(opt, HashMode::Git);
    let paths = base_paths(opt);
    let path_is_default = opt.paths.is_empty();

    for base_path in &paths {
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
            hasher_supplier: match algorithm {
                HashAlgorithm::Sha1 => {
                    use sha1::Digest;
                    || Box::new(Sha1::new())
                }
                HashAlgorithm::Sha256 => {
                    use sha2::Digest;
                    || Box::new(Sha256Holder(Sha256::new()))
                }
                HashAlgorithm::Blake3 => || Box::new(Blake3Holder(blake3::Hasher::new())),
                HashAlgorithm::XxHash64 => {
                    || Box::new(XxHash64Holder { hash: XxHash64::default(), little_endian: false })
                }
                HashAlgorithm::XxHash3_64 => {
                    || Box::new(XxHash3_64Holder { hash: XxHash3_64::default(), little_endian: false })
                }
            },
            blob_only: opt.blob_only,
            no_error: opt.no_error,
        };

        tw.walk(base_path, w, &mut |p, e, is_tree| {
            if opt.blob_only && is_tree {
                return;
            }
            let object_type = if is_tree { "tree" } else { "blob" };
            let path = if path_is_default { p.strip_prefix(base_path).unwrap() } else { p };
            let path = if path.to_str().is_some_and(|p| p.is_empty()) { base_path.as_ref() } else { path };
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
                        let path_str = path.display().to_string();
                        let record = GitRecord {
                            file_mode: e.file_mode.as_i32(),
                            object_type,
                            digest: digest.as_str(),
                            path: path_str.as_str(),
                        };
                        serde_json::to_vec(&record).unwrap()
                    };
                    record_json.push(b'\n');
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

// ──────────────────────────────────────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────────────────────────────────────

fn main() {
    let opt = Opt::parse();

    let mode: HashMode = opt.mode.parse().unwrap_or_else(|e| {
        eprintln!("treblo: {}", e);
        std::process::exit(1);
    });

    match mode {
        HashMode::Native => run_native(&opt),
        HashMode::Git => run_git(&opt),
    }
}
