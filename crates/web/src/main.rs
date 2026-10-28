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

/// Uploads one `image`/`audio`/`video` multipart field to storage.
/// Best-effort, same as before this was extracted: with no storage
/// configured (e.g. local dev without the S3_* env vars set), an attached
/// file is silently skipped rather than failing the whole submission —
/// the pasted-URL path (image only) still works either way.
#[cfg(feature = "ssr")]
async fn upload_media_field(
    state: &AppState,
    field: axum::extract::multipart::Field<'_>,
    kind: &str,
) -> Result<Option<String>, (axum::http::StatusCode, String)> {
    let content_type = field.content_type().map(str::to_string);
    let data = field
        .bytes()
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    if data.is_empty() {
        return Ok(None);
    }
    match (&state.storage, content_type) {
        (Some(storage), Some(content_type)) => {
            let key = format!("cards/{}", random_key());
            let url = storage
                .upload(&key, &content_type, &data)
                .await
                .map_err(|e| {
                    log::error!("card {kind} upload failed: {e}");
                    (axum::http::StatusCode::BAD_GATEWAY, e)
                })?;
            Ok(Some(url))
        }
        (None, _) => {
            log::warn!(
                "card {kind} upload attached but no storage is configured — dropping it silently"
            );
            Ok(None)
        }
        (Some(_), None) => {
            log::warn!("card {kind} upload attached with no content-type — dropping it");
            Ok(None)
        }
    }
}

/// Extracts a `YouTube` video ID from any of the common share-link shapes
/// users actually paste (`youtube.com/watch?v=...`, `youtu.be/...`, and
/// an already-canonical `youtube.com/embed/...`), rejecting anything else
/// outright rather than silently storing an unplayable link. Deliberately
/// not a full URL parser — just enough to cover what a "share" or "copy
/// link" button on youtube.com actually produces.
#[cfg(feature = "ssr")]
fn extract_youtube_video_id(input: &str) -> Option<&str> {
    let without_scheme = input
        .trim()
        .strip_prefix("https://")
        .or_else(|| input.trim().strip_prefix("http://"))?;
    let without_host_prefix = without_scheme
        .strip_prefix("www.")
        .or_else(|| without_scheme.strip_prefix("m."))
        .unwrap_or(without_scheme);

    let id = if let Some(rest) = without_host_prefix.strip_prefix("youtu.be/") {
        rest.split(['?', '&', '#']).next()?
    } else if let Some(rest) = without_host_prefix.strip_prefix("youtube.com/embed/") {
        rest.split(['?', '&', '#']).next()?
    } else {
        let rest = without_host_prefix.strip_prefix("youtube.com/watch")?;
        rest.strip_prefix('?')?
            .split('&')
            .find_map(|pair| pair.strip_prefix("v="))?
            .split(['&', '#'])
            .next()?
    };

    // Real YouTube video IDs are always exactly 11 URL-safe characters.
    let is_plausible_id = id.len() == 11
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    is_plausible_id.then_some(id)
}

/// Normalizes whatever `YouTube` share link a user pasted into the one
/// canonical embed-URL form the review/edit UI actually renders
/// (`<iframe src=...>`), or an error message if it isn't recognizably a
/// `YouTube` link at all.
#[cfg(feature = "ssr")]
fn youtube_embed_url(input: &str) -> Result<String, String> {
    extract_youtube_video_id(input)
        .map(|id| format!("https://www.youtube.com/embed/{id}"))
        .ok_or_else(|| "that doesn't look like a YouTube video link".to_string())
}

// Every bool here is an independent, orthogonal form flag (a value and its
// own "remove this" checkbox, per media field) rather than a set of states
// that could collapse into an enum, so the usual fix for this lint doesn't
// apply cleanly.
#[cfg(feature = "ssr")]
#[allow(clippy::struct_excessive_bools)]
struct CardFormFields {
    target_text: String,
    tags_raw: String,
    two_way: bool,
    image_url: Option<String>,
    audio_url: Option<String>,
    video_url: Option<String>,
    // `Some` only once validated by `youtube_embed_url` — an invalid
    // pasted link fails the whole submission (see parse_card_multipart)
    // rather than being silently dropped or stored unplayable.
    youtube_url: Option<String>,
    // Only meaningful on the edit form, which is the only one that ever
    // renders these checkboxes (there's nothing to remove from a brand
    // new card) — a new upload/link for the same field always wins over a
    // remove request, since submitting both at once only makes sense as
    // "replace", not "replace, then immediately delete what I just set".
    remove_image: bool,
    remove_audio: bool,
    remove_video: bool,
    remove_youtube: bool,
}

