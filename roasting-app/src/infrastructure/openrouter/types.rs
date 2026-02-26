use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: MessageContent,
}

#[derive(Debug, Deserialize)]
pub struct MessageContent {
    pub content: String,
}

impl ChatCompletionRequest {
    pub fn new(model: &str, prompt: String) -> Self {
        Self {
            model: model.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
            max_tokens: 2048,
            temperature: 0.9,
        }
    }
}
