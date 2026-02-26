use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedRoast {
    pub id: uuid::Uuid,
    pub startup_name: String,
    pub startup_url: String,
    pub roast_text: String,
    pub user_id: Option<uuid::Uuid>,
    pub fire_count: i32,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl PersistedRoast {
    pub fn new(
        startup_name: String,
        startup_url: String,
        roast_text: String,
        user_id: Option<uuid::Uuid>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            startup_name,
            startup_url,
            roast_text,
            user_id,
            fire_count: 0,
            created_at: None,
        }
    }
}

/// Roast with additional info for display (e.g., author name, user's vote status)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoastWithDetails {
    pub id: uuid::Uuid,
    pub startup_name: String,
    pub startup_url: String,
    pub roast_text: String,
    pub fire_count: i32,
    pub author_name: Option<String>,
    pub author_avatar: Option<String>,
    pub user_has_voted: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}
