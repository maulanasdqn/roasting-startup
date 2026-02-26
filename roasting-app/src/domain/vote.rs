use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub user_id: uuid::Uuid,
    pub roast_id: uuid::Uuid,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Vote {
    pub fn new(user_id: uuid::Uuid, roast_id: uuid::Uuid) -> Self {
        Self {
            user_id,
            roast_id,
            created_at: None,
        }
    }
}

/// Result of a vote toggle operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteResult {
    pub voted: bool,
    pub new_fire_count: i32,
}
