use crate::application::GenerateRoast;
use crate::infrastructure::security::{CostTracker, RateLimiter};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppContext {
    pub generate_roast: Arc<GenerateRoast>,
    pub rate_limiter: RateLimiter,
    pub cost_tracker: Arc<CostTracker>,
}

impl AppContext {
    pub fn new_openrouter(openrouter_api_key: String) -> Self {
        Self {
            generate_roast: Arc::new(GenerateRoast::new_openrouter(openrouter_api_key)),
            rate_limiter: RateLimiter::new(),
            cost_tracker: Arc::new(CostTracker::new()),
        }
    }

    #[cfg(feature = "local-llm")]
    pub fn new_local_llm() -> Self {
        Self {
            generate_roast: Arc::new(GenerateRoast::new_local()),
            rate_limiter: RateLimiter::new(),
            cost_tracker: Arc::new(CostTracker::new()),
        }
    }

    pub fn from_env() -> Self {
        #[cfg(feature = "local-llm")]
        {
            if std::env::var("USE_LOCAL_LLM").is_ok() {
                tracing::info!("Using local LLM backend (SmolLM2-135M-Instruct)");
                return Self::new_local_llm();
            }
        }

        let api_key = std::env::var("OPENROUTER_API_KEY")
            .expect("OPENROUTER_API_KEY or USE_LOCAL_LLM must be set");
        tracing::info!("Using OpenRouter backend");
        Self::new_openrouter(api_key)
    }
}
