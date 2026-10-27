// The #[server] macro's generated client-side (hydrate) stub trips this —
// nothing in code we wrote.
#![allow(clippy::unused_async_trait_impl)]

pub mod account;
pub mod app;
pub mod auth;
pub mod cards;
pub mod db;
pub mod decks;
pub mod email;
pub mod state;
pub mod storage;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}
