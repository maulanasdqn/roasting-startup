use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum AppError {
    #[error("URL tidak valid: {0}")]
    InvalidUrl(String),

    #[error("Gagal mengakses website: {0}")]
    ScrapingFailed(String),

    #[error("Gagal menghubungi AI: {0}")]
    OpenRouterError(String),

    #[error("Gagal generate dengan LLM lokal: {0}")]
    LlmError(String),

    #[error("Website tidak ditemukan")]
    NotFound,

    #[error("Request timeout")]
    Timeout,

    #[error("Terjadi kesalahan internal: {0}")]
    Internal(String),
}

impl FromStr for AppError {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("URL tidak valid") {
            Ok(AppError::InvalidUrl(s.to_string()))
        } else if s.starts_with("Gagal mengakses") {
            Ok(AppError::ScrapingFailed(s.to_string()))
        } else if s.starts_with("Gagal menghubungi") {
            Ok(AppError::OpenRouterError(s.to_string()))
        } else if s.contains("tidak ditemukan") {
            Ok(AppError::NotFound)
        } else if s.contains("timeout") {
            Ok(AppError::Timeout)
        } else {
            Ok(AppError::Internal(s.to_string()))
        }
    }
}

impl AppError {
    pub fn user_message(&self) -> &str {
        match self {
            Self::InvalidUrl(_) => "URL yang kamu masukkan tidak valid. Coba lagi!",
            Self::ScrapingFailed(_) => "Gagal mengakses website. Pastikan URL bisa diakses.",
            Self::OpenRouterError(_) => "AI sedang sibuk. Coba lagi nanti.",
            Self::LlmError(_) => "AI lokal lagi error. Coba lagi nanti.",
            Self::NotFound => "Website tidak ditemukan.",
            Self::Timeout => "Request terlalu lama. Coba lagi.",
            Self::Internal(_) => "Ada masalah di server. Coba lagi nanti.",
        }
    }
}

#[cfg(feature = "ssr")]
mod ssr_impl {
    use super::AppError;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::Json;

    #[derive(serde::Serialize)]
    struct ErrorResponse {
        message: String,
    }

    impl IntoResponse for AppError {
        fn into_response(self) -> Response {
            let (status, message) = match &self {
                AppError::InvalidUrl(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
                AppError::ScrapingFailed(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
                AppError::OpenRouterError(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
                AppError::LlmError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
                AppError::NotFound => (StatusCode::NOT_FOUND, "Not found".to_string()),
                AppError::Timeout => (StatusCode::GATEWAY_TIMEOUT, "Timeout".to_string()),
                AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            };
            (status, Json(ErrorResponse { message })).into_response()
        }
    }
}
