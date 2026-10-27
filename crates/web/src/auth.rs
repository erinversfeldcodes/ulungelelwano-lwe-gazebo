#![cfg(feature = "ssr")]
// Error messages here are already the full user-facing explanation; an
// `# Errors` doc section would just restate them.
#![allow(clippy::missing_errors_doc)]

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
    use std::fmt::Write as _;

    let bytes: [u8; 32] = rand::rng().random();
    let mut token = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(token, "{b:02x}").unwrap();
    }
    token
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
            log::warn!("auth: sign-up rejected at the {MAX_USERS}-user cap for {email}");
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
        .map_err(|e| {
            log::error!("auth: failed to send magic-link email to {email}: {e}");
            format!("failed to send email: {e}")
        })
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

    let Some((email, user_id, expires_at, used_at)) = row else {
        log::warn!("auth: sign-in attempted with an unknown token");
        return Err("invalid sign-in link".to_string());
    };

    if used_at.is_some() {
        log::warn!("auth: sign-in attempted with an already-used token for {email}");
        return Err("this sign-in link has already been used".to_string());
    }
    if expires_at < OffsetDateTime::now_utc() {
        log::warn!("auth: sign-in attempted with an expired token for {email}");
        return Err("this sign-in link has expired".to_string());
    }

    sqlx::query("UPDATE login_tokens SET used_at = now() WHERE token = $1")
        .bind(token)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    let user_id = if let Some(id) = user_id {
        id
    } else {
        let default_name = email.split('@').next().unwrap_or(&email).to_string();
        sqlx::query_scalar("INSERT INTO users (email, name) VALUES ($1, $2) RETURNING id")
            .bind(&email)
            .bind(default_name)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn token_for(pool: &PgPool, email: &str) -> String {
        sqlx::query_scalar(
            "SELECT token FROM login_tokens WHERE email = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn new_signup_creates_user_with_name_from_email(pool: PgPool) {
        let sender = crate::email::EmailSender::Console;
        request_magic_link(&pool, &sender, "http://localhost:3000", "New@Example.com")
            .await
            .unwrap();

        let token = token_for(&pool, "new@example.com").await;
        let session_token = verify_login_token(&pool, &token).await.unwrap();

        let user = user_for_session(&pool, &session_token).await.unwrap();
        assert_eq!(user.email, "new@example.com", "email should be lowercased");
        assert_eq!(user.name, "new");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn token_cannot_be_redeemed_twice(pool: PgPool) {
        let sender = crate::email::EmailSender::Console;
        request_magic_link(&pool, &sender, "http://localhost:3000", "reuse@example.com")
            .await
            .unwrap();
        let token = token_for(&pool, "reuse@example.com").await;

        verify_login_token(&pool, &token).await.unwrap();
        let second_attempt = verify_login_token(&pool, &token).await;
        assert!(second_attempt.is_err());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn expired_token_is_rejected(pool: PgPool) {
        let token = "already-expired-token";
        sqlx::query(
            "INSERT INTO login_tokens (token, email, user_id, expires_at) \
             VALUES ($1, $2, NULL, now() - interval '1 hour')",
        )
        .bind(token)
        .bind("expired@example.com")
        .execute(&pool)
        .await
        .unwrap();

        let result = verify_login_token(&pool, token).await;
        assert!(result.is_err());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn unknown_token_is_rejected(pool: PgPool) {
        let result = verify_login_token(&pool, "never-issued").await;
        assert!(result.is_err());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn fifth_new_signup_is_rejected_once_at_the_cap(pool: PgPool) {
        let sender = crate::email::EmailSender::Console;
        for i in 0..MAX_USERS {
            let email = format!("user{i}@example.com");
            request_magic_link(&pool, &sender, "http://localhost:3000", &email)
                .await
                .unwrap_or_else(|e| panic!("user {i} should be allowed to sign up: {e}"));
            let token = token_for(&pool, &email).await;
            verify_login_token(&pool, &token).await.unwrap();
        }

        let fifth =
            request_magic_link(&pool, &sender, "http://localhost:3000", "user4@example.com").await;
        assert!(
            fifth.is_err(),
            "a 5th new signup should be rejected at the cap"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn existing_user_can_still_sign_in_at_the_cap(pool: PgPool) {
        let sender = crate::email::EmailSender::Console;
        let mut emails = Vec::new();
        for i in 0..MAX_USERS {
            let email = format!("user{i}@example.com");
            request_magic_link(&pool, &sender, "http://localhost:3000", &email)
                .await
                .unwrap();
            let token = token_for(&pool, &email).await;
            verify_login_token(&pool, &token).await.unwrap();
            emails.push(email);
        }

        // At the cap now — an *existing* user should still be able to sign in again.
        let result = request_magic_link(&pool, &sender, "http://localhost:3000", &emails[0]).await;
        assert!(result.is_ok());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn deleted_session_no_longer_resolves_to_a_user(pool: PgPool) {
        let sender = crate::email::EmailSender::Console;
        request_magic_link(
            &pool,
            &sender,
            "http://localhost:3000",
            "logout@example.com",
        )
        .await
        .unwrap();
        let token = token_for(&pool, "logout@example.com").await;
        let session_token = verify_login_token(&pool, &token).await.unwrap();

        assert!(user_for_session(&pool, &session_token).await.is_some());
        delete_session(&pool, &session_token).await;
        assert!(user_for_session(&pool, &session_token).await.is_none());
    }
}
