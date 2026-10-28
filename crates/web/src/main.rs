#[cfg(feature = "ssr")]
use web::state::AppState;

#[cfg(feature = "ssr")]
async fn health(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::http::StatusCode {
    match sqlx::query("SELECT 1").execute(&state.db_pool).await {
        Ok(_) => axum::http::StatusCode::OK,
        Err(_) => axum::http::StatusCode::SERVICE_UNAVAILABLE,
    }
}

#[cfg(feature = "ssr")]
#[derive(serde::Deserialize)]
struct VerifyQuery {
    token: String,
}

#[cfg(feature = "ssr")]
async fn auth_verify(
    axum::extract::State(state): axum::extract::State<AppState>,
    cookies: tower_cookies::Cookies,
    axum::extract::Query(query): axum::extract::Query<VerifyQuery>,
) -> axum::response::Redirect {
    match web::auth::verify_login_token(&state.db_pool, &query.token).await {
        Ok(session_token) => {
            let cookie = tower_cookies::Cookie::build((web::auth::SESSION_COOKIE, session_token))
                .path("/")
                .http_only(true)
                .secure(true)
                .same_site(tower_cookies::cookie::SameSite::Lax)
                .max_age(tower_cookies::cookie::time::Duration::days(30))
                .build();
            cookies.add(cookie);
            axum::response::Redirect::to("/")
        }
        Err(_) => axum::response::Redirect::to("/login"),
    }
}

/// Hit by CI right after every deploy (prod and PR previews) to prove the
/// Resend integration actually works end to end — not a liveness probe, so
/// it's deliberately not wired into fly.toml's automatic health checks,
/// since it sends a real email on every call.
#[cfg(feature = "ssr")]
async fn email_check(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> axum::http::StatusCode {
    let expected_token = match std::env::var("INTERNAL_CHECK_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => return axum::http::StatusCode::UNAUTHORIZED,
    };
    let provided_token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if provided_token != Some(expected_token.as_str()) {
        return axum::http::StatusCode::UNAUTHORIZED;
    }

    let Ok(recipient) = std::env::var("TEST_EMAIL_RECIPIENT") else {
        log::info!("email-check: TEST_EMAIL_RECIPIENT not set, skipping");
        return axum::http::StatusCode::OK;
    };

    match state.email_sender.send_test_email(&recipient).await {
        Ok(()) => {
            log::info!("email-check: test email sent to {recipient}");
            axum::http::StatusCode::OK
        }
        Err(e) => {
            log::error!("email-check: failed to send test email: {e}");
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

/// Same shape and purpose as `email_check`: hit by CI right after every
/// deploy to prove the configured S3-compatible storage (Tigris in
/// prod/preview) is actually reachable and writable, not just that env
/// vars are set. Not a liveness probe, so not wired into fly.toml's
/// automatic health checks — it does a real write on every call.
#[cfg(feature = "ssr")]
async fn storage_check(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> axum::http::StatusCode {
    let expected_token = match std::env::var("INTERNAL_CHECK_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => return axum::http::StatusCode::UNAUTHORIZED,
    };
    let provided_token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if provided_token != Some(expected_token.as_str()) {
        return axum::http::StatusCode::UNAUTHORIZED;
    }

    let Some(storage) = &state.storage else {
        log::info!("storage-check: no S3_* env vars set, skipping");
        return axum::http::StatusCode::OK;
    };

    match storage
        .upload(
            "internal/storage-check.txt",
            "text/plain",
            b"gazebo storage-check",
        )
        .await
    {
        Ok(url) => {
            log::info!("storage-check: upload succeeded, reachable at {url}");
            axum::http::StatusCode::OK
        }
        Err(e) => {
            log::error!("storage-check: upload failed: {e}");
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

fn random_key() -> String {
    use rand::RngExt;
    use std::fmt::Write as _;

    let bytes: [u8; 16] = rand::rng().random();
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(out, "{b:02x}").unwrap();
    }
    out
}

/// Pulls `target_text`/`tags` fields and an optional `image` file out of a
/// card create/edit form submission. Uploading is best-effort: with no
/// storage configured (e.g. local dev without the S3_* env vars set), an
/// attached image is silently skipped rather than failing the whole
/// submission — the pasted-URL path still works either way.
#[cfg(feature = "ssr")]
async fn parse_card_multipart(
    state: &AppState,
    multipart: &mut axum::extract::Multipart,
) -> Result<(String, String, Option<String>), (axum::http::StatusCode, String)> {
    let mut target_text = String::new();
    let mut tags_raw = String::new();
    let mut uploaded_url: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "target_text" => target_text = field.text().await.unwrap_or_default(),
            "tags" => tags_raw = field.text().await.unwrap_or_default(),
            "image" => {
                let content_type = field.content_type().map(str::to_string);
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
                if !data.is_empty() {
                    match (&state.storage, content_type) {
                        (Some(storage), Some(content_type)) => {
                            let key = format!("cards/{}", random_key());
                            let url =
                                storage
                                    .upload(&key, &content_type, &data)
                                    .await
                                    .map_err(|e| {
                                        log::error!("card image upload failed: {e}");
                                        (axum::http::StatusCode::BAD_GATEWAY, e)
                                    })?;
                            uploaded_url = Some(url);
                        }
                        (None, _) => {
                            log::warn!(
                                "card image upload attached but no storage is configured — dropping it silently"
                            );
                        }
                        (Some(_), None) => {
                            log::warn!(
                                "card image upload attached with no content-type — dropping it"
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok((target_text, tags_raw, uploaded_url))
}

#[cfg(feature = "ssr")]
fn parse_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Plain multipart form POST, not a Leptos server function — file uploads
/// need `multipart/form-data`, which is far simpler to handle as an
/// ordinary Axum route (a classic full-page form submit) than to wire
/// through Leptos's server-action/WASM machinery.
#[cfg(feature = "ssr")]
async fn cards_new(
    axum::extract::State(state): axum::extract::State<AppState>,
    cookies: tower_cookies::Cookies,
    mut multipart: axum::extract::Multipart,
) -> Result<axum::response::Redirect, (axum::http::StatusCode, String)> {
    let user = web::auth::current_user(&state.db_pool, &cookies)
        .await
        .ok_or((
            axum::http::StatusCode::UNAUTHORIZED,
            "not signed in".to_string(),
        ))?;

    let (target_text, tags_raw, image_url) = parse_card_multipart(&state, &mut multipart).await?;

    web::db::create_card(
        &state.db_pool,
        user.id,
        image_url.as_deref().unwrap_or(""),
        &target_text,
        &parse_tags(&tags_raw),
    )
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::response::Redirect::to("/"))
}

#[cfg(feature = "ssr")]
async fn cards_update(
    axum::extract::State(state): axum::extract::State<AppState>,
    cookies: tower_cookies::Cookies,
    axum::extract::Path(card_id): axum::extract::Path<uuid::Uuid>,
    mut multipart: axum::extract::Multipart,
) -> Result<axum::response::Redirect, (axum::http::StatusCode, String)> {
    web::auth::current_user(&state.db_pool, &cookies)
        .await
        .ok_or((
            axum::http::StatusCode::UNAUTHORIZED,
            "not signed in".to_string(),
        ))?;

    let existing = web::db::get_card(&state.db_pool, card_id)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((
            axum::http::StatusCode::NOT_FOUND,
            "card not found".to_string(),
        ))?;

    let (target_text, tags_raw, uploaded_url) =
        parse_card_multipart(&state, &mut multipart).await?;
    let image_url = uploaded_url.unwrap_or_else(|| existing.image_url.unwrap_or_default());

    web::db::update_card(
        &state.db_pool,
        card_id,
        &image_url,
        &target_text,
        &parse_tags(&tags_raw),
    )
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::response::Redirect::to(&format!("/cards/{card_id}")))
}

#[cfg(feature = "ssr")]
async fn auth_logout(
    axum::extract::State(state): axum::extract::State<AppState>,
    cookies: tower_cookies::Cookies,
) -> axum::response::Redirect {
    if let Some(cookie) = cookies.get(web::auth::SESSION_COOKIE) {
        web::auth::delete_session(&state.db_pool, cookie.value()).await;
    }
    cookies.remove(tower_cookies::Cookie::new(web::auth::SESSION_COOKIE, ""));
    axum::response::Redirect::to("/login")
}

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use leptos::prelude::*;
    use leptos_axum::{LeptosRoutes, generate_route_list};
    use web::app::{App, shell};

    env_logger::init();

    let conf = get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(App);

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("failed to connect to the database");

    // Embedded at compile time (no runtime file access needed, so this
    // works unchanged in the minimal Docker runtime image) and applied on
    // every boot. Neither `preview.yml` nor `deploy.yml` ever ran
    // migrations against the real Neon branches they target — only
    // `ci.yml`'s ephemeral local Postgres got them — so staging and
    // production both still had zero tables despite "successful" deploys.
    // Idempotent via sqlx's own tracking table, so this is safe to run
    // alongside that pre-existing CI step too.
    sqlx::migrate!("../../migrations")
        .run(&db_pool)
        .await
        .expect("failed to run database migrations");

    let state = AppState {
        leptos_options: leptos_options.clone(),
        db_pool,
        email_sender: web::email::EmailSender::from_env(),
        storage: web::storage::Storage::from_env(),
    };

    let app = Router::new()
        .leptos_routes(&state, routes, {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .route("/health", axum::routing::get(health))
        .route("/auth/verify", axum::routing::get(auth_verify))
        .route("/auth/logout", axum::routing::get(auth_logout))
        .route("/internal/email-check", axum::routing::get(email_check))
        .route("/internal/storage-check", axum::routing::get(storage_check))
        .route("/cards/new", axum::routing::post(cards_new))
        .route("/cards/{id}", axum::routing::post(cards_update))
        .fallback(leptos_axum::file_and_error_handler::<AppState, _>(shell))
        .layer(tower_cookies::CookieManagerLayer::new())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

#[cfg(not(feature = "ssr"))]
fn main() {}
