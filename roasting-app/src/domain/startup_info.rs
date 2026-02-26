use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupInfo {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub headings: Vec<String>,
    pub content_summary: String,
}

impl StartupInfo {
    pub fn new(url: String) -> Self {
        Self {
            url,
            title: None,
            description: None,
            headings: Vec::new(),
            content_summary: String::new(),
        }
    }

    pub fn with_title(mut self, title: Option<String>) -> Self {
        self.title = title;
        self
    }

    pub fn with_description(mut self, description: Option<String>) -> Self {
        self.description = description;
        self
    }

    pub fn with_headings(mut self, headings: Vec<String>) -> Self {
        self.headings = headings;
        self
    }

    pub fn with_content_summary(mut self, content_summary: String) -> Self {
        self.content_summary = content_summary;
        self
    }
}
