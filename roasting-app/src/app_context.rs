use crate::application::GenerateRoast;
use crate::infrastructure::security::{CostTracker, RateLimiter};
use std::sync::Arc;

#[cfg(feature = "ssr")]
use crate::infrastructure::auth::GoogleOAuth;
#[cfg(feature = "ssr")]
use crate::infrastructure::db::{RoastRepository, UserRepository, VoteRepository};
#[cfg(feature = "ssr")]
use sea_orm::DatabaseConnection;

#[derive(Clone)]
pub struct AppContext {
    pub generate_roast: Arc<GenerateRoast>,
    pub rate_limiter: RateLimiter,
    pub cost_tracker: Arc<CostTracker>,
    #[cfg(feature = "ssr")]
    pub db: DatabaseConnection,
    #[cfg(feature = "ssr")]
    pub google_oauth: Arc<GoogleOAuth>,
    #[cfg(feature = "ssr")]
    pub user_repo: UserRepository,
    #[cfg(feature = "ssr")]
    pub roast_repo: RoastRepository,
    #[cfg(feature = "ssr")]
    pub vote_repo: VoteRepository,
}

impl AppContext {
    #[cfg(feature = "ssr")]
    pub fn new(
        generate_roast: Arc<GenerateRoast>,
        db: DatabaseConnection,
        google_oauth: Arc<GoogleOAuth>,
    ) -> Self {
        let user_repo = UserRepository::new(db.clone());
        let roast_repo = RoastRepository::new(db.clone());
        let vote_repo = VoteRepository::new(db.clone());

        Self {
            generate_roast,
            rate_limiter: RateLimiter::new(),
            cost_tracker: Arc::new(CostTracker::new()),
            db,
            google_oauth,
            user_repo,
            roast_repo,
            vote_repo,
        }
    }

    #[cfg(feature = "ssr")]
    pub async fn from_env() -> Self {
        // Database
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let db = crate::infrastructure::db::create_connection(&database_url)
            .await
            .expect("Failed to create database connection");

        // Run migrations
        crate::infrastructure::db::run_migrations(&db)
            .await
            .expect("Failed to run migrations");
        tracing::info!("Database connected and migrations applied");

        // Google OAuth
        let google_client_id =
            std::env::var("GOOGLE_CLIENT_ID").expect("GOOGLE_CLIENT_ID must be set");
        let google_client_secret =
            std::env::var("GOOGLE_CLIENT_SECRET").expect("GOOGLE_CLIENT_SECRET must be set");
        let google_redirect_uri =
            std::env::var("GOOGLE_REDIRECT_URI").expect("GOOGLE_REDIRECT_URI must be set");
        let google_oauth = Arc::new(
            GoogleOAuth::new(&google_client_id, &google_client_secret, &google_redirect_uri)
                .expect("Failed to create Google OAuth client"),
        );
        tracing::info!("Google OAuth configured");

        // LLM Backend
        let generate_roast = {
            #[cfg(feature = "local-llm")]
            {
                if std::env::var("USE_LOCAL_LLM").is_ok() {
                    tracing::info!("Using local LLM backend (SmolLM2-135M-Instruct)");
                    Arc::new(GenerateRoast::new_local())
                } else {
                    let api_key = std::env::var("OPENROUTER_API_KEY")
                        .expect("OPENROUTER_API_KEY or USE_LOCAL_LLM must be set");
                    tracing::info!("Using OpenRouter backend");
                    Arc::new(GenerateRoast::new_openrouter(api_key))
                }
            }
            #[cfg(not(feature = "local-llm"))]
            {
                let api_key = std::env::var("OPENROUTER_API_KEY")
                    .expect("OPENROUTER_API_KEY must be set");
                tracing::info!("Using OpenRouter backend");
                Arc::new(GenerateRoast::new_openrouter(api_key))
            }
        };

        Self::new(generate_roast, db, google_oauth)
    }
}
