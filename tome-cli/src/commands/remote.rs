use anyhow::Result;
use clap::{Args, Subcommand};
use sea_orm::DatabaseConnection;

use super::sync::{self, SyncAddArgs, SyncListArgs, SyncRmArgs, SyncSetArgs};

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
    Add(SyncAddArgs),
    /// Update a remote peer
    Set(SyncSetArgs),
    /// Remove a remote peer
    Rm(SyncRmArgs),
    /// List remote peers
    List(SyncListArgs),
}

// ──────────────────────────────────────────────────────────────────────────────
// Dispatch — delegates to sync implementation functions directly
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: RemoteArgs) -> Result<()> {
    match args.command {
        RemoteCommands::Add(a) => sync::sync_add(db, a).await,
        RemoteCommands::Set(a) => sync::sync_set(db, a).await,
        RemoteCommands::Rm(a) => sync::sync_rm(db, a).await,
        RemoteCommands::List(a) => sync::sync_list(db, a).await,
    }
}
