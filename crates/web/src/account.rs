use leptos::prelude::*;

#[server]
pub async fn request_magic_link(email: String) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let base_url =
        std::env::var("APP_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    crate::auth::request_magic_link(&state.db_pool, &state.email_sender, &base_url, &email)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn current_user_name() -> Result<Option<String>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let cookies = leptos_axum::extract::<tower_cookies::Cookies>()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(crate::auth::current_user(&state.db_pool, &cookies)
        .await
        .map(|u| u.name))
}
