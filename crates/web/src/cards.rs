use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub id: Uuid,
    pub image_url: Option<String>,
    pub target_text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum GradeInput {
    Again,
    Hard,
    Good,
    Easy,
}

#[cfg(feature = "ssr")]
impl From<GradeInput> for domain::srs::Grade {
    fn from(grade: GradeInput) -> Self {
        match grade {
            GradeInput::Again => domain::srs::Grade::Again,
            GradeInput::Hard => domain::srs::Grade::Hard,
            GradeInput::Good => domain::srs::Grade::Good,
            GradeInput::Easy => domain::srs::Grade::Easy,
        }
    }
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
pub async fn list_cards() -> Result<Vec<Card>, ServerFnError> {
    require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    let rows = crate::db::list_cards(&state.db_pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|r| Card {
            id: r.id,
            image_url: r.image_url,
            target_text: r.target_text,
        })
        .collect())
}

#[server]
pub async fn create_card(
    image_url: String,
    target_text: String,
    tags: String,
) -> Result<(), ServerFnError> {
    let user = require_user().await?;
    let state = expect_context::<crate::state::AppState>();

    let tags: Vec<String> = tags
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    crate::db::create_card(&state.db_pool, user.id, &image_url, &target_text, &tags)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(())
}

#[server]
pub async fn due_cards() -> Result<Vec<Card>, ServerFnError> {
    let user = require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    let rows = crate::db::due_cards(&state.db_pool, user.id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|r| Card {
            id: r.id,
            image_url: r.image_url,
            target_text: r.target_text,
        })
        .collect())
}

#[server]
pub async fn grade_card(card_id: Uuid, grade: GradeInput) -> Result<(), ServerFnError> {
    let user = require_user().await?;
    let state = expect_context::<crate::state::AppState>();
    let current = crate::db::get_review_state(&state.db_pool, user.id, card_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let next = current.schedule(grade.into());
    crate::db::save_review_state(&state.db_pool, user.id, card_id, next)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    Ok(())
}
