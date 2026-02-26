use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub google_id: String,
    #[sea_orm(unique)]
    pub email: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub created_at: Option<DateTimeUtc>,
    pub updated_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::roast::Entity")]
    Roasts,
    #[sea_orm(has_many = "super::vote::Entity")]
    Votes,
}

impl Related<super::roast::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Roasts.def()
    }
}

impl Related<super::vote::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Votes.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
