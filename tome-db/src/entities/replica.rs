use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "replicas")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    pub object_id: i64,
    pub store_id: i64,
    pub path: String,
    pub encrypted: bool,
    pub verified_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(belongs_to = "super::object::Entity", from = "Column::ObjectId", to = "super::object::Column::Id")]
    Object,
    #[sea_orm(belongs_to = "super::store::Entity", from = "Column::StoreId", to = "super::store::Column::Id")]
    Store,
}

impl Related<super::object::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Object.def()
    }
}

impl Related<super::store::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Store.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
