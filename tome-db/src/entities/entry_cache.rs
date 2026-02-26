use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "entry_cache")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub repository_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub path: String,
    pub snapshot_id: i64,
    pub entry_id: i64,
    pub status: i16,
    pub blob_id: Option<i64>,
    pub mtime: Option<DateTimeWithTimeZone>,
    /// Denormalized from blobs.digest for fast lookup
    pub digest: Option<Vec<u8>>,
    /// Denormalized from blobs.size
    pub size: Option<i64>,
    /// Denormalized from blobs.fast_digest
    pub fast_digest: Option<i64>,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::repository::Entity",
        from = "Column::RepositoryId",
        to = "super::repository::Column::Id"
    )]
    Repository,
    #[sea_orm(
        belongs_to = "super::snapshot::Entity",
        from = "Column::SnapshotId",
        to = "super::snapshot::Column::Id"
    )]
    Snapshot,
    #[sea_orm(
        belongs_to = "super::entry::Entity",
        from = "Column::EntryId",
        to = "super::entry::Column::Id"
    )]
    Entry,
}

impl Related<super::repository::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Repository.def()
    }
}

impl Related<super::snapshot::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Snapshot.def()
    }
}

impl Related<super::entry::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Entry.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
