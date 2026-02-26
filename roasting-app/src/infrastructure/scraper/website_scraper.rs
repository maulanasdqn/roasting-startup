use crate::domain::StartupInfo;
use roasting_errors::AppError;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
];

const CLOUDFLARE_CHALLENGE_INDICATORS: &[&str] = &[
    "cf-browser-verification",
    "cf-challenge",
    "cf-turnstile",
    "checking your browser",
    "just a moment",
    "please wait while we verify",
    "enable javascript and cookies to continue",
    "challenge-platform",
    "cf-chl-bypass",
    "ray id:</",
    "cloudflare ray id",
    "verify you are human",
    "security check",
];

const SPA_INDICATORS: &[&str] = &[
    "__next_data__",
    "__nuxt",
    "ng-app",
    "ng-controller",
    "data-reactroot",
    "data-react-helmet",
    "_app-root",
    "app-root",
    "loading your",
    "loading...",
    "memuat...",
    "please wait",
    "initializing",
];

#[derive(Serialize)]
struct FlareSolverrRequest {
    cmd: String,
    url: String,
    #[serde(rename = "maxTimeout")]
    max_timeout: u32,
}

#[derive(Deserialize)]
struct FlareSolverrResponse {
    status: String,
    solution: Option<FlareSolverrSolution>,
}

#[derive(Deserialize)]
struct FlareSolverrSolution {
    response: String,
}

pub struct WebsiteScraper {
    http_client: reqwest::Client,
}

impl WebsiteScraper {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    pub async fn scrape(&self, url: &str) -> Result<StartupInfo, AppError> {
        let parsed_url =
            Url::parse(url).map_err(|_| AppError::InvalidUrl("URL tidak valid".to_string()))?;

        if let Some(flaresolverr_url) = std::env::var("FLARESOLVERR_URL").ok() {
            if let Some(info) = self.try_flaresolverr(&flaresolverr_url, &parsed_url).await {
                tracing::info!("FlareSolverr succeeded for {}", url);
                return Ok(info);
            }
            tracing::warn!("FlareSolverr failed for {}, falling back to direct scraping", url);
        }

        match self.try_scrape(&parsed_url).await {
            Ok(info) => {
                if self.is_content_minimal(&info) {
                    tracing::info!("Detected SPA or minimal content for {}", url);

                    #[cfg(feature = "headless")]
                    {
                        if let Some(cf_info) = self.try_cloudflare_solver(&parsed_url) {
                            if !self.is_content_minimal(&cf_info) {
                                tracing::info!("CloudflareSolver got content for {}", url);
                                return Ok(cf_info);
                            }
                        }

                        tracing::warn!("CloudflareSolver didn't help for {}, trying headless", url);

                        if let Some(headless_info) = self.try_headless_scrape(&parsed_url) {
                            if !self.is_content_minimal(&headless_info) {
                                tracing::info!("Headless scraping got better content for {}", url);
                                return Ok(headless_info);
                            }
                        }
                    }

                    tracing::warn!("All browser methods failed for {}, trying Google Cache", url);

                    if let Some(cache_info) = self.try_google_cache(&parsed_url).await {
                        if !self.is_content_minimal(&cache_info) {
                            tracing::info!("Google Cache got better content for {}", url);
                            return Ok(cache_info);
                        }
                    }
                }
                Ok(info)
            }
            Err(e) => {
                tracing::warn!("HTTP scraping failed for {}: {}", url, e);

                #[cfg(feature = "headless")]
                if let Some(info) = self.try_headless_scrape(&parsed_url) {
                    tracing::info!("Headless scraping succeeded for {}", url);
                    return Ok(info);
                }

                if let Some(cache_info) = self.try_google_cache(&parsed_url).await {
                    tracing::info!("Google Cache succeeded for {}", url);
                    return Ok(cache_info);
                }

                tracing::warn!("All scraping methods failed for {}, using URL-only fallback", url);
                Ok(self.create_fallback_info(&parsed_url, Some(e.to_string())))
            }
        }
    }

