use headless_chrome::protocol::cdp::{Emulation, Input, Page};
use headless_chrome::{Browser, LaunchOptions, Tab};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

static BROWSER_POOL: Mutex<Option<Arc<Browser>>> = Mutex::new(None);

const STEALTH_JS: &str = r#"
(() => {
    // Remove webdriver property
    Object.defineProperty(navigator, 'webdriver', {
        get: () => undefined,
        configurable: true
    });
    delete navigator.__proto__.webdriver;

    // Fake plugins
    Object.defineProperty(navigator, 'plugins', {
        get: () => {
            const plugins = [
                { name: 'Chrome PDF Plugin', filename: 'internal-pdf-viewer', description: 'Portable Document Format' },
                { name: 'Chrome PDF Viewer', filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai', description: '' },
                { name: 'Native Client', filename: 'internal-nacl-plugin', description: '' }
            ];
            plugins.length = 3;
            plugins.item = (i) => plugins[i];
            plugins.namedItem = (name) => plugins.find(p => p.name === name);
            plugins.refresh = () => {};
            return plugins;
        }
    });

    // Fake languages
    Object.defineProperty(navigator, 'languages', {
        get: () => ['en-US', 'en', 'id-ID', 'id']
    });

    // Fake chrome runtime
    window.chrome = {
        runtime: {
            PlatformOs: { MAC: 'mac', WIN: 'win', ANDROID: 'android', CROS: 'cros', LINUX: 'linux', OPENBSD: 'openbsd' },
            PlatformArch: { ARM: 'arm', X86_32: 'x86-32', X86_64: 'x86-64' },
            PlatformNaclArch: { ARM: 'arm', X86_32: 'x86-32', X86_64: 'x86-64' },
            RequestUpdateCheckStatus: { THROTTLED: 'throttled', NO_UPDATE: 'no_update', UPDATE_AVAILABLE: 'update_available' },
            OnInstalledReason: { INSTALL: 'install', UPDATE: 'update', CHROME_UPDATE: 'chrome_update', SHARED_MODULE_UPDATE: 'shared_module_update' },
            OnRestartRequiredReason: { APP_UPDATE: 'app_update', OS_UPDATE: 'os_update', PERIODIC: 'periodic' }
        },
        csi: () => {},
        loadTimes: () => ({
            commitLoadTime: Date.now() / 1000,
            connectionInfo: 'h2',
            finishDocumentLoadTime: Date.now() / 1000,
            finishLoadTime: Date.now() / 1000,
            firstPaintAfterLoadTime: 0,
            firstPaintTime: Date.now() / 1000,
            navigationType: 'navigate',
            npnNegotiatedProtocol: 'h2',
            requestTime: Date.now() / 1000,
            startLoadTime: Date.now() / 1000,
            wasAlternateProtocolAvailable: false,
            wasFetchedViaSpdy: true,
            wasNpnNegotiated: true
        })
    };

    // Fake permissions
    const originalQuery = window.navigator.permissions.query;
    window.navigator.permissions.query = (parameters) => (
        parameters.name === 'notifications' ?
            Promise.resolve({ state: Notification.permission }) :
            originalQuery(parameters)
    );

    // Hardware properties
    Object.defineProperty(navigator, 'hardwareConcurrency', { get: () => 8 });
    Object.defineProperty(navigator, 'deviceMemory', { get: () => 8 });
    Object.defineProperty(navigator, 'maxTouchPoints', { get: () => 0 });

    // WebGL fingerprint
    const getParameter = WebGLRenderingContext.prototype.getParameter;
    WebGLRenderingContext.prototype.getParameter = function(parameter) {
        if (parameter === 37445) return 'Intel Inc.';
        if (parameter === 37446) return 'Intel Iris OpenGL Engine';
        return getParameter.call(this, parameter);
    };

    // Canvas fingerprint randomization
    const originalToDataURL = HTMLCanvasElement.prototype.toDataURL;
    HTMLCanvasElement.prototype.toDataURL = function(type) {
        if (type === 'image/png' && this.width === 16 && this.height === 16) {
            return originalToDataURL.apply(this, arguments);
        }
        const context = this.getContext('2d');
        if (context) {
            const imageData = context.getImageData(0, 0, this.width, this.height);
            for (let i = 0; i < imageData.data.length; i += 4) {
                imageData.data[i] ^= (Math.random() * 2) | 0;
            }
            context.putImageData(imageData, 0, 0);
        }
        return originalToDataURL.apply(this, arguments);
    };

    // Automation detection
    Object.defineProperty(navigator, 'platform', { get: () => 'MacIntel' });
    Object.defineProperty(navigator, 'vendor', { get: () => 'Google Inc.' });
    Object.defineProperty(navigator, 'appVersion', {
        get: () => '5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36'
    });

    // Remove automation flags from window
    delete window.cdc_adoQpoasnfa76pfcZLmcfl_Array;
    delete window.cdc_adoQpoasnfa76pfcZLmcfl_Promise;
    delete window.cdc_adoQpoasnfa76pfcZLmcfl_Symbol;

    console.log('Stealth mode activated');
})();
"#;

const CLOUDFLARE_CHALLENGE_MARKERS: &[&str] = &[
    "cf-browser-verification",
    "cf-challenge-running",
    "challenge-platform",
    "cf-turnstile",
    "challenges.cloudflare.com",
    "ray id:",
    "_cf_chl",
    "checking your browser before",
    "verify you are human",
    "<title>just a moment</title>",
];

pub struct CloudflareSolver {
    max_timeout: Duration,
}

pub struct SolveResult {
    pub html: String,
    pub cookies: Vec<(String, String)>,
    pub success: bool,
}

impl CloudflareSolver {
    pub fn new(max_timeout_secs: u64) -> Self {
        Self {
            max_timeout: Duration::from_secs(max_timeout_secs),
        }
    }

    pub fn solve(&self, url: &str) -> Option<SolveResult> {
        tracing::info!("CloudflareSolver: Starting solve for {}", url);

        let browser = self.get_or_create_browser()?;
        let tab = browser.new_tab().ok()?;

        self.setup_stealth(&tab)?;
        let result = self.navigate_and_solve(&tab, url);

        let _ = tab.close(true);

        result
    }

    fn get_or_create_browser(&self) -> Option<Arc<Browser>> {
        let mut pool = BROWSER_POOL.lock().ok()?;

        if let Some(ref browser) = *pool {
            if browser.get_version().is_ok() {
                tracing::info!("CloudflareSolver: Reusing existing browser");
                return Some(browser.clone());
            }
        }

        tracing::info!("CloudflareSolver: Creating new browser");
        let browser = Arc::new(self.create_stealth_browser()?);
        *pool = Some(browser.clone());
        Some(browser)
    }

    fn create_stealth_browser(&self) -> Option<Browser> {
        let args = vec![
            std::ffi::OsStr::new("--disable-blink-features=AutomationControlled"),
            std::ffi::OsStr::new("--disable-features=IsolateOrigins,site-per-process"),
            std::ffi::OsStr::new("--disable-site-isolation-trials"),
            std::ffi::OsStr::new("--disable-web-security"),
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
            std::ffi::OsStr::new("--disable-hang-monitor"),
            std::ffi::OsStr::new("--disable-prompt-on-repost"),
            std::ffi::OsStr::new("--disable-client-side-phishing-detection"),
            std::ffi::OsStr::new("--disable-component-update"),
            std::ffi::OsStr::new("--disable-domain-reliability"),
            std::ffi::OsStr::new("--disable-features=AudioServiceOutOfProcess"),
            std::ffi::OsStr::new("--disable-ipc-flooding-protection"),
            std::ffi::OsStr::new("--disable-renderer-backgrounding"),
            std::ffi::OsStr::new("--enable-features=NetworkService,NetworkServiceInProcess"),
            std::ffi::OsStr::new("--force-color-profile=srgb"),
            std::ffi::OsStr::new("--window-size=1920,1080"),
            std::ffi::OsStr::new("--start-maximized"),
            std::ffi::OsStr::new("--lang=en-US"),
        ];

        let launch_options = LaunchOptions::default_builder()
            .headless(true)
            .sandbox(false)
            .idle_browser_timeout(Duration::from_secs(120))
            .args(args)
            .build()
            .ok()?;

        tracing::info!("CloudflareSolver: Launching headless browser");
        Browser::new(launch_options).ok()
    }

    fn setup_stealth(&self, tab: &Arc<Tab>) -> Option<()> {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";
        tab.set_user_agent(ua, None, None).ok()?;

        let viewport = Emulation::SetDeviceMetricsOverride {
            width: 1920,
            height: 1080,
            device_scale_factor: 1.0,
            mobile: false,
            scale: None,
            screen_width: Some(1920),
            screen_height: Some(1080),
            position_x: None,
            position_y: None,
            dont_set_visible_size: None,
            screen_orientation: None,
            viewport: None,
            display_feature: None,
            device_posture: None,
        };
        let _ = tab.call_method(viewport);

        let add_script = Page::AddScriptToEvaluateOnNewDocument {
            source: STEALTH_JS.to_string(),
            world_name: None,
            include_command_line_api: None,
            run_immediately: None,
        };
        tab.call_method(add_script).ok()?;

        Some(())
    }

    fn navigate_and_solve(&self, tab: &Arc<Tab>, url: &str) -> Option<SolveResult> {
        tracing::info!("CloudflareSolver: Navigating to {}", url);
        tab.navigate_to(url).ok()?;

        if tab.wait_until_navigated().is_err() {
            tracing::warn!("CloudflareSolver: Navigation timeout");
        }

        std::thread::sleep(Duration::from_secs(2));

        let start = Instant::now();
        let mut attempt = 0;
        let mut clicked = false;

        while start.elapsed() < self.max_timeout {
            attempt += 1;
            let html = tab.get_content().ok()?;

            if !self.is_challenge_page(&html) {
                tracing::info!("CloudflareSolver: Challenge solved after {} attempts", attempt);
                let cookies = self.extract_cookies(tab);
                return Some(SolveResult {
                    html,
                    cookies,
                    success: true,
                });
            }

            if !clicked && attempt >= 2 {
                tracing::info!("CloudflareSolver: Attempting to click challenge checkbox");
                if self.try_click_challenge(tab) {
                    clicked = true;
                    tracing::info!("CloudflareSolver: Click sent, waiting for verification");
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
            }

            tracing::info!(
                "CloudflareSolver: Challenge still present, attempt {} ({:.1}s elapsed)",
                attempt,
                start.elapsed().as_secs_f32()
            );

            std::thread::sleep(Duration::from_secs(2));
        }

        tracing::warn!("CloudflareSolver: Timeout after {} attempts", attempt);
        None
    }

    fn try_click_challenge(&self, tab: &Arc<Tab>) -> bool {
        let find_challenge_js = r#"
            (() => {
                // Cloudflare Turnstile iframe has specific patterns
                const iframes = document.querySelectorAll('iframe');
                for (const iframe of iframes) {
                    const src = iframe.src || '';
                    if (src.includes('challenges.cloudflare.com') ||
                        src.includes('turnstile') ||
                        iframe.id.includes('turnstile') ||
                        iframe.className.includes('turnstile')) {
                        const rect = iframe.getBoundingClientRect();
                        // Checkbox is typically 20-30px from left, centered vertically
                        return {
                            found: true,
                            x: rect.x + 28,
                            y: rect.y + rect.height / 2,
                            type: 'turnstile-iframe',
                            width: rect.width,
                            height: rect.height
                        };
                    }
                }

                // Look for cf-turnstile container
                const turnstile = document.querySelector('.cf-turnstile') ||
                                  document.querySelector('[class*="cf-turnstile"]') ||
                                  document.querySelector('div[data-sitekey]');
                if (turnstile) {
                    const rect = turnstile.getBoundingClientRect();
                    return {
                        found: true,
                        x: rect.x + 28,
                        y: rect.y + rect.height / 2,
                        type: 'turnstile-div'
                    };
                }

                // Look for challenge-form or challenge-stage
                const challengeForm = document.querySelector('#challenge-form') ||
                                     document.querySelector('#challenge-stage') ||
                                     document.querySelector('.challenge-form');
                if (challengeForm) {
                    const rect = challengeForm.getBoundingClientRect();
                    return {
                        found: true,
                        x: rect.x + rect.width / 2,
                        y: rect.y + rect.height / 2,
                        type: 'challenge-form'
                    };
                }

                // Look for any large centered element that could be the challenge
                const main = document.querySelector('main') || document.body;
                const mainRect = main.getBoundingClientRect();
                const centerX = mainRect.x + mainRect.width / 2;
                const centerY = mainRect.y + mainRect.height / 2;

                // Check for any interactive element near center
                for (let offsetY = -100; offsetY <= 100; offsetY += 50) {
                    const el = document.elementFromPoint(centerX, centerY + offsetY);
                    if (el && (el.tagName === 'INPUT' || el.tagName === 'BUTTON' ||
                               el.role === 'checkbox' || el.role === 'button')) {
                        const rect = el.getBoundingClientRect();
                        return {
                            found: true,
                            x: rect.x + rect.width / 2,
                            y: rect.y + rect.height / 2,
                            type: 'interactive-element'
                        };
                    }
                }

                // Get page dimensions for fallback
                return {
                    found: false,
                    pageWidth: window.innerWidth,
                    pageHeight: window.innerHeight,
                    bodyRect: document.body.getBoundingClientRect()
                };
            })()
        "#;

        if let Ok(result) = tab.evaluate(find_challenge_js, false) {
            if let Some(obj) = result.value {
                if let Some(found) = obj.get("found").and_then(|v| v.as_bool()) {
                    if found {
                        let x = obj.get("x").and_then(|v| v.as_f64()).unwrap_or(960.0);
                        let y = obj.get("y").and_then(|v| v.as_f64()).unwrap_or(400.0);
                        let typ = obj.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");

                        tracing::info!("CloudflareSolver: Found '{}' at ({}, {})", typ, x, y);
                        self.human_click(tab, x, y);
                        return true;
                    } else {
                        let pw = obj.get("pageWidth").and_then(|v| v.as_f64()).unwrap_or(1920.0);
                        let ph = obj.get("pageHeight").and_then(|v| v.as_f64()).unwrap_or(1080.0);
                        tracing::info!("CloudflareSolver: Page size {}x{}, clicking likely checkbox area", pw, ph);

                        self.human_click(tab, pw / 2.0 - 100.0, ph / 2.0);
                        return true;
                    }
                }
            }
        }

        tracing::info!("CloudflareSolver: Fallback - clicking common Turnstile position");
        self.human_click(tab, 200.0, 400.0);
        true
    }

    fn is_challenge_page(&self, html: &str) -> bool {
        let lower = html.to_lowercase();

        let has_cloudflare_marker = CLOUDFLARE_CHALLENGE_MARKERS.iter().any(|m| lower.contains(m));

        if !has_cloudflare_marker {
            return false;
        }

        let has_real_content = {
            let has_h1 = lower.contains("<h1") && !lower.contains("<h1>just a moment");
            let has_main = lower.contains("<main") || lower.contains("id=\"root\"") || lower.contains("id=\"app\"");
            let has_nav = lower.contains("<nav");
            let has_article = lower.contains("<article");
            let body_text_len = html.len() > 5000;

            (has_h1 || has_main || has_nav || has_article) && body_text_len
        };

        !has_real_content
    }


    fn human_click(&self, tab: &Arc<Tab>, x: f64, y: f64) {
        let base_x = x + (rand_f64() * 10.0 - 5.0);
        let base_y = y + (rand_f64() * 10.0 - 5.0);

        let steps = 5 + (rand_f64() * 5.0) as i32;
        let start_x = base_x - 100.0 + rand_f64() * 50.0;
        let start_y = base_y - 50.0 + rand_f64() * 30.0;

        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let ease_t = t * t * (3.0 - 2.0 * t);

            let current_x = start_x + (base_x - start_x) * ease_t + rand_f64() * 2.0 - 1.0;
            let current_y = start_y + (base_y - start_y) * ease_t + rand_f64() * 2.0 - 1.0;

            let move_event = Input::DispatchMouseEvent {
                Type: Input::DispatchMouseEventTypeOption::MouseMoved,
                x: current_x,
                y: current_y,
                modifiers: None,
                timestamp: None,
                button: None,
                buttons: None,
                click_count: None,
                force: None,
                tangential_pressure: None,
                tilt_x: None,
                tilt_y: None,
                twist: None,
                delta_x: None,
                delta_y: None,
                pointer_Type: None,
            };
            let _ = tab.call_method(move_event);

            std::thread::sleep(Duration::from_millis(20 + (rand_f64() * 30.0) as u64));
        }

        std::thread::sleep(Duration::from_millis(100 + (rand_f64() * 200.0) as u64));

        let click_down = Input::DispatchMouseEvent {
            Type: Input::DispatchMouseEventTypeOption::MousePressed,
            x: base_x,
            y: base_y,
            modifiers: None,
            timestamp: None,
            button: Some(Input::MouseButton::Left),
            buttons: Some(1),
            click_count: Some(1),
            force: None,
            tangential_pressure: None,
            tilt_x: None,
            tilt_y: None,
            twist: None,
            delta_x: None,
            delta_y: None,
            pointer_Type: None,
        };
        let _ = tab.call_method(click_down);

        std::thread::sleep(Duration::from_millis(50 + (rand_f64() * 100.0) as u64));

        let click_up = Input::DispatchMouseEvent {
            Type: Input::DispatchMouseEventTypeOption::MouseReleased,
            x: base_x,
            y: base_y,
            modifiers: None,
            timestamp: None,
            button: Some(Input::MouseButton::Left),
            buttons: Some(0),
            click_count: Some(1),
            force: None,
            tangential_pressure: None,
            tilt_x: None,
            tilt_y: None,
            twist: None,
            delta_x: None,
            delta_y: None,
            pointer_Type: None,
        };
        let _ = tab.call_method(click_up);

        tracing::info!("CloudflareSolver: Clicked at ({}, {})", base_x, base_y);
    }

    fn extract_cookies(&self, tab: &Arc<Tab>) -> Vec<(String, String)> {
        let get_cookies_js = r#"
            document.cookie.split(';').map(c => {
                const [name, ...rest] = c.trim().split('=');
                return { name, value: rest.join('=') };
            })
        "#;

        if let Ok(result) = tab.evaluate(get_cookies_js, false) {
            if let Some(arr) = result.value.and_then(|v| v.as_array().cloned()) {
                return arr
                    .iter()
                    .filter_map(|c| {
                        let name = c.get("name")?.as_str()?.to_string();
                        let value = c.get("value")?.as_str()?.to_string();
                        Some((name, value))
                    })
                    .collect();
            }
        }
        vec![]
    }
}

fn rand_f64() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (nanos as f64 % 1000.0) / 1000.0
}

impl Default for CloudflareSolver {
    fn default() -> Self {
        Self::new(60)
    }
}
