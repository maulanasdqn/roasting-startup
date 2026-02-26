use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "roasts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub startup_name: String,
    pub startup_url: String,
    #[sea_orm(column_type = "Text")]
    pub roast_text: String,
    pub user_id: Option<Uuid>,
    pub fire_count: i32,
    pub created_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id",
        on_delete = "SetNull"
    )]
    User,
    #[sea_orm(has_many = "super::vote::Entity")]
    Votes,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::vote::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Votes.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
