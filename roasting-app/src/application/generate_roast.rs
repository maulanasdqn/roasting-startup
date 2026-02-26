use crate::domain::{Roast, StartupInfo};
use crate::infrastructure::openrouter::OpenRouterClient;
use crate::infrastructure::scraper::WebsiteScraper;
use roasting_errors::AppError;

#[cfg(feature = "local-llm")]
use crate::infrastructure::local_llm::LocalLlm;

pub enum LlmBackend {
    OpenRouter(OpenRouterClient),
    #[cfg(feature = "local-llm")]
    Local,
}

pub struct GenerateRoast {
    scraper: WebsiteScraper,
    backend: LlmBackend,
}

impl GenerateRoast {
    pub fn new_openrouter(openrouter_api_key: String) -> Self {
        Self {
            scraper: WebsiteScraper::new(),
            backend: LlmBackend::OpenRouter(OpenRouterClient::new(openrouter_api_key)),
        }
    }

    #[cfg(feature = "local-llm")]
    pub fn new_local() -> Self {
        Self {
            scraper: WebsiteScraper::new(),
            backend: LlmBackend::Local,
        }
    }

    pub async fn execute(&self, url: String) -> Result<Roast, AppError> {
        let startup_info = self.scraper.scrape(&url).await?;
        let startup_name = startup_info
            .title
            .clone()
            .unwrap_or_else(|| "Startup Misterius".to_string());

        let roast_text = self.generate_roast_text(&startup_info).await?;
        Ok(Roast::new(startup_name, roast_text))
    }

    async fn generate_roast_text(&self, startup_info: &StartupInfo) -> Result<String, AppError> {
        match &self.backend {
            LlmBackend::OpenRouter(client) => client.generate_roast(startup_info).await,
            #[cfg(feature = "local-llm")]
            LlmBackend::Local => {
                let llm = LocalLlm::get_or_init()
                    .await
                    .map_err(|e| AppError::LlmError(e.to_string()))?;

                // Clone data for spawn_blocking
                let startup_info = startup_info.clone();

                // Run CPU-intensive generation in blocking thread pool
                tokio::task::spawn_blocking(move || {
                    llm.generate_roast(&startup_info)
                })
                .await
                .map_err(|e| AppError::LlmError(format!("Task join error: {}", e)))?
                .map_err(|e| AppError::LlmError(e.to_string()))
            }
        }
    }
}
