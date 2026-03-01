use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use tome_core::hash::{DigestAlgorithm, FastHashAlgorithm};
use tome_core::id::next_id;

use crate::entities::repository;

/// Get or create a repository by name.
pub async fn get_or_create_repository(db: &DatabaseConnection, name: &str) -> anyhow::Result<repository::Model> {
    if let Some(repo) = repository::Entity::find().filter(repository::Column::Name.eq(name)).one(db).await? {
        return Ok(repo);
    }

    let now = Utc::now().fixed_offset();
    let am = repository::ActiveModel {
        id: Set(next_id()?),
        name: Set(name.to_owned()),
        description: Set(String::new()),
        config: Set(serde_json::json!({})),
        created_at: Set(now),
        updated_at: Set(now),
    };
    Ok(am.insert(db).await?)
}

/// Read the digest algorithm stored in `repo.config["digest_algorithm"]`.
/// Returns `Sha256` if the key is absent (legacy repos default to SHA-256).
pub fn get_repository_digest_algorithm(repo: &repository::Model) -> anyhow::Result<DigestAlgorithm> {
    match repo.config.get("digest_algorithm").and_then(|v| v.as_str()) {
        Some(s) => s.parse::<DigestAlgorithm>().map_err(|e| anyhow::anyhow!(e)),
        None => Ok(DigestAlgorithm::Sha256),
    }
}

/// Persist `algo` into `repositories.config["digest_algorithm"]`.
pub async fn set_repository_digest_algorithm(
    db: &DatabaseConnection,
    repo: &repository::Model,
    algo: DigestAlgorithm,
) -> anyhow::Result<()> {
    let mut config = repo.config.clone();
    config["digest_algorithm"] = serde_json::Value::String(algo.as_str().to_owned());
    let mut am: repository::ActiveModel = repo.clone().into();
    am.config = Set(config);
    am.updated_at = Set(Utc::now().fixed_offset());
    am.update(db).await?;
    Ok(())
}

/// Return the digest algorithm for a repository, initialising it in the config
/// if not already set.  On first use `default_algo` is persisted.
pub async fn get_or_init_repository_digest_algorithm(
    db: &DatabaseConnection,
    repo: &repository::Model,
    default_algo: DigestAlgorithm,
) -> anyhow::Result<DigestAlgorithm> {
    if repo.config.get("digest_algorithm").is_some() {
        return get_repository_digest_algorithm(repo);
    }
    set_repository_digest_algorithm(db, repo, default_algo).await?;
    Ok(default_algo)
}

/// Read the fast-hash algorithm stored in `repo.config["fast_hash_algorithm"]`.
/// Returns `XxHash64` if the key is absent (legacy repos default to xxHash64).
pub fn get_repository_fast_hash_algorithm(repo: &repository::Model) -> anyhow::Result<FastHashAlgorithm> {
    match repo.config.get("fast_hash_algorithm").and_then(|v| v.as_str()) {
        Some(s) => s.parse::<FastHashAlgorithm>().map_err(|e| anyhow::anyhow!(e)),
        None => Ok(FastHashAlgorithm::XxHash64),
    }
}

/// Persist `algo` into `repositories.config["fast_hash_algorithm"]`.
pub async fn set_repository_fast_hash_algorithm(
    db: &DatabaseConnection,
    repo: &repository::Model,
    algo: FastHashAlgorithm,
) -> anyhow::Result<()> {
    let mut config = repo.config.clone();
    config["fast_hash_algorithm"] = serde_json::Value::String(algo.as_str().to_owned());
    let mut am: repository::ActiveModel = repo.clone().into();
    am.config = Set(config);
    am.updated_at = Set(Utc::now().fixed_offset());
    am.update(db).await?;
    Ok(())
}

/// Return the fast-hash algorithm for a repository, initialising it in the config
/// if not already set.  On first use `default_algo` is persisted.
pub async fn get_or_init_repository_fast_hash_algorithm(
    db: &DatabaseConnection,
    repo: &repository::Model,
    default_algo: FastHashAlgorithm,
) -> anyhow::Result<FastHashAlgorithm> {
    if repo.config.get("fast_hash_algorithm").is_some() {
        return get_repository_fast_hash_algorithm(repo);
    }
    set_repository_fast_hash_algorithm(db, repo, default_algo).await?;
    Ok(default_algo)
}

/// List all repositories.
pub async fn list_repositories(db: &DatabaseConnection) -> anyhow::Result<Vec<repository::Model>> {
    Ok(repository::Entity::find().all(db).await?)
}
