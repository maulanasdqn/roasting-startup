use axum::{
    extract::Query,
    response::{Html, IntoResponse},
    routing::{get, post},
    Form, Router,
};
use leptos::prelude::*;
use leptos_axum::{generate_route_list, handle_server_fns_with_context, LeptosRoutes};
use roasting_app::AppContext;
use roasting_ui::pages::GenerateRoastFn;
use roasting_ui::App;
use serde::Deserialize;
use tower_http::compression::CompressionLayer;

#[derive(Deserialize)]
struct RoastForm {
    url: String,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let conf = get_configuration(Some("Cargo.toml")).expect("Failed to load Leptos config");
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;

    let app_context = AppContext::from_env();

    // Pre-initialize local LLM model at startup (downloads model on first run)
    #[cfg(feature = "local-llm")]
    if std::env::var("USE_LOCAL_LLM").is_ok() {
        tracing::info!("Pre-initializing local LLM model (this may take a while on first run)...");
        match roasting_app::infrastructure::local_llm::LocalLlm::get_or_init().await {
            Ok(_) => tracing::info!("Local LLM model ready!"),
            Err(e) => {
                tracing::error!("Failed to initialize local LLM: {}", e);
                std::process::exit(1);
            }
        }
    }

    let routes = generate_route_list(App);

    server_fn::axum::register_explicit::<GenerateRoastFn>();
    tracing::info!("Registered server function: GenerateRoastFn");

    let app = Router::new()
        .route("/roast", get({
            let ctx = app_context.clone();
            move |query: Query<RoastForm>| {
                let ctx = ctx.clone();
                async move {
                    handle_roast_form(ctx, query.0).await
                }
            }
        }).post({
            let ctx = app_context.clone();
            move |form: Form<RoastForm>| {
                let ctx = ctx.clone();
                async move {
                    handle_roast_form(ctx, form.0).await
                }
            }
        }))
        .route("/api/{*fn_name}", post({
            let ctx = app_context.clone();
            move |req| {
                let ctx = ctx.clone();
                async move {
                    handle_server_fns_with_context(
                        move || provide_context(ctx.clone()),
                        req
                    ).await
                }
            }
        }))
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let ctx = app_context.clone();
                move || provide_context(ctx.clone())
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .layer(CompressionLayer::new())
        .with_state(leptos_options);

    tracing::info!("Listening on http://{}", addr);
    tracing::info!(
        "Security: Rate limit 5/min, 20/hour. Daily limit: {} requests",
        app_context.cost_tracker.get_remaining_requests()
    );

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");

    axum::serve(listener, app.into_make_service())
        .await
        .expect("Server error");
}

async fn handle_roast_form(ctx: AppContext, form: RoastForm) -> impl IntoResponse {
    use roasting_app::infrastructure::security::InputSanitizer;
    use std::net::{IpAddr, Ipv4Addr};

    let client_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    if let Err(e) = ctx.rate_limiter.check_rate_limit(client_ip) {
        return Html(render_error_page(&e.message_id()));
    }

    if let Err(e) = ctx.cost_tracker.check_and_increment() {
        return Html(render_error_page(&e.message_id()));
    }

    let validated_url = match InputSanitizer::validate_url(&form.url) {
        Ok(url) => url,
        Err(e) => return Html(render_error_page(&e.user_message())),
    };

    match ctx.generate_roast.execute(validated_url).await {
        Ok(roast) => Html(render_result_page(&roast.startup_name, &roast.roast_text, &form.url)),
        Err(e) => Html(render_error_page(&e.user_message())),
    }
}

