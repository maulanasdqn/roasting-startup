use super::entities::{vote, Vote};
use crate::domain::VoteResult;
use sea_orm::{entity::*, query::*, DatabaseConnection, DbErr};
use uuid::Uuid;

#[derive(Clone)]
pub struct VoteRepository {
    db: DatabaseConnection,
}

impl VoteRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn exists(&self, user_id: Uuid, roast_id: Uuid) -> Result<bool, DbErr> {
        let vote = Vote::find()
            .filter(vote::Column::UserId.eq(user_id))
            .filter(vote::Column::RoastId.eq(roast_id))
            .one(&self.db)
            .await?;
        Ok(vote.is_some())
    }

    pub async fn create(&self, user_id: Uuid, roast_id: Uuid) -> Result<vote::Model, DbErr> {
        let active = vote::ActiveModel {
            user_id: Set(user_id),
            roast_id: Set(roast_id),
            created_at: Set(Some(chrono::Utc::now())),
        };
        active.insert(&self.db).await
    }

    pub async fn delete(&self, user_id: Uuid, roast_id: Uuid) -> Result<(), DbErr> {
        Vote::delete_many()
            .filter(vote::Column::UserId.eq(user_id))
            .filter(vote::Column::RoastId.eq(roast_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    /// Toggle vote and return the new state + fire count
    pub async fn toggle(
        &self,
        user_id: Uuid,
        roast_id: Uuid,
        roast_repo: &super::RoastRepository,
    ) -> Result<VoteResult, DbErr> {
        let exists = self.exists(user_id, roast_id).await?;

        if exists {
            // Remove vote
            self.delete(user_id, roast_id).await?;
            let new_count = roast_repo.decrement_fire_count(roast_id).await?;
            Ok(VoteResult {
                voted: false,
                new_fire_count: new_count,
            })
        } else {
            // Add vote
            self.create(user_id, roast_id).await?;
            let new_count = roast_repo.increment_fire_count(roast_id).await?;
            Ok(VoteResult {
                voted: true,
                new_fire_count: new_count,
            })
        }
    }
}
