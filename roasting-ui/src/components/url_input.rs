use leptos::prelude::*;

#[component]
pub fn UrlInput(
    value: RwSignal<String>,
    #[prop(into)] on_submit: Callback<String>,
    #[prop(into)] is_loading: Signal<bool>,
) -> impl IntoView {
    let on_form_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let url = value.get();
        if !url.is_empty() {
            on_submit.run(url);
        }
    };

    view! {
        <form class="url-form" on:submit=on_form_submit>
            <input
                type="url"
                class="url-form__input"
                placeholder="Masukkan URL startup... (contoh: https://tokopedia.com)"
                prop:value=move || value.get()
                on:input=move |ev| value.set(event_target_value(&ev))
                prop:disabled=move || is_loading.get()
                required
            />
            <button
                type="submit"
                class="url-form__button"
                prop:disabled=move || is_loading.get()
            >
                {move || if is_loading.get() { "Memproses..." } else { "Roast Sekarang!" }}
            </button>
        </form>
    }
}
