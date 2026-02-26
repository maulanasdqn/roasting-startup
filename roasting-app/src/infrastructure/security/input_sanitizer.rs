use roasting_errors::AppError;

const MAX_URL_LENGTH: usize = 2048;
const BLOCKED_KEYWORDS: &[&str] = &[
    "ignore previous",
    "ignore all",
    "disregard",
    "forget your",
    "new instructions",
    "system prompt",
    "you are now",
    "pretend to be",
    "act as",
    "roleplay",
    "jailbreak",
    "dan mode",
    "developer mode",
    "bypass",
    "override",
    "abaikan instruksi",
    "lupakan",
    "instruksi baru",
];

const ALLOWED_SCHEMES: &[&str] = &["http", "https"];

pub struct InputSanitizer;

impl InputSanitizer {
    pub fn validate_url(url: &str) -> Result<String, AppError> {
        let url = url.trim();

        if url.is_empty() {
            return Err(AppError::InvalidUrl("URL tidak boleh kosong".to_string()));
        }

        if url.len() > MAX_URL_LENGTH {
            return Err(AppError::InvalidUrl("URL terlalu panjang".to_string()));
        }

        if Self::contains_injection_attempt(url) {
            tracing::warn!("Potential prompt injection detected in URL: {}", url);
            return Err(AppError::InvalidUrl(
                "URL mengandung karakter tidak valid".to_string(),
            ));
        }

        let parsed = url::Url::parse(url)
            .map_err(|_| AppError::InvalidUrl("Format URL tidak valid".to_string()))?;

        let scheme = parsed.scheme().to_lowercase();
        if !ALLOWED_SCHEMES.contains(&scheme.as_str()) {
            return Err(AppError::InvalidUrl(
                "Hanya HTTP dan HTTPS yang diizinkan".to_string(),
            ));
        }

        if parsed.host_str().is_none() {
            return Err(AppError::InvalidUrl("URL harus memiliki host".to_string()));
        }

        let host = parsed.host_str().unwrap();
        if host == "localhost" || host.starts_with("127.") || host.starts_with("192.168.") {
            return Err(AppError::InvalidUrl(
                "URL lokal tidak diizinkan".to_string(),
            ));
        }

        Ok(parsed.to_string())
    }

    pub fn sanitize_scraped_content(content: &str) -> String {
        let mut sanitized = content.to_string();

        for keyword in BLOCKED_KEYWORDS {
            let re = regex_lite::Regex::new(&format!("(?i){}", regex_lite::escape(keyword)))
                .unwrap_or_else(|_| regex_lite::Regex::new(".^").unwrap());
            sanitized = re.replace_all(&sanitized, "[FILTERED]").to_string();
        }

        sanitized
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .take(2000)
            .collect()
    }

    fn contains_injection_attempt(input: &str) -> bool {
        let lower = input.to_lowercase();
        BLOCKED_KEYWORDS.iter().any(|kw| lower.contains(kw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_url() {
        assert!(InputSanitizer::validate_url("https://tokopedia.com").is_ok());
        assert!(InputSanitizer::validate_url("http://example.com/path").is_ok());
    }

    #[test]
    fn test_invalid_url() {
        assert!(InputSanitizer::validate_url("").is_err());
        assert!(InputSanitizer::validate_url("not-a-url").is_err());
        assert!(InputSanitizer::validate_url("ftp://example.com").is_err());
        assert!(InputSanitizer::validate_url("http://localhost").is_err());
    }

    #[test]
    fn test_injection_detection() {
        assert!(InputSanitizer::validate_url("https://example.com/ignore previous").is_err());
        assert!(InputSanitizer::validate_url("https://example.com?q=system prompt").is_err());
    }
}
