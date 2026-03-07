use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Args;
use notify::{RecursiveMode, Watcher};
use sea_orm::DatabaseConnection;
use tracing::{info, warn};

use tome_core::hash::{DigestAlgorithm, FastHashAlgorithm};
use tome_db::ops;

use super::scan::{ScanArgs, run as run_scan};

const DEFAULT_QUIET_PERIOD_SECS: u64 = 60;
const DEFAULT_MAX_DELAY_SECS: u64 = 600;

#[derive(Args)]
pub struct WatchArgs {
    /// Repository name [default: "default"]
    #[arg(long, short = 'r', default_value = "default")]
    pub repo: String,

    /// Directory to watch (default: current directory or saved scan_root)
    pub path: Option<PathBuf>,

    /// Seconds of inactivity before taking a snapshot [config: watch.quiet_period]
    #[arg(long)]
    pub quiet_period: Option<u64>,

    /// Max seconds from first change to forced snapshot [config: watch.max_delay]
    #[arg(long)]
    pub max_delay: Option<u64>,
}

pub async fn run(db: &DatabaseConnection, args: WatchArgs) -> Result<()> {
    let quiet_period = Duration::from_secs(args.quiet_period.unwrap_or(DEFAULT_QUIET_PERIOD_SECS));
    let max_delay = Duration::from_secs(args.max_delay.unwrap_or(DEFAULT_MAX_DELAY_SECS));

    // Resolve watch root: CLI arg > saved config > current directory.
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let watch_path = match args.path.as_ref() {
        Some(p) => p.clone(),
        None => ops::get_repository_scan_root(&repo).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")),
    };
    let watch_root = watch_path.canonicalize().with_context(|| format!("cannot access watch root {:?}", watch_path))?;

    eprintln!(
        "tome watch: monitoring {:?} (quiet={}s, max_delay={}s) — Ctrl+C to stop",
        watch_root,
        quiet_period.as_secs(),
        max_delay.as_secs(),
    );
    info!("watching {:?} (quiet={}s, max_delay={}s)", watch_root, quiet_period.as_secs(), max_delay.as_secs(),);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    // Filter events: ignore .git/ directory and .db files to avoid spurious
    // triggers from the tome database itself.
    let filter_root = watch_root.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
        Ok(event) => {
            if event.kind.is_access() {
                return; // ignore pure read events
            }
            let relevant = event.paths.iter().any(|p| {
                let rel = p.strip_prefix(&filter_root).unwrap_or(p);
                !rel.components().any(|c| c.as_os_str() == ".git") && p.extension().is_none_or(|e| e != "db")
            });
            if relevant {
                let _ = tx.send(());
            }
        }
        Err(e) => warn!("watcher error: {}", e),
    })
    .context("failed to create filesystem watcher")?;

    watcher
        .watch(&watch_root, RecursiveMode::Recursive)
        .with_context(|| format!("failed to watch {:?}", watch_root))?;

    loop {
        // Block until the first file-system event arrives.
        if rx.recv().await.is_none() {
            break; // all senders dropped (watcher stopped)
        }

        let first_change = Instant::now();
        let mut last_change = Instant::now();
        info!("change detected, waiting for activity to settle...");

        // Drain events until the quiet period elapses or max_delay is hit.
        loop {
            let since_last = last_change.elapsed();
            let since_first = first_change.elapsed();

            if since_last >= quiet_period {
                info!("quiet period elapsed, taking snapshot");
                break;
            }
            if since_first >= max_delay {
                info!("max_delay reached, forcing snapshot");
                break;
            }

            let wait_quiet = quiet_period.saturating_sub(since_last);
            let wait_max = max_delay.saturating_sub(since_first);
            let wait = wait_quiet.min(wait_max);

            match tokio::time::timeout(wait, rx.recv()).await {
                Ok(Some(())) => last_change = Instant::now(),
                Ok(None) => return Ok(()), // channel closed
                Err(_) => {}               // timeout — re-check conditions
            }
        }

        // Discard queued events that arrived while we were waiting.
        while rx.try_recv().is_ok() {}

        let scan_args = ScanArgs {
            repo: args.repo.clone(),
            no_ignore: false,
            message: String::new(),
            digest_algorithm: DigestAlgorithm::Sha256,
            fast_hash_algorithm: FastHashAlgorithm::default(),
            batch_size: 1000,
            allow_empty: false,
            path: Some(watch_root.clone()),
        };

        if let Err(e) = run_scan(db, scan_args).await {
            warn!("scan failed: {:#}", e);
        }
    }

    Ok(())
}
