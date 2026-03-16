use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "entries")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    pub snapshot_id: i64,
    pub path: String,
    /// 0 = deleted, 1 = present
    pub status: i16,
    pub object_id: Option<i64>,
    pub mode: Option<i32>,
    pub mtime: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(belongs_to = "super::snapshot::Entity", from = "Column::SnapshotId", to = "super::snapshot::Column::Id")]
    Snapshot,
    #[sea_orm(belongs_to = "super::object::Entity", from = "Column::ObjectId", to = "super::object::Column::Id")]
    Object,
}

impl Related<super::snapshot::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Snapshot.def()
    }
}

impl Related<super::object::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Object.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
