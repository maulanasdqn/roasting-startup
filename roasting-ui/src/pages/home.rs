use leptos::prelude::*;
use roasting_app::domain::Roast;
use server_fn::ServerFnError;

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

#[component]
pub fn HomePage() -> impl IntoView {
    view! {
        <div class="hero">
            <h1 class="hero__title">"Hancurkan Startup-mu"</h1>
            <p class="hero__subtitle">
                "Masukkan URL startup dan AI akan memberikan roasting brutal dalam bahasa Indonesia"
            </p>
        </div>

        <form action="/roast" method="post" class="url-form">
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
    }
}
