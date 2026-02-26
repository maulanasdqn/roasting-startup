use leptos::prelude::*;

#[component]
pub fn ErrorDisplay(
    #[prop(into)] message: String,
    #[prop(optional)] on_retry: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <div class="error">
            <p class="error__title">"Waduh, ada masalah!"</p>
            <p class="error__message">{message}</p>
            {move || on_retry.map(|retry| view! {
                <button
                    class="error__retry"
                    on:click=move |_| retry.run(())
                >
                    "Coba Lagi"
                </button>
            })}
        </div>
    }
}
