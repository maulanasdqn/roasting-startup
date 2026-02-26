use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Roast {
    pub startup_name: String,
    pub roast_text: String,
}

impl Roast {
    pub fn new(startup_name: String, roast_text: String) -> Self {
        Self {
            startup_name,
            roast_text,
        }
    }
}
