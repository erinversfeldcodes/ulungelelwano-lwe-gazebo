#![cfg(feature = "ssr")]

use domain::srs::ReviewState;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CardRow {
    pub id: Uuid,
    pub image_url: Option<String>,
    pub target_text: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DueCardRow {
    pub id: Uuid,
    pub image_url: Option<String>,
    pub target_text: String,
}

pub async fn list_cards(pool: &PgPool) -> sqlx::Result<Vec<CardRow>> {
    sqlx::query_as::<_, CardRow>(
        "SELECT id, image_url, target_text FROM cards ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
}

pub async fn create_card(
    pool: &PgPool,
    created_by: Uuid,
    image_url: &str,
    target_text: &str,
    tags: &[String],
) -> sqlx::Result<Uuid> {
    let mut tx = pool.begin().await?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO cards (card_type, image_url, target_text, created_by) \
         VALUES ('image_to_target', $1, $2, $3) RETURNING id",
    )
    .bind(image_url)
    .bind(target_text)
    .bind(created_by)
    .fetch_one(&mut *tx)
    .await?;

    for tag in tags {
        sqlx::query("INSERT INTO card_tags (card_id, tag) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(tag)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(id)
}

/// Cards due for review: either never reviewed by this user, or past due.
pub async fn due_cards(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<DueCardRow>> {
    sqlx::query_as::<_, DueCardRow>(
        "SELECT c.id, c.image_url, c.target_text \
         FROM cards c \
         LEFT JOIN review_state rs ON rs.card_id = c.id AND rs.user_id = $1 \
         WHERE rs.due_at IS NULL OR rs.due_at <= now() \
         ORDER BY rs.due_at NULLS FIRST",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn get_review_state(
    pool: &PgPool,
    user_id: Uuid,
    card_id: Uuid,
) -> sqlx::Result<ReviewState> {
    let row: Option<(f32, f32, i32)> = sqlx::query_as(
        "SELECT ease_factor, interval_days, repetitions FROM review_state \
         WHERE user_id = $1 AND card_id = $2",
    )
    .bind(user_id)
    .bind(card_id)
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some((ease_factor, interval_days, repetitions)) => ReviewState {
            ease_factor,
            interval_days,
            repetitions: repetitions as u32,
        },
        None => ReviewState::default(),
    })
}

pub async fn save_review_state(
    pool: &PgPool,
    user_id: Uuid,
    card_id: Uuid,
    state: ReviewState,
) -> sqlx::Result<()> {
    let due_at = OffsetDateTime::now_utc()
        + time::Duration::seconds((state.interval_days * 86_400.0) as i64);

    sqlx::query(
        "INSERT INTO review_state (user_id, card_id, ease_factor, interval_days, repetitions, due_at, last_reviewed_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now()) \
         ON CONFLICT (user_id, card_id) DO UPDATE SET \
            ease_factor = EXCLUDED.ease_factor, \
            interval_days = EXCLUDED.interval_days, \
            repetitions = EXCLUDED.repetitions, \
            due_at = EXCLUDED.due_at, \
            last_reviewed_at = EXCLUDED.last_reviewed_at",
    )
    .bind(user_id)
    .bind(card_id)
    .bind(state.ease_factor)
    .bind(state.interval_days)
    .bind(state.repetitions as i32)
    .bind(due_at)
    .execute(pool)
    .await?;

    Ok(())
}
