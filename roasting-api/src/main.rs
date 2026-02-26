use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Json, Router,
};
use leptos::prelude::*;
use leptos_axum::{generate_route_list, handle_server_fns_with_context, LeptosRoutes};
use roasting_app::domain::{PersistedRoast, RoastWithDetails, User};
use roasting_app::AppContext;
use roasting_ui::pages::{GenerateRoastFn, GetCurrentUserFn};
use roasting_ui::App;
use serde::Deserialize;
use tower_http::compression::CompressionLayer;
use tower_sessions::{Expiry, MemoryStore, Session, SessionManagerLayer};
use uuid::Uuid;

#[derive(Deserialize)]
struct RoastForm {
    url: String,
}

#[derive(Deserialize)]
struct AuthCallbackQuery {
    code: String,
    state: String,
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

    // Initialize app context (database, OAuth, etc.)
    let app_context = AppContext::from_env().await;

    // Set up session store
    // Use MemoryStore for sessions (sessions lost on restart - consider PostgresStore in production)
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(tower_sessions::cookie::time::Duration::days(7)))
        .with_secure(false) // Set to true in production with HTTPS
        .with_same_site(tower_sessions::cookie::SameSite::Lax); // Allow cookies on OAuth redirects

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
    server_fn::axum::register_explicit::<GetCurrentUserFn>();
    tracing::info!("Registered server functions: GenerateRoastFn, GetCurrentUserFn");

    let app = Router::new()
        // Auth routes
        .route("/auth/login", get({
            let ctx = app_context.clone();
            move |session: Session| {
                let ctx = ctx.clone();
                async move { handle_auth_login(ctx, session).await }
            }
        }))
        .route("/auth/callback", get({
            let ctx = app_context.clone();
            move |session: Session, query: Query<AuthCallbackQuery>| {
                let ctx = ctx.clone();
                async move { handle_auth_callback(ctx, session, query.0).await }
            }
        }))
        .route("/auth/logout", post({
            move |session: Session| async move { handle_auth_logout(session).await }
        }))
        .route("/auth/me", get({
            let ctx = app_context.clone();
            move |session: Session| {
                let ctx = ctx.clone();
                async move { handle_auth_me(ctx, session).await }
            }
        }))
        // API routes
        .route("/api/roast/{id}/vote", post({
            let ctx = app_context.clone();
            move |session: Session, path: Path<Uuid>| {
                let ctx = ctx.clone();
                async move { handle_vote(ctx, session, path.0).await }
            }
        }))
        .route("/api/leaderboard", get({
            let ctx = app_context.clone();
            move |session: Session| {
                let ctx = ctx.clone();
                async move { handle_leaderboard(ctx, session).await }
            }
        }))
        .route("/api/roast/{id}", get({
            let ctx = app_context.clone();
            move |session: Session, path: Path<Uuid>| {
                let ctx = ctx.clone();
                async move { handle_get_roast(ctx, session, path.0).await }
            }
        }))
        // View roast page
        .route("/r/{id}", get({
            let ctx = app_context.clone();
            move |session: Session, path: Path<Uuid>| {
                let ctx = ctx.clone();
                async move { handle_view_roast_page(ctx, session, path.0).await }
            }
        }))
        // Leaderboard page
        .route("/leaderboard", get({
            let ctx = app_context.clone();
            move |session: Session| {
                let ctx = ctx.clone();
                async move { handle_leaderboard_page(ctx, session).await }
            }
        }))
        // Roast form route
        .route("/roast", get({
            let ctx = app_context.clone();
            move |session: Session, query: Query<RoastForm>| {
                let ctx = ctx.clone();
                async move {
                    handle_roast_form(ctx, session, query.0).await
                }
            }
        }).post({
            let ctx = app_context.clone();
            move |session: Session, form: Form<RoastForm>| {
                let ctx = ctx.clone();
                async move {
                    handle_roast_form(ctx, session, form.0).await
                }
            }
        }))
        .route("/api/{*fn_name}", post({
            let ctx = app_context.clone();
            move |session: Session, req: axum::http::Request<axum::body::Body>| {
                let ctx = ctx.clone();
                let session = session.clone();
                tracing::info!("Server function called, session available: true");
                async move {
                    handle_server_fns_with_context(
                        {
                            let ctx = ctx.clone();
                            let session = session.clone();
                            move || {
                                tracing::info!("Providing context with session");
                                provide_context(ctx.clone());
                                provide_context(session.clone());
                            }
                        },
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
        .layer(session_layer)
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

async fn handle_roast_form(ctx: AppContext, session: Session, form: RoastForm) -> impl IntoResponse {
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
        Ok(roast) => {
            // Get current user if logged in
            let user_id: Option<Uuid> = session.get("user_id").await.ok().flatten();

            // Create PersistedRoast and save to database
            let persisted = PersistedRoast::new(
                roast.startup_name.clone(),
                form.url.clone(),
                roast.roast_text.clone(),
                user_id,
            );

            // Persist the roast to database
            match ctx.roast_repo.create(&persisted).await {
                Ok(saved_roast) => {
                    Html(render_result_page_with_id(
                        &roast.startup_name,
                        &roast.roast_text,
                        &form.url,
                        saved_roast.id,
                    ))
                }
                Err(e) => {
                    tracing::error!("Failed to persist roast: {}", e);
                    // Still show the roast even if persistence fails
                    Html(render_result_page(&roast.startup_name, &roast.roast_text, &form.url))
                }
            }
        }
        Err(e) => Html(render_error_page(&e.user_message())),
    }
}

// Session keys
const SESSION_USER_ID: &str = "user_id";
const SESSION_CSRF_TOKEN: &str = "csrf_token";
const SESSION_PKCE_VERIFIER: &str = "pkce_verifier";

async fn handle_auth_login(ctx: AppContext, session: Session) -> impl IntoResponse {
    let (auth_url, csrf_token, pkce_verifier) = ctx.google_oauth.get_auth_url();

    // Store CSRF token and PKCE verifier in session
    if let Err(e) = session.insert(SESSION_CSRF_TOKEN, csrf_token.secret().clone()).await {
        tracing::error!("Failed to store CSRF token: {}", e);
        return Redirect::to("/?error=session_error");
    }
    if let Err(e) = session.insert(SESSION_PKCE_VERIFIER, pkce_verifier.secret().clone()).await {
        tracing::error!("Failed to store PKCE verifier: {}", e);
        return Redirect::to("/?error=session_error");
    }

    Redirect::to(&auth_url)
}

async fn handle_auth_callback(
    ctx: AppContext,
    session: Session,
    query: AuthCallbackQuery,
) -> impl IntoResponse {
    // Verify CSRF token
    let stored_csrf: Option<String> = session.get(SESSION_CSRF_TOKEN).await.ok().flatten();
    if stored_csrf.is_none() {
        tracing::warn!("CSRF token not found in session - session may have expired or server restarted");
        // Redirect to login again instead of showing error
        return Redirect::to("/auth/login");
    }
    if stored_csrf.as_ref() != Some(&query.state) {
        tracing::warn!("CSRF token mismatch: stored={:?}, received={}", stored_csrf, &query.state);
        return Redirect::to("/auth/login");
    }

    // Get PKCE verifier
    let pkce_secret: Option<String> = session.get(SESSION_PKCE_VERIFIER).await.ok().flatten();
    let pkce_verifier = match pkce_secret {
        Some(secret) => oauth2::PkceCodeVerifier::new(secret),
        None => {
            tracing::warn!("PKCE verifier not found in session");
            return Redirect::to("/?error=session_error");
        }
    };

    // Exchange code for user info
    let user_info = match ctx.google_oauth.exchange_code(&query.code, pkce_verifier).await {
        Ok(info) => info,
        Err(e) => {
            tracing::error!("OAuth exchange failed: {}", e);
            return Redirect::to("/?error=oauth_failed");
        }
    };

    // Create User object
    let new_user = User {
        id: Uuid::new_v4(),
        google_id: user_info.sub.clone(),
        email: user_info.email.clone(),
        name: user_info.name.clone(),
        avatar_url: user_info.picture.clone(),
        created_at: None,
        updated_at: None,
    };

    // Upsert user in database
    let user = match ctx.user_repo.upsert(&new_user).await {
        Ok(user) => user,
        Err(e) => {
            tracing::error!("Failed to upsert user: {}", e);
            return Redirect::to("/?error=db_error");
        }
    };

    // Store user ID in session
    if let Err(e) = session.insert(SESSION_USER_ID, user.id).await {
        tracing::error!("Failed to store user ID in session: {}", e);
        return Redirect::to("/?error=session_error");
    }

    // Clean up OAuth state from session
    let _ = session.remove::<String>(SESSION_CSRF_TOKEN).await;
    let _ = session.remove::<String>(SESSION_PKCE_VERIFIER).await;

    tracing::info!("User logged in: {} ({})", user.name, user.email);
    Redirect::to("/")
}

async fn handle_auth_logout(session: Session) -> impl IntoResponse {
    session.flush().await.ok();
    Redirect::to("/")
}

async fn handle_auth_me(ctx: AppContext, session: Session) -> impl IntoResponse {
    let user_id: Option<Uuid> = session.get(SESSION_USER_ID).await.ok().flatten();

    match user_id {
        Some(id) => match ctx.user_repo.find_by_id(id).await {
            Ok(Some(user)) => Json(serde_json::json!({
                "authenticated": true,
                "user": {
                    "id": user.id,
                    "name": user.name,
                    "email": user.email,
                    "avatar_url": user.avatar_url,
                }
            })).into_response(),
            _ => Json(serde_json::json!({ "authenticated": false })).into_response(),
        },
        None => Json(serde_json::json!({ "authenticated": false })).into_response(),
    }
}

async fn handle_vote(ctx: AppContext, session: Session, roast_id: Uuid) -> impl IntoResponse {
    let user_id: Option<Uuid> = session.get(SESSION_USER_ID).await.ok().flatten();

    match user_id {
        Some(user_id) => {
            // toggle() already handles incrementing/decrementing fire count
            match ctx.vote_repo.toggle(user_id, roast_id, &ctx.roast_repo).await {
                Ok(result) => {
                    Json(serde_json::json!({
                        "success": true,
                        "voted": result.voted,
                        "fire_count": result.new_fire_count,
                    })).into_response()
                }
                Err(e) => {
                    tracing::error!("Vote failed: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                        "success": false,
                        "error": "Failed to toggle vote"
                    }))).into_response()
                }
            }
        }
        None => {
            (StatusCode::UNAUTHORIZED, Json(serde_json::json!({
                "success": false,
                "error": "Must be logged in to vote"
            }))).into_response()
        }
    }
}

async fn handle_leaderboard(ctx: AppContext, session: Session) -> impl IntoResponse {
    let user_id: Option<Uuid> = session.get(SESSION_USER_ID).await.ok().flatten();

    match ctx.roast_repo.get_leaderboard(50, user_id).await {
        Ok(roasts) => Json(serde_json::json!({
            "success": true,
            "roasts": roasts.into_iter().map(|r| serde_json::json!({
                "id": r.id,
                "startup_name": r.startup_name,
                "startup_url": r.startup_url,
                "roast_text": r.roast_text,
                "fire_count": r.fire_count,
                "created_at": r.created_at,
                "author_name": r.author_name,
                "author_avatar": r.author_avatar,
                "user_has_voted": r.user_has_voted,
            })).collect::<Vec<_>>(),
        })).into_response(),
        Err(e) => {
            tracing::error!("Failed to get leaderboard: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "success": false,
                "error": "Failed to fetch leaderboard"
            }))).into_response()
        }
    }
}

