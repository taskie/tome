use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "objects")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    #[sea_orm(column_type = "VarBinary(StringLen::None)", unique)]
    pub digest: Vec<u8>,
    /// File size (blob) or serialized tree content size (tree).
    pub size: Option<i64>,
    /// xxHash64 of content.
    pub fast_digest: Option<i64>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::entry::Entity")]
    Entries,
    #[sea_orm(has_many = "super::replica::Entity")]
    Replicas,
    #[sea_orm(has_many = "super::tag::Entity")]
    Tags,
}

impl Related<super::entry::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Entries.def()
    }
}

impl Related<super::replica::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Replicas.def()
    }
}

impl Related<super::tag::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tags.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
