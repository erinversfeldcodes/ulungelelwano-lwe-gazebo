#![cfg(feature = "ssr")]

use rand::RngExt;
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

/// Hard cap on signups, matching the 4-friend trust model this app is built
/// for. Enforced here rather than as a SQL constraint, since expressing
/// "at most N rows" declaratively needs a trigger — more machinery than
/// this warrants.
pub const MAX_USERS: i64 = 4;

pub const SESSION_COOKIE: &str = "session";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub name: String,
}

fn generate_token() -> String {
    let bytes: [u8; 32] = rand::rng().random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Starts a sign-in attempt: mints a single-use token and emails a magic
/// link for it. Rejects only when the email is new *and* the 4-user cap is
/// already reached; an existing user can always request a new link.
pub async fn request_magic_link(
    pool: &PgPool,
    email_sender: &crate::email::EmailSender,
    base_url: &str,
    email: &str,
) -> Result<(), String> {
    let email = email.trim().to_lowercase();
    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

    if existing.is_none() {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;
        if count >= MAX_USERS {
            return Err("This app is limited to 4 users and is already full.".to_string());
        }
    }

    let token = generate_token();
    let expires_at = OffsetDateTime::now_utc() + Duration::minutes(30);

    sqlx::query(
        "INSERT INTO login_tokens (token, email, user_id, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(&token)
    .bind(&email)
    .bind(existing)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    let link = format!("{base_url}/auth/verify?token={token}");
    email_sender
        .send_magic_link(&email, &link)
        .await
        .map_err(|e| format!("failed to send email: {e}"))
}

/// Redeems a magic-link token: finds-or-creates the user, marks the token
/// used, and starts a session. Returns the new session token.
pub async fn verify_login_token(pool: &PgPool, token: &str) -> Result<String, String> {
    let row: Option<(String, Option<Uuid>, OffsetDateTime, Option<OffsetDateTime>)> =
        sqlx::query_as(
            "SELECT email, user_id, expires_at, used_at FROM login_tokens WHERE token = $1",
        )
        .bind(token)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

    let (email, user_id, expires_at, used_at) =
        row.ok_or_else(|| "invalid sign-in link".to_string())?;

    if used_at.is_some() {
        return Err("this sign-in link has already been used".to_string());
    }
    if expires_at < OffsetDateTime::now_utc() {
        return Err("this sign-in link has expired".to_string());
    }

    sqlx::query("UPDATE login_tokens SET used_at = now() WHERE token = $1")
        .bind(token)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    let user_id = match user_id {
        Some(id) => id,
        None => {
            let default_name = email.split('@').next().unwrap_or(&email).to_string();
            sqlx::query_scalar("INSERT INTO users (email, name) VALUES ($1, $2) RETURNING id")
                .bind(&email)
                .bind(default_name)
                .fetch_one(pool)
                .await
                .map_err(|e| e.to_string())?
        }
    };

    let session_token = generate_token();
    let expires_at = OffsetDateTime::now_utc() + Duration::days(30);
    sqlx::query("INSERT INTO sessions (token, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(&session_token)
        .bind(user_id)
        .bind(expires_at)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(session_token)
}

pub async fn user_for_session(pool: &PgPool, session_token: &str) -> Option<UserRow> {
    sqlx::query_as::<_, UserRow>(
        "SELECT u.id, u.email, u.name FROM sessions s \
         JOIN users u ON u.id = s.user_id \
         WHERE s.token = $1 AND s.expires_at > now()",
    )
    .bind(session_token)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

pub async fn delete_session(pool: &PgPool, session_token: &str) {
    let _ = sqlx::query("DELETE FROM sessions WHERE token = $1")
        .bind(session_token)
        .execute(pool)
        .await;
}

/// Reads the session cookie and resolves it to the signed-in user, if any.
pub async fn current_user(pool: &PgPool, cookies: &tower_cookies::Cookies) -> Option<UserRow> {
    let token = cookies.get(SESSION_COOKIE)?;
    user_for_session(pool, token.value()).await
}

/// Like [`current_user`], but fails loudly — for server functions that
/// require a signed-in user rather than treating one as optional.
pub async fn require_user(
    pool: &PgPool,
    cookies: &tower_cookies::Cookies,
) -> Result<UserRow, String> {
    current_user(pool, cookies)
        .await
        .ok_or_else(|| "not signed in".to_string())
}
