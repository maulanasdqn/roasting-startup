use leptos::prelude::*;

#[component]
pub fn LoadingSpinner() -> impl IntoView {
    view! {
        <div class="loading">
            <div class="loading__spinner"></div>
            <p class="loading__text">"Sedang membakar startup-mu..."</p>
        </div>
    }
}
