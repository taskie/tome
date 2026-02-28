use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use sea_orm::DatabaseConnection;

use tome_core::hash;
use tome_db::ops;

// ──────────────────────────────────────────────────────────────────────────────
// CLI types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct TagArgs {
    #[command(subcommand)]
    pub command: TagCommands,
}

#[derive(Subcommand)]
pub enum TagCommands {
    /// Set (upsert) a key=value tag on a blob
    Set(TagSetArgs),
    /// Delete a tag key from a blob
    Delete(TagDeleteArgs),
    /// List all tags for a blob
    List(TagListArgs),
    /// Search for blobs by tag key or key=value
    Search(TagSearchArgs),
}

#[derive(Args)]
pub struct TagSetArgs {
    /// Blob SHA-256 digest (full hex or unambiguous prefix)
    pub digest: String,
    /// Tag key
    pub key: String,
    /// Tag value (omit to set a key-only tag)
    pub value: Option<String>,
}

#[derive(Args)]
pub struct TagDeleteArgs {
    /// Blob SHA-256 digest (full hex or unambiguous prefix)
    pub digest: String,
    /// Tag key to delete
    pub key: String,
}

#[derive(Args)]
pub struct TagListArgs {
    /// Blob SHA-256 digest (full hex or unambiguous prefix)
    pub digest: String,
}

#[derive(Args)]
pub struct TagSearchArgs {
    /// Tag key to search for
    pub key: String,
    /// Optional tag value filter
    pub value: Option<String>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Dispatch
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run(db: &DatabaseConnection, args: TagArgs) -> Result<()> {
    match args.command {
        TagCommands::Set(a) => tag_set(db, a).await,
        TagCommands::Delete(a) => tag_delete(db, a).await,
        TagCommands::List(a) => tag_list(db, a).await,
        TagCommands::Search(a) => tag_search(db, a).await,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// tag set
// ──────────────────────────────────────────────────────────────────────────────

async fn tag_set(db: &DatabaseConnection, args: TagSetArgs) -> Result<()> {
    let blob = ops::find_blob_by_hex(db, &args.digest)
        .await?
        .ok_or_else(|| anyhow::anyhow!("blob not found: {:?}", args.digest))?;

    let value = args.value.as_deref();
    ops::upsert_tag(db, blob.id, &args.key, value).await?;

    let digest_short = hash::hex_encode(&blob.digest)[..12].to_owned();
    match value {
        Some(v) => println!("set {}={:?} on blob {}", args.key, v, digest_short),
        None => println!("set {} on blob {}", args.key, digest_short),
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// tag delete
// ──────────────────────────────────────────────────────────────────────────────

async fn tag_delete(db: &DatabaseConnection, args: TagDeleteArgs) -> Result<()> {
    let blob = ops::find_blob_by_hex(db, &args.digest)
        .await?
        .ok_or_else(|| anyhow::anyhow!("blob not found: {:?}", args.digest))?;

    let deleted = ops::delete_tags(db, blob.id, &args.key).await?;
    let digest_short = hash::hex_encode(&blob.digest)[..12].to_owned();

    if deleted == 0 {
        bail!("no tag {:?} found on blob {}", args.key, digest_short);
    }
    println!("deleted {} tag(s) {:?} from blob {}", deleted, args.key, digest_short);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// tag list
// ──────────────────────────────────────────────────────────────────────────────

async fn tag_list(db: &DatabaseConnection, args: TagListArgs) -> Result<()> {
    let blob = ops::find_blob_by_hex(db, &args.digest)
        .await?
        .ok_or_else(|| anyhow::anyhow!("blob not found: {:?}", args.digest))?;

    let tags = ops::list_tags(db, blob.id).await?;
    let digest_short = hash::hex_encode(&blob.digest)[..12].to_owned();

    if tags.is_empty() {
        println!("no tags for blob {}", digest_short);
        return Ok(());
    }

    println!("tags for blob {}:", digest_short);
    for t in &tags {
        match &t.value {
            Some(v) => println!("  {}={}", t.key, v),
            None => println!("  {}", t.key),
        }
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// tag search
// ──────────────────────────────────────────────────────────────────────────────

async fn tag_search(db: &DatabaseConnection, args: TagSearchArgs) -> Result<()> {
    let results = ops::search_blobs_by_tag(db, &args.key, args.value.as_deref()).await?;

    if results.is_empty() {
        println!("no blobs found");
        return Ok(());
    }

    println!("{:<20} {:<14} tags", "digest", "size");
    println!("{}", "-".repeat(70));
    for (blob, tags) in &results {
        let digest_hex = hash::hex_encode(&blob.digest);
        let tag_str = tags
            .iter()
            .map(|t| match &t.value {
                Some(v) => format!("{}={}", t.key, v),
                None => t.key.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        println!("{:<20} {:>14} {}", &digest_hex[..20], blob.size.to_string(), tag_str);
    }
    Ok(())
}
