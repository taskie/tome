use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "repositories")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    #[sea_orm(unique)]
    pub name: String,
    pub description: String,
    pub config: Json,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::snapshot::Entity")]
    Snapshots,
    #[sea_orm(has_many = "super::sync_peer::Entity")]
    SyncPeers,
    #[sea_orm(has_many = "super::entry_cache::Entity")]
    EntryCache,
}

impl Related<super::snapshot::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Snapshots.def()
    }
}

impl Related<super::sync_peer::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SyncPeers.def()
    }
}

impl Related<super::entry_cache::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::EntryCache.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
