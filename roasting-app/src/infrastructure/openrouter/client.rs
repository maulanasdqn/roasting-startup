use super::prompt::build_roast_prompt;
use super::types::{ChatCompletionRequest, ChatCompletionResponse};
use crate::domain::StartupInfo;
use roasting_errors::AppError;

const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const MODEL: &str = "deepseek/deepseek-chat";

pub struct OpenRouterClient {
    http_client: reqwest::Client,
    api_key: String,
}

impl OpenRouterClient {
    pub fn new(api_key: String) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            api_key,
        }
    }

    pub async fn generate_roast(&self, startup_info: &StartupInfo) -> Result<String, AppError> {
        let prompt = build_roast_prompt(startup_info);
        let request = ChatCompletionRequest::new(MODEL, prompt);

        let response = self
            .http_client
            .post(OPENROUTER_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://roasting-startup.local")
            .header("X-Title", "Roasting Startup Indonesia")
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::OpenRouterError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!("OpenRouter error: {} - {}", status, body);
            return Err(AppError::OpenRouterError(format!(
                "API error: {}",
                status
            )));
        }

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| AppError::OpenRouterError(e.to_string()))?;

        completion
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| AppError::OpenRouterError("No response from AI".to_string()))
    }
}
