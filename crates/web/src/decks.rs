use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deck {
    pub id: Uuid,
    pub name: String,
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
pub async fn list_decks() -> Result<Vec<Deck>, ServerFnError> {
    require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    let rows = crate::db::list_decks(&state.db_pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|r| Deck {
            id: r.id,
            name: r.name,
        })
        .collect())
}

#[server]
pub async fn create_deck(name: String) -> Result<(), ServerFnError> {
    let user = require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    crate::db::create_deck(&state.db_pool, user.id, &name)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(())
}

#[server]
pub async fn add_card_to_deck(deck_id: Uuid, card_id: Uuid) -> Result<(), ServerFnError> {
    require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    crate::db::add_card_to_deck(&state.db_pool, deck_id, card_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(())
}

#[server]
pub async fn remove_card_from_deck(deck_id: Uuid, card_id: Uuid) -> Result<(), ServerFnError> {
    require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    crate::db::remove_card_from_deck(&state.db_pool, deck_id, card_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(())
}