async fn handle_leaderboard_page(ctx: AppContext, session: Session) -> impl IntoResponse {
    let user_id: Option<Uuid> = session.get(SESSION_USER_ID).await.ok().flatten();

    match ctx.roast_repo.get_leaderboard(50, user_id).await {
        Ok(roasts) => Html(render_leaderboard_page(&roasts)),
        Err(e) => {
            tracing::error!("Failed to get leaderboard: {}", e);
            Html(render_error_page("Gagal memuat leaderboard"))
        }
    }
}

async fn handle_view_roast_page(ctx: AppContext, session: Session, roast_id: Uuid) -> impl IntoResponse {
    let user_id: Option<Uuid> = session.get(SESSION_USER_ID).await.ok().flatten();

    match ctx.roast_repo.find_by_id_with_details(roast_id, user_id).await {
        Ok(Some(roast)) => {
            Html(render_result_page_with_id(
                &roast.startup_name,
                &roast.roast_text,
                &roast.startup_url,
                roast_id,
            ))
        }
        Ok(None) => Html(render_error_page("Roast tidak ditemukan")),
        Err(e) => {
            tracing::error!("Failed to get roast: {}", e);
            Html(render_error_page("Gagal memuat roast"))
        }
    }
}

async fn handle_get_roast(ctx: AppContext, session: Session, roast_id: Uuid) -> impl IntoResponse {
    let user_id: Option<Uuid> = session.get(SESSION_USER_ID).await.ok().flatten();

    match ctx.roast_repo.find_by_id_with_details(roast_id, user_id).await {
        Ok(Some(roast)) => {
            Json(serde_json::json!({
                "success": true,
                "roast": {
                    "id": roast.id,
                    "startup_name": roast.startup_name,
                    "startup_url": roast.startup_url,
                    "roast_text": roast.roast_text,
                    "fire_count": roast.fire_count,
                    "created_at": roast.created_at,
                    "author_name": roast.author_name,
                    "author_avatar": roast.author_avatar,
                },
                "has_voted": roast.user_has_voted,
            })).into_response()
        }
        Ok(None) => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "success": false,
                "error": "Roast not found"
            }))).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to get roast: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "success": false,
                "error": "Failed to fetch roast"
            }))).into_response()
        }
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

