use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: uuid::Uuid,
    pub google_id: String,
    pub email: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl User {
    pub fn new(google_id: String, email: String, name: String, avatar_url: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            google_id,
            email,
            name,
            avatar_url,
            created_at: None,
            updated_at: None,
        }
    }
}
