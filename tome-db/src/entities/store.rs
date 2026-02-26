use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "stores")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    #[sea_orm(unique)]
    pub name: String,
    /// e.g. "file:///path", "s3://bucket/prefix", "ssh://host/path"
    pub url: String,
    pub config: Json,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::replica::Entity")]
    Replicas,
}

impl Related<super::replica::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Replicas.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