fn render_result_page_with_id(startup_name: &str, roast_text: &str, url: &str, roast_id: Uuid) -> String {
    let html_content = simple_markdown_to_html(roast_text);
    format!(r#"<!DOCTYPE html>
<html lang="id">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Roasting: {startup_name}</title>
    <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🔥</text></svg>">
    <style>{CSS}</style>
    <script>history.replaceState(null, '', '/r/{roast_id}');</script>
</head>
<body>
    <main class="container">
        <div class="roast">
            <h2 class="roast__title">Roasting: {startup_name}</h2>
            <div class="roast__content">{html_content}</div>
            <div class="roast__actions">
                <button id="vote-btn" class="roast__vote-btn" onclick="toggleVote()">
                    <span class="fire-emoji">🔥</span>
                    <span id="fire-count">0</span>
                </button>
                <a href="/" class="roast__button--primary" style="text-decoration:none;display:inline-block;">Roast Lagi!</a>
                <a href="/leaderboard" class="roast__button--secondary" style="text-decoration:none;display:inline-block;margin-left:0.5rem;">Leaderboard</a>
            </div>
        </div>
    </main>
    <script>
        const roastId = '{roast_id}';
        let hasVoted = false;

        // Load initial vote state
        fetch('/api/roast/' + roastId)
            .then(r => r.json())
            .then(data => {{
                if (data.success) {{
                    document.getElementById('fire-count').textContent = data.roast.fire_count;
                    hasVoted = data.has_voted;
                    updateVoteButton();
                }}
            }});

        function updateVoteButton() {{
            const btn = document.getElementById('vote-btn');
            if (hasVoted) {{
                btn.classList.add('voted');
            }} else {{
                btn.classList.remove('voted');
            }}
        }}

        function toggleVote() {{
            fetch('/api/roast/' + roastId + '/vote', {{ method: 'POST' }})
                .then(r => r.json())
                .then(data => {{
                    if (data.success) {{
                        hasVoted = data.voted;
                        document.getElementById('fire-count').textContent = data.fire_count;
                        updateVoteButton();
                    }} else if (data.error === 'Must be logged in to vote') {{
                        if (confirm('Kamu harus login untuk vote. Login dengan Google?')) {{
                            window.location.href = '/auth/login';
                        }}
                    }}
                }});
        }}
    </script>
</body>
</html>"#, startup_name = startup_name, html_content = html_content, CSS = CSS, roast_id = roast_id)
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

fn render_leaderboard_page(roasts: &[RoastWithDetails]) -> String {
    let mut cards = String::new();
    for (i, roast) in roasts.iter().enumerate() {
        let rank = i + 1;
        let preview: String = roast.roast_text.chars().take(80).collect();
        let user_display = roast.author_name.as_deref().unwrap_or("Anonim");
        let rank_class = match rank {
            1 => "lb-card__rank--gold",
            2 => "lb-card__rank--silver",
            3 => "lb-card__rank--bronze",
            _ => "",
        };
        cards.push_str(&format!(
            r#"<a href="/r/{id}" class="lb-card">
                <div class="lb-card__rank {rank_class}">{rank}</div>
                <div class="lb-card__content">
                    <div class="lb-card__startup">{startup_name}</div>
                    <div class="lb-card__preview">{preview}...</div>
                    <div class="lb-card__meta">
                        <span class="lb-card__fire">🔥 {fire_count}</span>
                        <span class="lb-card__user">oleh {user_display}</span>
                    </div>
                </div>
            </a>"#,
            id = roast.id,
            rank = rank,
            rank_class = rank_class,
            startup_name = roast.startup_name,
            preview = preview,
            fire_count = roast.fire_count,
            user_display = user_display,
        ));
    }

    format!(r#"<!DOCTYPE html>
<html lang="id">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Leaderboard - Roasting Startup</title>
    <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🔥</text></svg>">
    <style>{CSS}
    .lb-page {{ padding: 1rem 0; }}
    .lb-title {{
        color: var(--love);
        font-size: 1.5rem;
        font-weight: 800;
        text-align: center;
        margin-bottom: 1.5rem;
    }}
    @media (min-width: 640px) {{ .lb-title {{ font-size: 2rem; margin-bottom: 2rem; }} }}
    .lb-list {{
        display: flex;
        flex-direction: column;
        gap: 0.75rem;
    }}
    @media (min-width: 640px) {{ .lb-list {{ gap: 1rem; }} }}
    .lb-card {{
        display: flex;
        align-items: flex-start;
        gap: 0.75rem;
        padding: 1rem;
        background: var(--surface);
        border: 2px solid var(--overlay);
        border-radius: 12px;
        text-decoration: none;
        color: inherit;
        transition: all 0.2s ease;
    }}
    @media (min-width: 640px) {{ .lb-card {{ padding: 1.25rem; gap: 1rem; }} }}
    .lb-card:hover {{
        border-color: var(--pine);
        transform: translateY(-2px);
        box-shadow: 0 4px 12px rgba(87, 82, 121, 0.1);
    }}
    .lb-card__rank {{
        flex-shrink: 0;
        width: 32px;
        height: 32px;
        display: flex;
        align-items: center;
        justify-content: center;
        background: var(--overlay);
        color: var(--text);
        font-weight: 700;
        font-size: 0.9rem;
        border-radius: 50%;
    }}
    @media (min-width: 640px) {{ .lb-card__rank {{ width: 40px; height: 40px; font-size: 1rem; }} }}
    .lb-card__rank--gold {{ background: var(--gold); color: #fff; }}
    .lb-card__rank--silver {{ background: #a0a0a0; color: #fff; }}
    .lb-card__rank--bronze {{ background: #cd7f32; color: #fff; }}
    .lb-card__content {{
        flex: 1;
        min-width: 0;
        display: flex;
        flex-direction: column;
        gap: 0.35rem;
    }}
    .lb-card__startup {{
        font-weight: 600;
        font-size: 0.95rem;
        color: var(--pine);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }}
    @media (min-width: 640px) {{ .lb-card__startup {{ font-size: 1.05rem; }} }}
    .lb-card__preview {{
        font-size: 0.85rem;
        color: var(--subtle);
        line-height: 1.4;
        display: -webkit-box;
        -webkit-line-clamp: 2;
        -webkit-box-orient: vertical;
        overflow: hidden;
    }}
    @media (min-width: 640px) {{ .lb-card__preview {{ font-size: 0.9rem; }} }}
    .lb-card__meta {{
        display: flex;
        align-items: center;
        gap: 1rem;
        margin-top: 0.25rem;
    }}
    .lb-card__fire {{
        font-weight: 600;
        font-size: 0.9rem;
        color: var(--gold);
    }}
    .lb-card__user {{
        font-size: 0.8rem;
        color: var(--muted);
    }}
    .lb-actions {{
        text-align: center;
        margin-top: 1.5rem;
        padding-top: 1.5rem;
        border-top: 1px solid var(--overlay);
    }}
    @media (min-width: 640px) {{ .lb-actions {{ margin-top: 2rem; }} }}
    .lb-empty {{
        text-align: center;
        padding: 3rem 1rem;
        color: var(--muted);
        font-style: italic;
    }}
    </style>
</head>
<body>
    <main class="container">
        <div class="lb-page">
            <h1 class="lb-title">🔥 Leaderboard Roasting 🔥</h1>
            <div class="lb-list">
                {cards}
            </div>
            <div class="lb-actions">
                <a href="/" class="roast__button--primary">Roast Startup Lain!</a>
            </div>
        </div>
    </main>
</body>
</html>"#, CSS = CSS, cards = cards)
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
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    background: var(--base);
    color: var(--text);
    min-height: 100vh;
    line-height: 1.6;
}
.container { max-width: 700px; margin: 0 auto; padding: 1rem; }
@media (min-width: 640px) { .container { padding: 2rem; } }
.roast {
    background: var(--surface);
    border: 2px solid var(--overlay);
    border-radius: 16px;
    padding: 1.25rem;
    margin: 1rem 0;
    box-shadow: 0 4px 12px rgba(87, 82, 121, 0.08);
}
@media (min-width: 640px) { .roast { padding: 2rem; margin: 2rem 0; } }
.roast__title {
    color: var(--love);
    font-size: 1.25rem;
    font-weight: 700;
    margin-bottom: 1rem;
    padding-bottom: 0.75rem;
    border-bottom: 2px solid var(--overlay);
}
@media (min-width: 640px) { .roast__title { font-size: 1.5rem; } }
.roast__content {
    color: var(--text);
    line-height: 1.9;
    font-size: 1rem;
}
@media (min-width: 640px) { .roast__content { font-size: 1.1rem; } }
.roast__content p { margin-bottom: 1rem; }
.roast__content p:last-child { margin-bottom: 0; }
.roast__content strong { font-weight: 700; color: var(--pine); }
.roast__content em { font-style: italic; color: var(--subtle); }
.roast__content h3 { font-size: 1.15rem; color: var(--pine); margin: 1.25rem 0 0.5rem; font-weight: 600; }
.roast__content h4 { font-size: 1.05rem; color: var(--subtle); margin: 1rem 0 0.5rem; font-weight: 600; }
.roast__content li { margin-left: 1.5rem; margin-bottom: 0.5rem; list-style: disc; }
.roast__actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    margin-top: 1.5rem;
    padding-top: 1.25rem;
    border-top: 2px solid var(--overlay);
}
.roast__button--primary {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.75rem 1.5rem;
    background: var(--love);
    color: #fff;
    border: none;
    border-radius: 9999px;
    font-size: 0.95rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s ease;
    text-decoration: none;
}
.roast__button--primary:hover { background: #a3566a; transform: translateY(-1px); }
.roast__button--secondary {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.75rem 1.5rem;
    background: var(--overlay);
    color: var(--text);
    border: none;
    border-radius: 9999px;
    font-size: 0.95rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s ease;
    text-decoration: none;
}
.roast__button--secondary:hover { background: #e5dcd4; }
.roast__vote-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1.25rem;
    background: var(--surface);
    border: 2px solid var(--overlay);
    border-radius: 9999px;
    font-size: 1rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s ease;
    color: var(--text);
}
.roast__vote-btn:hover { border-color: var(--gold); background: #fff8ed; }
.roast__vote-btn.voted { background: #fff8ed; border-color: var(--gold); color: var(--gold); }
.roast__vote-btn .fire-emoji { font-size: 1.2rem; }
.error {
    background: #fef2f4;
    border: 2px solid var(--love);
    border-radius: 12px;
    padding: 1.25rem;
    margin: 2rem 0;
}
.error__title { color: var(--love); font-weight: 700; margin-bottom: 0.5rem; font-size: 1.1rem; }
.error__message { color: #8b3d4d; line-height: 1.6; }
.error__retry {
    display: inline-block;
    margin-top: 1rem;
    padding: 0.6rem 1.25rem;
    background: var(--love);
    color: #fff;
    border: none;
    border-radius: 9999px;
    font-weight: 600;
    cursor: pointer;
    text-decoration: none;
    transition: all 0.2s ease;
}
.error__retry:hover { background: #a3566a; }
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
