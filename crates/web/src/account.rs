use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardCard {
    pub id: Uuid,
    pub target_text: String,
    pub difficulty: f32,
    pub lapses: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub total_cards: i64,
    pub due_now: i64,
    pub reviews_today: i64,
    pub current_streak_days: i64,
    pub hardest_cards: Vec<HardCard>,
}

#[cfg(feature = "ssr")]
async fn require_user() -> Result<crate::auth::UserRow, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let cookies = leptos_axum::extract::<tower_cookies::Cookies>()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    crate::auth::require_user(&state.db_pool, &cookies)
        .await
        .map_err(ServerFnError::new)
}

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

#[server]
pub async fn get_daily_new_card_limit() -> Result<Option<i32>, ServerFnError> {
    let user = require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    crate::db::daily_new_card_limit(&state.db_pool, user.id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[server]
pub async fn set_daily_new_card_limit(limit: Option<i32>) -> Result<(), ServerFnError> {
    let user = require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    crate::db::set_daily_new_card_limit(&state.db_pool, user.id, limit)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(())
}

#[server]
pub async fn user_stats() -> Result<Stats, ServerFnError> {
    let user = require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    let db_stats = crate::db::user_stats(&state.db_pool, user.id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(Stats {
        total_cards: db_stats.total_cards,
        due_now: db_stats.due_now,
        reviews_today: db_stats.reviews_today,
        current_streak_days: db_stats.current_streak_days,
        hardest_cards: db_stats
            .hardest_cards
            .into_iter()
            .map(|c| HardCard {
                id: c.id,
                target_text: c.target_text,
                difficulty: c.difficulty,
                lapses: c.lapses,
            })
            .collect(),
    })
}
