use leptos::prelude::*;
use roasting_app::domain::{Roast, RoastWithDetails, User};
use server_fn::ServerFnError;

#[server(GetCurrentUserFn, "/api", endpoint = "current_user")]
pub async fn get_current_user() -> Result<Option<User>, ServerFnError> {
    use roasting_app::AppContext;
    use tower_sessions::Session;

    // Use use_context instead of expect_context to handle SSR gracefully
    let session = match use_context::<Session>() {
        Some(s) => {
            tracing::info!("get_current_user: Session context found");
            s
        }
        None => {
            tracing::info!("get_current_user: No session context");
            return Ok(None);
        }
    };

    let ctx = match use_context::<AppContext>() {
        Some(c) => c,
        None => {
            tracing::info!("get_current_user: No AppContext");
            return Ok(None);
        }
    };

    let user_id: Option<uuid::Uuid> = session.get("user_id").await.ok().flatten();
    tracing::info!("get_current_user: user_id from session = {:?}", user_id);

    match user_id {
        Some(id) => {
            let model = ctx
                .user_repo
                .find_by_id(id)
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;

            tracing::info!("get_current_user: found user = {:?}", model.is_some());

            Ok(model.map(|m| User {
                id: m.id,
                google_id: m.google_id,
                email: m.email,
                name: m.name,
                avatar_url: m.avatar_url,
                created_at: m.created_at,
                updated_at: m.updated_at,
            }))
        }
        None => Ok(None),
    }
}

#[server(GenerateRoastFn, "/api", endpoint = "generate_roast")]
pub async fn generate_roast(url: String) -> Result<Roast, ServerFnError> {
    use roasting_app::infrastructure::security::InputSanitizer;
    use roasting_app::AppContext;
    use std::net::{IpAddr, Ipv4Addr};

    let ctx = expect_context::<AppContext>();

    let client_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    if let Err(e) = ctx.rate_limiter.check_rate_limit(client_ip) {
        return Err(ServerFnError::new(e.message_id()));
    }

    if let Err(e) = ctx.cost_tracker.check_and_increment() {
        return Err(ServerFnError::new(e.message_id()));
    }

    let validated_url = InputSanitizer::validate_url(&url)
        .map_err(|e| ServerFnError::new(e.user_message()))?;

    ctx.generate_roast
        .execute(validated_url)
        .await
        .map_err(|e| ServerFnError::new(e.user_message()))
}

#[server(GetLeaderboardFn, "/api", endpoint = "home_leaderboard")]
pub async fn get_leaderboard() -> Result<Vec<RoastWithDetails>, ServerFnError> {
    use roasting_app::AppContext;

    let ctx = expect_context::<AppContext>();

    ctx.roast_repo
        .get_leaderboard(10, None)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[component]
pub fn HomePage() -> impl IntoView {
    let leaderboard = Resource::new(|| (), |_| get_leaderboard());

    view! {
        <div class="hero">
            <h1 class="hero__title">"Hancurkan Startup-mu"</h1>
            <p class="hero__subtitle">
                "Masukkan URL startup dan AI akan memberikan roasting brutal dalam bahasa Indonesia"
            </p>
        </div>

        <div class="home-layout">
            // Left side: Input form + Google login
            <div class="home-layout__left">
                <AuthSection/>

                <form action="/roast" method="post" class="url-form url-form--vertical">
                    <input
                        type="url"
                        name="url"
                        class="url-form__input"
                        placeholder="Masukkan URL startup... (contoh: https://perfect10.id)"
                        required
                    />
                    <button
                        type="submit"
                        class="url-form__button"
                    >
                        "Roast Sekarang!"
                    </button>
                </form>
            </div>

            // Right side: Leaderboard
            <div class="home-layout__right">
                <div class="leaderboard">
                    <h2 class="leaderboard__title">"Leaderboard"</h2>
                    <Suspense fallback=move || view! { <p class="leaderboard__loading">"Memuat..."</p> }>
                        {move || {
                            leaderboard.get().map(|result| {
                                match result {
                                    Ok(roasts) => {
                                        if roasts.is_empty() {
                                            view! {
                                                <p class="leaderboard__empty">"Belum ada roast. Jadilah yang pertama!"</p>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <ul class="leaderboard__list">
                                                    {roasts.into_iter().enumerate().map(|(i, roast)| {
                                                        view! {
                                                            <li class="leaderboard__item">
                                                                <span class="leaderboard__rank">{i + 1}</span>
                                                                <div class="leaderboard__info">
                                                                    <a href={format!("/r/{}", roast.id)} class="leaderboard__name">
                                                                        {roast.startup_name}
                                                                    </a>
                                                                    <span class="leaderboard__author">
                                                                        {roast.author_name.unwrap_or_else(|| "Anonim".to_string())}
                                                                    </span>
                                                                </div>
                                                                <span class="leaderboard__fire">{roast.fire_count} " 🔥"</span>
                                                            </li>
                                                        }
                                                    }).collect::<Vec<_>>()}
                                                </ul>
                                            }.into_any()
                                        }
                                    }
                                    Err(_) => view! {
                                        <p class="leaderboard__error">"Gagal memuat leaderboard"</p>
                                    }.into_any()
                                }
                            })
                        }}
                    </Suspense>
                </div>
            </div>
        </div>
    }
}

/// Auth section component - uses JS to check auth after page load
#[component]
fn AuthSection() -> impl IntoView {
    view! {
        <div class="auth-section" id="auth-section">
            // Default: show login button, JS will replace if logged in
            <a href="/auth/login" class="google-login-btn" id="login-btn">
                <svg class="google-login-btn__icon" viewBox="0 0 24 24" width="20" height="20">
                    <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"/>
                    <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
                    <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"/>
                    <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"/>
                </svg>
                "Login dengan Google"
            </a>
            <p class="auth-section__hint" id="login-hint">"Login untuk menyimpan dan vote roast"</p>
            // Hidden user info section - shown by JS if logged in
            <div id="user-section" style="display:none;">
                <div class="user-info">
                    <img id="user-avatar" src="" alt="Avatar" class="user-info__avatar"/>
                    <div class="user-info__details">
                        <span class="user-info__name" id="user-name"></span>
                        <span class="user-info__email" id="user-email"></span>
                    </div>
                </div>
                <form action="/auth/logout" method="post" class="logout-form">
                    <button type="submit" class="logout-btn">"Logout"</button>
                </form>
            </div>
        </div>
        <script>
            r#"
            (function() {
                fetch('/auth/me', { credentials: 'include' })
                    .then(r => r.json())
                    .then(data => {
                        if (data.authenticated && data.user) {
                            document.getElementById('login-btn').style.display = 'none';
                            document.getElementById('login-hint').style.display = 'none';
                            document.getElementById('user-section').style.display = 'flex';
                            document.getElementById('user-name').textContent = data.user.name;
                            document.getElementById('user-email').textContent = data.user.email;
                            var avatarEl = document.getElementById('user-avatar');
                            if (data.user.avatar_url && data.user.avatar_url.length > 0) {
                                avatarEl.src = data.user.avatar_url;
                                avatarEl.onerror = function() { this.style.display = 'none'; };
                            } else {
                                avatarEl.style.display = 'none';
                            }
                            document.getElementById('auth-section').classList.add('auth-section--logged-in');
                        }
                    })
                    .catch(err => console.error('Auth check failed:', err));
            })();
            "#
        </script>
    }
}