fn render_result_page(startup_name: &str, roast_text: &str, url: &str) -> String {
    let html_content = simple_markdown_to_html(roast_text);
    let encoded_url = urlencoding::encode(url);
    format!(r#"<!DOCTYPE html>
<html lang="id">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Roasting: {startup_name}</title>
    <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🔥</text></svg>">
    <style>{CSS}</style>
    <script>history.replaceState(null, '', '/roast?url={encoded_url}');</script>
</head>
<body>
    <main class="container">
        <div class="roast">
            <h2 class="roast__title">Roasting: {startup_name}</h2>
            <div class="roast__content">{html_content}</div>
            <div class="roast__actions">
                <a href="/" class="roast__button--primary" style="text-decoration:none;display:inline-block;">Roast Lagi!</a>
            </div>
        </div>
    </main>
</body>
</html>"#, startup_name = startup_name, html_content = html_content, CSS = CSS, encoded_url = encoded_url)
}

fn render_error_page(message: &str) -> String {
    format!(r#"<!DOCTYPE html>
<html lang="id">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Error - Roasting Startup</title>
    <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🔥</text></svg>">
    <style>{CSS}</style>
</head>
<body>
    <main class="container">
        <div class="error">
            <p class="error__title">Yah, error nih!</p>
            <p class="error__message">{message}</p>
            <a href="/" class="error__retry" style="text-decoration:none;display:inline-block;margin-top:1rem;">Coba Lagi</a>
        </div>
    </main>
</body>
</html>"#, message = message, CSS = CSS)
}

fn simple_markdown_to_html(text: &str) -> String {
    let mut result = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let processed = line.replace("**", "<strong>").replace("__", "<strong>");
        let processed = fix_strong_tags(&processed);
        let processed = processed.replace("*", "<em>").replace("_", "<em>");
        let processed = fix_em_tags(&processed);

        if line.starts_with("# ") {
            result.push_str(&format!("<h3>{}</h3>", &processed[2..]));
        } else if line.starts_with("## ") {
            result.push_str(&format!("<h4>{}</h4>", &processed[3..]));
        } else if line.starts_with("- ") || line.starts_with("* ") {
            result.push_str(&format!("<li>{}</li>", &processed[2..]));
        } else {
            result.push_str(&format!("<p>{}</p>", processed));
        }
    }
    result
}

fn fix_strong_tags(text: &str) -> String {
    let count = text.matches("<strong>").count();
    let mut result = text.to_string();
    for i in 0..count {
        if i % 2 == 1 {
            result = result.replacen("<strong>", "</strong>", 1);
        }
    }
    result
}

fn fix_em_tags(text: &str) -> String {
    let count = text.matches("<em>").count();
    let mut result = text.to_string();
    for i in 0..count {
        if i % 2 == 1 {
            result = result.replacen("<em>", "</em>", 1);
        }
    }
    result
}

const CSS: &str = r#"
:root {
    --base: #faf4ed;
    --surface: #fffaf3;
    --overlay: #f2e9e1;
    --muted: #9893a5;
    --subtle: #797593;
    --text: #575279;
    --love: #b4637a;
    --gold: #ea9d34;
    --pine: #286983;
    --foam: #56949f;
}
* { box-sizing: border-box; margin: 0; padding: 0; }
body {
    font-family: 'Inter', -apple-system, sans-serif;
    background: var(--base);
    color: var(--text);
    min-height: 100vh;
}
.container { max-width: 800px; margin: 0 auto; padding: 1.5rem; }
.roast {
    background: var(--surface); border: 2px solid var(--overlay);
    border-radius: 12px; padding: 1.5rem; margin: 2rem 0;
}
.roast__title { color: var(--love); font-size: 1.4rem; margin-bottom: 1rem; padding-bottom: 0.75rem; border-bottom: 2px solid var(--overlay); }
.roast__content { line-height: 1.8; font-size: 1.05rem; }
.roast__content p { margin-bottom: 1rem; }
.roast__content strong { font-weight: 700; color: var(--love); }
.roast__content em { font-style: italic; }
.roast__content h3 { font-size: 1.2rem; color: var(--pine); margin: 1rem 0 0.5rem; }
.roast__content h4 { font-size: 1.1rem; color: var(--subtle); margin: 0.75rem 0 0.5rem; }
.roast__content li { margin-left: 1.5rem; margin-bottom: 0.5rem; list-style: disc; }
.roast__actions { margin-top: 1.5rem; padding-top: 1rem; border-top: 2px solid var(--overlay); }
.roast__button--primary { padding: 0.75rem 1.5rem; background: var(--pine); color: var(--base); border: none; border-radius: 8px; font-weight: 600; cursor: pointer; }
.error { background: #fce8ec; border: 2px solid var(--love); border-radius: 8px; padding: 1.25rem; margin: 2rem 0; }
.error__title { color: var(--love); font-weight: 700; margin-bottom: 0.5rem; }
.error__message { color: #8b3d4d; }
.error__retry { padding: 0.5rem 1rem; background: var(--love); color: var(--base); border: none; border-radius: 4px; cursor: pointer; }
"#;

fn shell(_options: LeptosOptions) -> impl IntoView {
    use leptos::prelude::*;
    use leptos_meta::*;

    let css = r#"
        :root {
            --base: #faf4ed;
            --surface: #fffaf3;
            --overlay: #f2e9e1;
            --muted: #9893a5;
            --subtle: #797593;
            --text: #575279;
            --love: #b4637a;
            --gold: #ea9d34;
            --pine: #286983;
            --foam: #56949f;
        }
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: 'Inter', -apple-system, sans-serif;
            background: var(--base);
            color: var(--text);
            min-height: 100vh;
        }
        .container { max-width: 800px; margin: 0 auto; padding: 1.5rem; }
        .hero { text-align: center; padding: 3rem 0 2rem; }
        .hero__title { font-size: clamp(2rem, 5vw, 3rem); color: var(--love); font-weight: 800; margin-bottom: 0.75rem; }
        .hero__subtitle { color: var(--subtle); font-size: 1.1rem; max-width: 500px; margin: 0 auto; }
        .url-form { display: flex; flex-direction: column; gap: 1rem; margin: 2rem 0; }
        @media (min-width: 640px) { .url-form { flex-direction: row; } }
        .url-form__input {
            flex: 1; padding: 1rem 1.25rem; border: 2px solid var(--overlay);
            border-radius: 8px; background: var(--surface); color: var(--text); font-size: 1rem;
        }
        .url-form__input:focus { outline: none; border-color: var(--pine); }
        .url-form__input::placeholder { color: var(--muted); }
        .url-form__button {
            padding: 1rem 2rem; background: var(--love); color: var(--base);
            border: none; border-radius: 8px; font-size: 1rem; font-weight: 600; cursor: pointer;
        }
        .url-form__button:hover { opacity: 0.9; }
        .url-form__button:disabled { background: var(--muted); cursor: not-allowed; }
        .loading { display: flex; flex-direction: column; align-items: center; padding: 3rem; }
        .loading__spinner {
            width: 50px; height: 50px; border: 4px solid var(--overlay);
            border-top-color: var(--gold); border-radius: 50%; animation: spin 1s linear infinite;
        }
        @keyframes spin { to { transform: rotate(360deg); } }
        .loading__text { margin-top: 1rem; color: var(--subtle); font-style: italic; }
        .roast {
            background: var(--surface); border: 2px solid var(--overlay);
            border-radius: 12px; padding: 1.5rem; margin: 2rem 0;
        }
        .roast__title { color: var(--love); font-size: 1.4rem; margin-bottom: 1rem; padding-bottom: 0.75rem; border-bottom: 2px solid var(--overlay); }
        .roast__content { line-height: 1.8; font-size: 1.05rem; }
        .roast__content p { margin-bottom: 1rem; }
        .roast__content strong { font-weight: 700; color: var(--love); }
        .roast__content em { font-style: italic; }
        .roast__content h3 { font-size: 1.2rem; color: var(--pine); margin: 1rem 0 0.5rem; }
        .roast__content h4 { font-size: 1.1rem; color: var(--subtle); margin: 0.75rem 0 0.5rem; }
        .roast__content li { margin-left: 1.5rem; margin-bottom: 0.5rem; list-style: disc; }
        .roast__actions { margin-top: 1.5rem; padding-top: 1rem; border-top: 2px solid var(--overlay); }
        .roast__button--primary { padding: 0.75rem 1.5rem; background: var(--pine); color: var(--base); border: none; border-radius: 8px; font-weight: 600; cursor: pointer; }
        .error { background: #fce8ec; border: 2px solid var(--love); border-radius: 8px; padding: 1.25rem; margin: 2rem 0; }
        .error__title { color: var(--love); font-weight: 700; margin-bottom: 0.5rem; }
        .error__message { color: #8b3d4d; }
        .error__retry { margin-top: 1rem; padding: 0.5rem 1rem; background: var(--love); color: var(--base); border: none; border-radius: 4px; cursor: pointer; }
        .footer { text-align: center; padding: 2rem 0; color: var(--muted); font-size: 0.9rem; border-top: 1px solid var(--overlay); margin-top: 3rem; }
    "#;

    let validation_script = r#"
        document.addEventListener('DOMContentLoaded', function() {
            const form = document.querySelector('.url-form');
            const input = document.querySelector('.url-form__input');
            const button = document.querySelector('.url-form__button');
            const originalText = button.textContent;

            function validateUrl(str) {
                try {
                    const url = new URL(str);
                    return url.protocol === 'http:' || url.protocol === 'https:';
                } catch {
                    return false;
                }
            }

            function updateButton() {
                const isValid = validateUrl(input.value.trim());
                button.disabled = !isValid;
            }

            form.addEventListener('submit', function() {
                button.disabled = true;
                button.textContent = 'Memproses...';
                button.style.cursor = 'wait';
            });

            input.addEventListener('input', updateButton);
            input.addEventListener('change', updateButton);
            updateButton();
        });
    "#;

    view! {
        <!DOCTYPE html>
        <html lang="id">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <title>"Roasting Startup Indonesia"</title>
                <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🔥</text></svg>"/>
                <style>{css}</style>
                <MetaTags/>
            </head>
            <body>
                <App/>
                <script>{validation_script}</script>
            </body>
        </html>
    }
}
