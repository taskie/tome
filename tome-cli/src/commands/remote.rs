use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use sea_orm::DatabaseConnection;

use tome_db::ops;

// ──────────────────────────────────────────────────────────────────────────────
// CLI types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct RemoteArgs {
    #[command(subcommand)]
    pub command: RemoteCommands,
}

#[derive(Subcommand)]
pub enum RemoteCommands {
    /// Register a remote peer
    Add(RemoteAddArgs),
    /// Update a remote peer
    Set(RemoteSetArgs),
    /// Remove a remote peer
    Rm(RemoteRmArgs),
    /// List remote peers
    List(RemoteListArgs),
}

#[derive(Args)]
pub struct RemoteAddArgs {
    /// Peer name
    pub name: String,
    /// Peer URL: sqlite:///path, postgres://... or https://tome.example.com
    pub peer_url: String,
    /// Local repository name [default: "default"]
    #[arg(long, default_value = "default")]
    pub repo: String,
    /// Remote repository name on the peer [default: same as --repo]
    #[arg(long)]
    pub peer_repo: Option<String>,
}

#[derive(Args)]
pub struct RemoteSetArgs {
    /// Peer name
    pub name: String,
    /// New peer URL
    #[arg(long)]
    pub peer_url: Option<String>,
    /// New remote repository name on the peer
    #[arg(long)]
    pub peer_repo: Option<String>,
    /// Repository name [default: "default"]
    #[arg(long, default_value = "default")]
    pub repo: String,
}

#[derive(Args)]
pub struct RemoteRmArgs {
    /// Peer name
    pub name: String,
    /// Repository name [default: "default"]
    #[arg(long, default_value = "default")]
    pub repo: String,
}

#[derive(Args)]
pub struct RemoteListArgs {
    /// Repository name [default: "default"]
    #[arg(long, default_value = "default")]
    pub repo: String,
}

// ──────────────────────────────────────────────────────────────────────────────
// Dispatch
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: RemoteArgs) -> Result<()> {
    match args.command {
        RemoteCommands::Add(a) => remote_add(db, a).await,
        RemoteCommands::Set(a) => remote_set(db, a).await,
        RemoteCommands::Rm(a) => remote_rm(db, a).await,
        RemoteCommands::List(a) => remote_list(db, a).await,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// remote add
// ──────────────────────────────────────────────────────────────────────────────

pub(crate) async fn remote_add(db: &DatabaseConnection, args: RemoteAddArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let peer_repo = args.peer_repo.unwrap_or_else(|| args.repo.clone());

    let config = serde_json::json!({ "peer_repo": peer_repo });
    let peer = ops::insert_sync_peer(db, &args.name, &args.peer_url, repo.id, config).await?;

    println!("remote peer registered: {} (id={}, url={}, peer_repo={})", peer.name, peer.id, peer.url, peer_repo);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// remote set
// ──────────────────────────────────────────────────────────────────────────────

pub(crate) async fn remote_set(db: &DatabaseConnection, args: RemoteSetArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let peer = ops::find_sync_peer(db, &args.name, repo.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("remote peer {:?} not found in repo {:?}", args.name, args.repo))?;

    if args.peer_url.is_none() && args.peer_repo.is_none() {
        bail!("nothing to update (specify --peer-url and/or --peer-repo)");
    }

    let new_config = if let Some(ref pr) = args.peer_repo {
        let mut cfg = peer.config.clone();
        cfg["peer_repo"] = serde_json::json!(pr);
        Some(cfg)
    } else {
        None
    };

    let updated = ops::update_sync_peer(db, peer.id, args.peer_url.as_deref(), new_config).await?;
    let peer_repo = updated.config.get("peer_repo").and_then(|v| v.as_str()).unwrap_or("-");
    println!("remote peer updated: {} (id={}, url={}, peer_repo={})", updated.name, updated.id, updated.url, peer_repo);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// remote rm
// ──────────────────────────────────────────────────────────────────────────────

pub(crate) async fn remote_rm(db: &DatabaseConnection, args: RemoteRmArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let peer = ops::find_sync_peer(db, &args.name, repo.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("remote peer {:?} not found in repo {:?}", args.name, args.repo))?;

    ops::delete_sync_peer(db, peer.id).await?;
    println!("remote peer removed: {} (id={})", peer.name, peer.id);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// remote list
// ──────────────────────────────────────────────────────────────────────────────

pub(crate) async fn remote_list(db: &DatabaseConnection, args: RemoteListArgs) -> Result<()> {
    let repo = ops::get_or_create_repository(db, &args.repo).await?;
    let peers = ops::list_sync_peers(db, repo.id).await?;

    if peers.is_empty() {
        println!("no remote peers for repo {:?}", args.repo);
        return Ok(());
    }

    println!("{:<20} {:<20} url", "name", "last_snapshot_id");
    println!("{}", "-".repeat(70));
    for p in peers {
        let last = p.last_snapshot_id.map(|id| id.to_string()).unwrap_or_else(|| "-".to_owned());
        println!("{:<20} {:<20} {}", p.name, last, p.url);
    }
    Ok(())
}
