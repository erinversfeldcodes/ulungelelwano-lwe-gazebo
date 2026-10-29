#![cfg(feature = "ssr")]

use leptos::prelude::LeptosOptions;

#[derive(Clone)]
pub struct AppState {
    pub leptos_options: LeptosOptions,
    pub db_pool: sqlx::PgPool,
    pub email_sender: crate::email::EmailSender,
    pub storage: Option<crate::storage::Storage>,
    pub image_search: crate::image_search::ImageSearch,
}

impl axum::extract::FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}