/// Pulls every field out of a card create/edit form submission: text
/// fields, the `two_way` checkbox, whichever of image/audio/video were
/// attached (each independent and optional — a card can have any
/// combination, including none), and the edit form's "remove" checkboxes.
#[cfg(feature = "ssr")]
async fn parse_card_multipart(
    state: &AppState,
    multipart: &mut axum::extract::Multipart,
) -> Result<CardFormFields, (axum::http::StatusCode, String)> {
    let mut fields = CardFormFields {
        target_text: String::new(),
        tags_raw: String::new(),
        two_way: false,
        image_url: None,
        audio_url: None,
        video_url: None,
        youtube_url: None,
        remove_image: false,
        remove_audio: false,
        remove_video: false,
        remove_youtube: false,
    };

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "target_text" => fields.target_text = field.text().await.unwrap_or_default(),
            "tags" => fields.tags_raw = field.text().await.unwrap_or_default(),
            // A checkbox only appears in the multipart body at all when
            // checked, so this field's mere presence means true.
            "two_way" => fields.two_way = true,
            "remove_image" => fields.remove_image = true,
            "remove_audio" => fields.remove_audio = true,
            "remove_video" => fields.remove_video = true,
            "remove_youtube" => fields.remove_youtube = true,
            "image" => fields.image_url = upload_media_field(state, field, "image").await?,
            "audio" => fields.audio_url = upload_media_field(state, field, "audio").await?,
            "video" => fields.video_url = upload_media_field(state, field, "video").await?,
            "youtube_url" => {
                let raw = field.text().await.unwrap_or_default();
                let trimmed = raw.trim();
                if !trimmed.is_empty() {
                    fields.youtube_url = Some(
                        youtube_embed_url(trimmed)
                            .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?,
                    );
                }
            }
            _ => {}
        }
    }

    Ok(fields)
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

    let fields = parse_card_multipart(&state, &mut multipart).await?;

    web::db::create_card(
        &state.db_pool,
        user.id,
        web::db::NewCard {
            image_url: fields.image_url.as_deref().unwrap_or(""),
            audio_url: fields.audio_url.as_deref().unwrap_or(""),
            video_url: fields.video_url.as_deref().unwrap_or(""),
            youtube_url: fields.youtube_url.as_deref().unwrap_or(""),
            target_text: &fields.target_text,
            two_way: fields.two_way,
        },
        &parse_tags(&fields.tags_raw),
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

    let fields = parse_card_multipart(&state, &mut multipart).await?;
    // A new upload wins over a remove request (see CardFormFields); a
    // remove request wins over keeping the existing value; otherwise the
    // existing value is kept as-is.
    let image_url = fields.image_url.unwrap_or_else(|| {
        if fields.remove_image {
            String::new()
        } else {
            existing.image_url.unwrap_or_default()
        }
    });
    let audio_url = fields.audio_url.unwrap_or_else(|| {
        if fields.remove_audio {
            String::new()
        } else {
            existing.audio_url.unwrap_or_default()
        }
    });
    let video_url = fields.video_url.unwrap_or_else(|| {
        if fields.remove_video {
            String::new()
        } else {
            existing.video_url.unwrap_or_default()
        }
    });
    let youtube_url = fields.youtube_url.unwrap_or_else(|| {
        if fields.remove_youtube {
            String::new()
        } else {
            existing.youtube_url.unwrap_or_default()
        }
    });

    web::db::update_card(
        &state.db_pool,
        card_id,
        web::db::NewCard {
            image_url: &image_url,
            audio_url: &audio_url,
            video_url: &video_url,
            youtube_url: &youtube_url,
            target_text: &fields.target_text,
            two_way: fields.two_way,
        },
        &parse_tags(&fields.tags_raw),
    )
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::response::Redirect::to(&format!("/cards/{card_id}")))
}

/// Axum's default multipart body limit (2MB) comfortably fits an image
/// but not a voice clip or a short video, so card uploads need a higher
/// ceiling. Applied to the whole router rather than scoped to just the
/// card routes — every other route's payloads are tiny text/JSON, so a
/// generous shared ceiling costs nothing there.
#[cfg(feature = "ssr")]
const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;

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
        .layer(axum::extract::DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

#[cfg(not(feature = "ssr"))]
fn main() {}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::youtube_embed_url;

    #[test]
    fn accepts_a_watch_url() {
        assert_eq!(
            youtube_embed_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap(),
            "https://www.youtube.com/embed/dQw4w9WgXcQ"
        );
    }

    #[test]
    fn accepts_a_watch_url_with_v_not_in_first_position() {
        assert_eq!(
            youtube_embed_url("https://youtube.com/watch?list=PLxyz&v=dQw4w9WgXcQ&t=30s").unwrap(),
            "https://www.youtube.com/embed/dQw4w9WgXcQ"
        );
    }

    #[test]
    fn accepts_a_shortened_youtu_be_url() {
        assert_eq!(
            youtube_embed_url("https://youtu.be/dQw4w9WgXcQ?t=10").unwrap(),
            "https://www.youtube.com/embed/dQw4w9WgXcQ"
        );
    }

    #[test]
    fn accepts_an_already_canonical_embed_url() {
        assert_eq!(
            youtube_embed_url("https://www.youtube.com/embed/dQw4w9WgXcQ").unwrap(),
            "https://www.youtube.com/embed/dQw4w9WgXcQ"
        );
    }

    #[test]
    fn accepts_a_mobile_subdomain_url() {
        assert_eq!(
            youtube_embed_url("https://m.youtube.com/watch?v=dQw4w9WgXcQ").unwrap(),
            "https://www.youtube.com/embed/dQw4w9WgXcQ"
        );
    }

    #[test]
    fn rejects_a_non_youtube_url() {
        assert!(youtube_embed_url("https://vimeo.com/123456").is_err());
    }

    #[test]
    fn rejects_a_watch_url_with_no_video_id() {
        assert!(youtube_embed_url("https://www.youtube.com/watch?list=PLxyz").is_err());
    }

    #[test]
    fn rejects_plain_text_that_is_not_a_url_at_all() {
        assert!(youtube_embed_url("not a url").is_err());
    }
}
