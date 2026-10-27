pub mod account;
pub mod app;
pub mod auth;
pub mod cards;
pub mod db;
pub mod email;
pub mod state;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}