    async fn try_flaresolverr(&self, flaresolverr_url: &str, parsed_url: &Url) -> Option<StartupInfo> {
        tracing::info!("Attempting FlareSolverr for {}", parsed_url);

        let request = FlareSolverrRequest {
            cmd: "request.get".to_string(),
            url: parsed_url.to_string(),
            max_timeout: 60000,
        };

        let response = self
            .http_client
            .post(format!("{}/v1", flaresolverr_url))
            .json(&request)
            .timeout(std::time::Duration::from_secs(70))
            .send()
            .await
            .ok()?;

        let result: FlareSolverrResponse = response.json().await.ok()?;

        if result.status != "ok" {
            tracing::warn!("FlareSolverr returned non-ok status: {}", result.status);
            return None;
        }

        let html = result.solution?.response;
        self.parse_html(parsed_url.as_str(), &html).ok()
    }

    #[cfg(feature = "headless")]
    fn try_cloudflare_solver(&self, parsed_url: &Url) -> Option<StartupInfo> {
        use crate::infrastructure::cloudflare::CloudflareSolver;

        tracing::info!("Attempting CloudflareSolver for {}", parsed_url);

        let solver = CloudflareSolver::new(20);
        let result = solver.solve(parsed_url.as_str())?;

        if !result.success {
            tracing::warn!("CloudflareSolver did not succeed for {}", parsed_url);
            return None;
        }

        tracing::info!(
            "CloudflareSolver succeeded, got {} cookies",
            result.cookies.len()
        );

        self.parse_html(parsed_url.as_str(), &result.html).ok()
    }

