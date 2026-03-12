use std::{ffi::OsStr, io::stdout, path::PathBuf};

use clap::Parser;
use serde::Serialize;
use std::io::Write;
use treblo::{
    hex::to_hex_string,
    mode::{HashAlgorithm, HashConfig, HashMode},
    walk::TrebloWalk,
};

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

    /// JSON output
    #[arg(short, long)]
    json: bool,

    // ── Output options ────────────────────────────────────────────────────────
    /// Show only the root hash
    #[arg(short, long)]
    summarize: bool,

    /// Maximum path depth to display
    #[arg(short, long)]
    depth: Option<usize>,

    /// Include the root path itself in output
    #[arg(short = 'S', long = "no-self", action = clap::ArgAction::SetFalse)]
    show_self: bool,

    /// Hash file content only, skip tree hashing
    #[arg(short, long)]
    blob_only: bool,

    /// Continue on errors instead of panicking
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
// JSON output type
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct Record<'a> {
    file_mode: i32,
    object_type: &'a str,
    digest: &'a str,
    path: &'a str,
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
// Unified run
// ──────────────────────────────────────────────────────────────────────────────

fn run(opt: &Opt, mode: HashMode) {
    let algorithm = resolve_algorithm(opt, mode);
    let config = HashConfig::new(mode).with_algorithm(algorithm).unwrap_or_else(|e| {
        eprintln!("treblo: {}", e);
        std::process::exit(1);
    });

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

        let tw = TrebloWalk { config, blob_only: opt.blob_only, no_error: opt.no_error };

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
                        let record = Record {
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

    run(&opt, mode);
}
