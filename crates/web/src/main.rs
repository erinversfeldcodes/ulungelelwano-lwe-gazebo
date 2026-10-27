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

    let state = AppState {
        leptos_options: leptos_options.clone(),
        db_pool,
        email_sender: web::email::EmailSender::from_env(),
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