    async fn try_google_cache(&self, parsed_url: &Url) -> Option<StartupInfo> {
        tracing::info!("Attempting Google Cache for {}", parsed_url);

        let cache_url = format!(
            "https://webcache.googleusercontent.com/search?q=cache:{}",
            urlencoding::encode(parsed_url.as_str())
        );

        let response = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.http_client
                .get(&cache_url)
                .header("User-Agent", USER_AGENTS[0])
                .send()
        ).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(e)) => {
                tracing::warn!("Google Cache request failed: {}", e);
                return None;
            }
            Err(_) => {
                tracing::warn!("Google Cache request timed out");
                return None;
            }
        };

        if !response.status().is_success() {
            tracing::warn!("Google Cache returned {}", response.status());
            return None;
        }

        let html = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            response.text()
        ).await {
            Ok(Ok(text)) => text,
            _ => {
                tracing::warn!("Google Cache body read timed out");
                return None;
            }
        };

        if html.contains("did not match any documents") || html.len() < 500 {
            tracing::warn!("Google Cache has no cached version");
            return None;
        }

        // Parse Google Cache HTML with special handling for title extraction
        self.parse_google_cache_html(parsed_url, &html)
    }

    fn parse_google_cache_html(&self, parsed_url: &Url, html: &str) -> Option<StartupInfo> {
        let document = Html::parse_document(html);

        // Google Cache wraps the original content with its own UI
        // The original page title might be in different locations:
        // 1. Inside the cached content's <title> (not Google's wrapper)
        // 2. In og:title meta tag
        // 3. In the cache header showing the original URL

        // Try to extract title from og:title (often preserved in cache)
        let og_title = Selector::parse("meta[property='og:title']").ok()
            .and_then(|sel| document.select(&sel).next())
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && !s.to_lowercase().contains("google"));

        // Try twitter:title as fallback
        let twitter_title = Selector::parse("meta[name='twitter:title']").ok()
            .and_then(|sel| document.select(&sel).next())
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && !s.to_lowercase().contains("google"));

        // Get the <title> tag but filter out Google's wrapper title
        let page_title = self.extract_title(&document)
            .filter(|t| {
                let lower = t.to_lowercase();
                !lower.contains("google search") &&
                !lower.contains("google cache") &&
                !lower.starts_with("cache:") &&
                !lower.contains("webcache.googleusercontent")
            });

        // Use domain name as fallback title
        let domain = parsed_url.host_str().unwrap_or("unknown");
        let domain_parts: Vec<&str> = domain.split('.').collect();
        let domain_name = if domain_parts.len() > 1 {
            domain_parts[domain_parts.len() - 2].to_string()
        } else {
            domain.to_string()
        };
        let fallback_title = format!("{} (dari Google Cache)", domain_name);

        // Priority: og:title > twitter:title > filtered page title > domain name
        let title = og_title
            .or(twitter_title)
            .or(page_title)
            .unwrap_or(fallback_title);

        let description = self.extract_meta_description(&document);
        let headings = self.extract_headings(&document);
        let content_summary = self.extract_content_summary(&document);

        Some(StartupInfo::new(parsed_url.to_string())
            .with_title(Some(title))
            .with_description(description)
            .with_headings(headings)
            .with_content_summary(content_summary))
    }

    fn is_content_minimal(&self, info: &StartupInfo) -> bool {
        let has_headings = !info.headings.is_empty();
        let has_content = !info.content_summary.trim().is_empty() && info.content_summary.len() > 50;
        let has_description = info.description.as_ref().map_or(false, |d| d.len() > 20);

        if has_headings && has_content {
            return false;
        }

        if !has_headings && !has_content && !has_description {
            return true;
        }

        let content_lower = info.content_summary.to_lowercase();
        let is_loading_content = SPA_INDICATORS
            .iter()
            .any(|indicator| content_lower.contains(indicator));

        if is_loading_content {
            return true;
        }

        !has_headings && !has_content
    }

    async fn try_scrape(&self, parsed_url: &Url) -> Result<StartupInfo, AppError> {
        let ua_index = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            % USER_AGENTS.len() as u64) as usize;

        let response = self
            .http_client
            .get(parsed_url.as_str())
            .header("User-Agent", USER_AGENTS[ua_index])
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
            .header("Accept-Language", "id-ID,id;q=0.9,en-US;q=0.8,en;q=0.7")
            .header("Accept-Encoding", "gzip, deflate, br")
            .header("Connection", "keep-alive")
            .header("Upgrade-Insecure-Requests", "1")
            .header("Sec-Fetch-Dest", "document")
            .header("Sec-Fetch-Mode", "navigate")
            .header("Sec-Fetch-Site", "none")
            .header("Sec-Fetch-User", "?1")
            .header("Cache-Control", "max-age=0")
            .send()
            .await
            .map_err(|e| AppError::ScrapingFailed(e.to_string()))?;

        let status = response.status();
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
            return Err(AppError::ScrapingFailed("Cloudflare or bot protection detected".to_string()));
        }

        if !status.is_success() {
            return Err(AppError::ScrapingFailed(format!("HTTP {}", status)));
        }

        let html = response
            .text()
            .await
            .map_err(|e| AppError::ScrapingFailed(e.to_string()))?;

        if html.len() < 100 {
            return Err(AppError::ScrapingFailed("Empty or minimal content".to_string()));
        }

        if self.is_cloudflare_challenge(&html) {
            return Err(AppError::ScrapingFailed("Cloudflare challenge page detected".to_string()));
        }

        self.parse_html(parsed_url.as_str(), &html)
    }

    #[cfg(feature = "headless")]
    fn try_headless_scrape(&self, parsed_url: &Url) -> Option<StartupInfo> {
        use headless_chrome::{Browser, LaunchOptions};

        tracing::info!("Attempting stealth headless scrape for {}", parsed_url);

        let stealth_args = vec![
            std::ffi::OsStr::new("--disable-blink-features=AutomationControlled"),
            std::ffi::OsStr::new("--disable-features=IsolateOrigins,site-per-process"),
            std::ffi::OsStr::new("--disable-site-isolation-trials"),
            std::ffi::OsStr::new("--disable-web-security"),
            std::ffi::OsStr::new("--disable-features=BlockInsecurePrivateNetworkRequests"),
            std::ffi::OsStr::new("--no-first-run"),
            std::ffi::OsStr::new("--no-default-browser-check"),
            std::ffi::OsStr::new("--disable-default-apps"),
            std::ffi::OsStr::new("--disable-popup-blocking"),
            std::ffi::OsStr::new("--disable-extensions"),
            std::ffi::OsStr::new("--disable-component-extensions-with-background-pages"),
            std::ffi::OsStr::new("--disable-background-networking"),
            std::ffi::OsStr::new("--disable-sync"),
            std::ffi::OsStr::new("--disable-translate"),
            std::ffi::OsStr::new("--metrics-recording-only"),
            std::ffi::OsStr::new("--mute-audio"),
            std::ffi::OsStr::new("--no-pings"),
            std::ffi::OsStr::new("--window-size=1920,1080"),
            std::ffi::OsStr::new("--start-maximized"),
            std::ffi::OsStr::new("--lang=id-ID"),
        ];

        let use_visible_browser = std::env::var("VISIBLE_BROWSER").is_ok();

        let launch_options = LaunchOptions::default_builder()
            .headless(!use_visible_browser)
            .sandbox(false)
            .idle_browser_timeout(std::time::Duration::from_secs(90))
            .args(stealth_args)
            .build()
            .ok()?;

        if use_visible_browser {
            tracing::info!("Using visible browser mode for better Cloudflare bypass");
        }

        let browser = Browser::new(launch_options).ok()?;
        let tab = browser.new_tab().ok()?;

        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";
        let _ = tab.set_user_agent(ua, None, None);

        let stealth_js = r#"
            Object.defineProperty(navigator, 'webdriver', {
                get: () => undefined,
                configurable: true
            });
            delete navigator.__proto__.webdriver;

            Object.defineProperty(navigator, 'plugins', {
                get: () => {
                    const plugins = [
                        { name: 'Chrome PDF Plugin', filename: 'internal-pdf-viewer' },
                        { name: 'Chrome PDF Viewer', filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai' },
                        { name: 'Native Client', filename: 'internal-nacl-plugin' }
                    ];
                    plugins.length = 3;
                    return plugins;
                }
            });

            Object.defineProperty(navigator, 'languages', {
                get: () => ['id-ID', 'id', 'en-US', 'en']
            });

            window.chrome = {
                runtime: {
                    PlatformOs: { MAC: 'mac', WIN: 'win', ANDROID: 'android', CROS: 'cros', LINUX: 'linux', OPENBSD: 'openbsd' },
                    PlatformArch: { ARM: 'arm', X86_32: 'x86-32', X86_64: 'x86-64' },
                    PlatformNaclArch: { ARM: 'arm', X86_32: 'x86-32', X86_64: 'x86-64' },
                    RequestUpdateCheckStatus: { THROTTLED: 'throttled', NO_UPDATE: 'no_update', UPDATE_AVAILABLE: 'update_available' },
                    OnInstalledReason: { INSTALL: 'install', UPDATE: 'update', CHROME_UPDATE: 'chrome_update', SHARED_MODULE_UPDATE: 'shared_module_update' },
                    OnRestartRequiredReason: { APP_UPDATE: 'app_update', OS_UPDATE: 'os_update', PERIODIC: 'periodic' }
                }
            };

            Object.defineProperty(navigator, 'permissions', {
                get: () => ({
                    query: (params) => Promise.resolve({ state: 'granted', onchange: null })
                })
            });

            const originalQuery = window.navigator.permissions.query;
            window.navigator.permissions.query = (parameters) => (
                parameters.name === 'notifications' ?
                    Promise.resolve({ state: Notification.permission }) :
                    originalQuery(parameters)
            );

            Object.defineProperty(navigator, 'maxTouchPoints', { get: () => 0 });
            Object.defineProperty(navigator, 'hardwareConcurrency', { get: () => 8 });
            Object.defineProperty(navigator, 'deviceMemory', { get: () => 8 });

            const getParameter = WebGLRenderingContext.prototype.getParameter;
            WebGLRenderingContext.prototype.getParameter = function(parameter) {
                if (parameter === 37445) return 'Intel Inc.';
                if (parameter === 37446) return 'Intel Iris OpenGL Engine';
                return getParameter.call(this, parameter);
            };
        "#;

        use headless_chrome::protocol::cdp::Page;
        let add_script = Page::AddScriptToEvaluateOnNewDocument {
            source: stealth_js.to_string(),
            world_name: None,
            include_command_line_api: None,
            run_immediately: None,
        };
        let _ = tab.call_method(add_script);

        tab.navigate_to(parsed_url.as_str()).ok()?;

        if tab.wait_until_navigated().is_err() {
            tracing::warn!("Navigation timeout for {}", parsed_url);
        }

        std::thread::sleep(std::time::Duration::from_secs(3));

        let html = tab.get_content().ok()?;

        if self.is_cloudflare_challenge(&html) {
            tracing::info!("Cloudflare detected, waiting for challenge to auto-solve...");

            for attempt in 1..=4 {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let html = tab.get_content().ok()?;

                if !self.is_cloudflare_challenge(&html) {
                    tracing::info!("Cloudflare bypassed after {} attempts", attempt);
                    return self.parse_html(parsed_url.as_str(), &html).ok();
                }

                tracing::info!("Cloudflare still present, attempt {}/4", attempt);
            }

            tracing::warn!("Cloudflare challenge not bypassed for {}", parsed_url);
            return None;
        }

        if self.is_spa_loading(&html) {
            tracing::info!("SPA still loading, waiting for client-side render...");
            std::thread::sleep(std::time::Duration::from_secs(4));
            let html = tab.get_content().ok()?;
            return self.parse_html(parsed_url.as_str(), &html).ok();
        }

        self.parse_html(parsed_url.as_str(), &html).ok()
    }

    fn is_spa_loading(&self, html: &str) -> bool {
        let lower = html.to_lowercase();
        let has_spa_marker = SPA_INDICATORS.iter().any(|i| lower.contains(i));
        let has_minimal_body = {
            let body_start = lower.find("<body");
            let body_end = lower.find("</body>");
            if let (Some(start), Some(end)) = (body_start, body_end) {
                let body_content = &html[start..end];
                let text_content: String = body_content
                    .chars()
                    .filter(|c| c.is_alphanumeric() || c.is_whitespace())
                    .collect();
                text_content.split_whitespace().count() < 30
            } else {
                false
            }
        };
        has_spa_marker || has_minimal_body
    }

    fn is_cloudflare_challenge(&self, html: &str) -> bool {
        let lower = html.to_lowercase();
        CLOUDFLARE_CHALLENGE_INDICATORS.iter().any(|indicator| lower.contains(indicator))
    }

    fn create_fallback_info(&self, url: &Url, error_reason: Option<String>) -> StartupInfo {
        let domain = url.host_str().unwrap_or("unknown");
        let path = url.path();

        let domain_parts: Vec<&str> = domain.split('.').collect();
        let main_name = if domain_parts.len() > 1 {
            domain_parts[domain_parts.len() - 2]
        } else {
            domain
        };

        let subdomain = if domain_parts.len() > 2 {
            Some(domain_parts[..domain_parts.len() - 2].join("."))
        } else {
            None
        };

        let path_hints: Vec<&str> = path
            .split('/')
            .filter(|s| !s.is_empty() && s.len() > 2)
            .take(3)
            .collect();

        let query_hints: Vec<String> = url
            .query_pairs()
            .filter(|(k, _)| !k.is_empty())
            .take(3)
            .map(|(k, v)| {
                if v.is_empty() {
                    k.to_string()
                } else {
                    format!("{}={}", k, v)
                }
            })
            .collect();

        let reason = error_reason.unwrap_or_else(|| "tidak dapat diakses".to_string());
        let is_cloudflare = reason.to_lowercase().contains("cloudflare");

        let protection_note = if is_cloudflare {
            "Website ini dilindungi Cloudflare (takut banget di-scrape, pasti ada yang disembunyiin)"
        } else {
            "Website tidak dapat diakses"
        };

        let mut description_parts = vec![format!("Startup dengan domain {}", domain)];

        if let Some(ref sub) = subdomain {
            description_parts.push(format!("subdomain: {}", sub));
        }

        if !path_hints.is_empty() {
            description_parts.push(format!("path: /{}", path_hints.join("/")));
        }

        if !query_hints.is_empty() {
            description_parts.push(format!("params: {}", query_hints.join(", ")));
        }

        description_parts.push(format!("({})", protection_note));

        let tld = domain_parts.last().unwrap_or(&"com");
        let tld_roast = match *tld {
            "io" => "pakai .io biar keliatan tech-savvy padahal cuma modal domain doang",
            "co" => "pakai .co karena .com udah diambil orang, literally second choice",
            "id" => "at least pakai domain lokal, nasionalis dikit lah",
            "xyz" => "pakai .xyz karena bokek, domain paling murah sedunia",
            "app" => "pakai .app biar keliatan modern, padahal belum tentu ada app-nya",
            "dev" => "pakai .dev, developer wannabe detected",
            "ai" => "pakai .ai biar dikira startup AI, padahal cuma wrapper ChatGPT",
            "tech" => "pakai .tech, generic banget kayak ide startupnya",
            _ => "domain biasa aja",
        };

        let mut headings = vec![
            format!("Domain: {}", domain),
            format!("TLD Analysis: {}", tld_roast),
        ];

        if let Some(ref sub) = subdomain {
            headings.push(format!("Subdomain: {} (ribet amat URL-nya)", sub));
        }

        let content = format!(
            "Website {} tidak bisa di-scrape ({}). \
            Analisis URL: domain={}, TLD=.{} ({}), \
            path={}, subdomain={}. \
            Roasting tetap bisa dilakukan berdasarkan: nama domain yang {}, \
            struktur URL, dan pilihan TLD mereka.",
            main_name,
            reason,
            domain,
            tld,
            tld_roast,
            if path_hints.is_empty() { "/" } else { path },
            subdomain.as_deref().unwrap_or("tidak ada"),
            if main_name.len() > 10 { "kepanjangan" } else { "sok singkat" }
        );

        StartupInfo::new(url.to_string())
            .with_title(Some(format!("{} - [{}]", main_name.to_uppercase(), if is_cloudflare { "Cloudflare Protected" } else { "Unreachable" })))
            .with_description(Some(description_parts.join(", ")))
            .with_headings(headings)
            .with_content_summary(content)
    }

    fn parse_html(&self, url: &str, html: &str) -> Result<StartupInfo, AppError> {
        let document = Html::parse_document(html);

        let title = self.extract_title(&document);
        let description = self.extract_meta_description(&document);
        let headings = self.extract_headings(&document);
        let content_summary = self.extract_content_summary(&document);

        Ok(StartupInfo::new(url.to_string())
            .with_title(title)
            .with_description(description)
            .with_headings(headings)
            .with_content_summary(content_summary))
    }

    fn extract_title(&self, document: &Html) -> Option<String> {
        let selector = Selector::parse("title").ok()?;
        document
            .select(&selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
    }

    fn extract_meta_description(&self, document: &Html) -> Option<String> {
        let selector = Selector::parse("meta[name='description']").ok()?;
        document
            .select(&selector)
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.trim().to_string())
    }

    fn extract_headings(&self, document: &Html) -> Vec<String> {
        let selectors = ["h1", "h2", "h3"];
        let mut headings = Vec::new();

        for sel in selectors {
            if let Ok(selector) = Selector::parse(sel) {
                for element in document.select(&selector).take(3) {
                    let text = element.text().collect::<String>().trim().to_string();
                    if !text.is_empty() && text.len() < 200 {
                        headings.push(text);
                    }
                }
            }
        }

        headings.truncate(10);
        headings
    }

    fn extract_content_summary(&self, document: &Html) -> String {
        let selector = Selector::parse("p").ok();
        let mut content = String::new();

        if let Some(sel) = selector {
            for element in document.select(&sel).take(5) {
                let text = element.text().collect::<String>().trim().to_string();
                if !text.is_empty() && text.len() > 20 {
                    content.push_str(&text);
                    content.push(' ');
                }
                if content.len() > 500 {
                    break;
                }
            }
        }

        if content.len() > 500 {
            content.truncate(500);
            content.push_str("...");
        }

        content
    }
}

impl Default for WebsiteScraper {
    fn default() -> Self {
        Self::new()
    }
}
