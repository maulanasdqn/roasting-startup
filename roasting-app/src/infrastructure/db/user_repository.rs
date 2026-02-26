use super::entities::{user, User};
use sea_orm::{entity::*, query::*, DatabaseConnection, DbErr};
use uuid::Uuid;

#[derive(Clone)]
pub struct UserRepository {
    db: DatabaseConnection,
}

impl UserRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<user::Model>, DbErr> {
        User::find_by_id(id).one(&self.db).await
    }

    pub async fn find_by_google_id(&self, google_id: &str) -> Result<Option<user::Model>, DbErr> {
        User::find()
            .filter(user::Column::GoogleId.eq(google_id))
            .one(&self.db)
            .await
    }

    pub async fn upsert(&self, user_data: &crate::domain::User) -> Result<user::Model, DbErr> {
        // Try to find existing user by google_id
        if let Some(existing) = self.find_by_google_id(&user_data.google_id).await? {
            // Update existing user
            let mut active: user::ActiveModel = existing.into();
            active.email = Set(user_data.email.clone());
            active.name = Set(user_data.name.clone());
            active.avatar_url = Set(user_data.avatar_url.clone());
            active.updated_at = Set(Some(chrono::Utc::now()));
            active.update(&self.db).await
        } else {
            // Insert new user
            let active = user::ActiveModel {
                id: Set(user_data.id),
                google_id: Set(user_data.google_id.clone()),
                email: Set(user_data.email.clone()),
                name: Set(user_data.name.clone()),
                avatar_url: Set(user_data.avatar_url.clone()),
                created_at: Set(Some(chrono::Utc::now())),
                updated_at: Set(Some(chrono::Utc::now())),
            };
            active.insert(&self.db).await
        }
    }
}
