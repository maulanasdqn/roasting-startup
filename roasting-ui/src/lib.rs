pub mod components;
pub mod pages;

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use pages::HomePage;

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Title text="Hancurkan Startup-mu | Roasting Indonesia"/>
        <Meta name="description" content="Roasting startup brutal dalam bahasa Indonesia"/>
        <Stylesheet id="leptos" href="/pkg/roasting-startup.css"/>

        <Router>
            <main class="container">
                <Routes fallback=|| "Halaman tidak ditemukan">
                    <Route path=path!("/") view=HomePage/>
                </Routes>
            </main>
        </Router>
    }
}

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
