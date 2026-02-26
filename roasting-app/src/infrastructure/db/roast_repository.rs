use super::entities::{roast, user, vote, Roast, User, Vote};
use crate::domain::RoastWithDetails;
use sea_orm::{entity::*, query::*, DatabaseConnection, DbErr, JoinType};
use uuid::Uuid;

#[derive(Clone)]
pub struct RoastRepository {
    db: DatabaseConnection,
}

impl RoastRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create(&self, roast_data: &crate::domain::PersistedRoast) -> Result<roast::Model, DbErr> {
        let active = roast::ActiveModel {
            id: Set(roast_data.id),
            startup_name: Set(roast_data.startup_name.clone()),
            startup_url: Set(roast_data.startup_url.clone()),
            roast_text: Set(roast_data.roast_text.clone()),
            user_id: Set(roast_data.user_id),
            fire_count: Set(roast_data.fire_count),
            created_at: Set(Some(chrono::Utc::now())),
        };
        active.insert(&self.db).await
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<roast::Model>, DbErr> {
        Roast::find_by_id(id).one(&self.db).await
    }

    pub async fn find_by_id_with_details(
        &self,
        id: Uuid,
        current_user_id: Option<Uuid>,
    ) -> Result<Option<RoastWithDetails>, DbErr> {
        // Build query with left join to users
        let query = Roast::find()
            .filter(roast::Column::Id.eq(id))
            .join(JoinType::LeftJoin, roast::Relation::User.def())
            .column_as(user::Column::Name, "author_name")
            .column_as(user::Column::AvatarUrl, "author_avatar");

        // Execute query and manually check vote status
        let row: Option<roast::Model> = query.clone().one(&self.db).await?;

        match row {
            Some(r) => {
                // Get user info separately
                let author_info: Option<(Option<String>, Option<String>)> = if r.user_id.is_some() {
                    User::find_by_id(r.user_id.unwrap())
                        .one(&self.db)
                        .await?
                        .map(|u| (Some(u.name), u.avatar_url))
                } else {
                    None
                };

                // Check if current user has voted
                let user_has_voted = match current_user_id {
                    Some(uid) => {
                        Vote::find()
                            .filter(vote::Column::UserId.eq(uid))
                            .filter(vote::Column::RoastId.eq(id))
                            .one(&self.db)
                            .await?
                            .is_some()
                    }
                    None => false,
                };

                Ok(Some(RoastWithDetails {
                    id: r.id,
                    startup_name: r.startup_name,
                    startup_url: r.startup_url,
                    roast_text: r.roast_text,
                    fire_count: r.fire_count,
                    author_name: author_info.as_ref().and_then(|(n, _)| n.clone()),
                    author_avatar: author_info.and_then(|(_, a)| a),
                    user_has_voted,
                    created_at: r.created_at,
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn get_leaderboard(
        &self,
        limit: u64,
        current_user_id: Option<Uuid>,
    ) -> Result<Vec<RoastWithDetails>, DbErr> {
        let roasts: Vec<roast::Model> = Roast::find()
            .order_by_desc(roast::Column::FireCount)
            .order_by_desc(roast::Column::CreatedAt)
            .limit(limit)
            .all(&self.db)
            .await?;

        let mut results = Vec::new();
        for r in roasts {
            // Get author info
            let author_info: Option<(String, Option<String>)> = if let Some(uid) = r.user_id {
                User::find_by_id(uid)
                    .one(&self.db)
                    .await?
                    .map(|u| (u.name, u.avatar_url))
            } else {
                None
            };

            // Check if current user has voted
            let user_has_voted = match current_user_id {
                Some(uid) => {
                    Vote::find()
                        .filter(vote::Column::UserId.eq(uid))
                        .filter(vote::Column::RoastId.eq(r.id))
                        .one(&self.db)
                        .await?
                        .is_some()
                }
                None => false,
            };

            results.push(RoastWithDetails {
                id: r.id,
                startup_name: r.startup_name,
                startup_url: r.startup_url,
                roast_text: r.roast_text,
                fire_count: r.fire_count,
                author_name: author_info.as_ref().map(|(n, _)| n.clone()),
                author_avatar: author_info.and_then(|(_, a)| a),
                user_has_voted,
                created_at: r.created_at,
            });
        }

        Ok(results)
    }

    pub async fn increment_fire_count(&self, id: Uuid) -> Result<i32, DbErr> {
        let roast = Roast::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(DbErr::RecordNotFound("Roast not found".to_string()))?;

        let new_count = roast.fire_count + 1;
        let mut active: roast::ActiveModel = roast.into();
        active.fire_count = Set(new_count);
        active.update(&self.db).await?;

        Ok(new_count)
    }

    pub async fn decrement_fire_count(&self, id: Uuid) -> Result<i32, DbErr> {
        let roast = Roast::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(DbErr::RecordNotFound("Roast not found".to_string()))?;

        let new_count = (roast.fire_count - 1).max(0);
        let mut active: roast::ActiveModel = roast.into();
        active.fire_count = Set(new_count);
        active.update(&self.db).await?;

        Ok(new_count)
    }
}
