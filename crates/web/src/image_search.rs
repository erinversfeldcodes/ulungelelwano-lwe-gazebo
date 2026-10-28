#![cfg(feature = "ssr")]
// The Err(String) on each of these already is the full explanation; an
// `# Errors` doc section would just restate it.
#![allow(clippy::missing_errors_doc)]

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// One image search result, already normalized across providers — just
/// enough to render a thumbnail and let the user pick it. `provider` +
/// `id` is everything [`ImageSearch::fetch_by_id`] needs to re-fetch a
/// trusted, fresh copy of the full-resolution image server-side; the
/// thumbnail URL itself is only ever rendered directly in the browser,
/// never fetched by this server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageResult {
    pub provider: String,
    pub id: String,
    pub thumbnail_url: String,
    pub description: String,
}

const MAX_RESULTS_PER_PROVIDER: u32 = 12;
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
// reqwest's default client has no timeout at all — a slow or unreachable
// provider would otherwise hang a search indefinitely instead of just
// being treated as that one provider failing (see `search`'s own graceful
// degradation). Generous enough to cover a full-resolution image download
// in `fetch_by_id`, not just the smaller search/metadata calls.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
// Refetch an Openverse token a bit before it actually expires, so a
// request never races a token that dies mid-flight.
const TOKEN_EXPIRY_SAFETY_MARGIN_SECS: u64 = 60;

fn new_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("building the image-search HTTP client should never fail")
}

struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

/// Combined image search across Pixabay and Openverse. Pixabay needs its
/// own API key (`PIXABAY_API_KEY`) and is simply skipped when unset.
/// Openverse works without credentials too, but anonymous Openverse
/// access is rate-limited per source IP — on a shared pool like GitHub
/// Actions runners, that quota is effectively already exhausted by
/// everyone else's CI traffic, which silently starves searches down to
/// zero Openverse results. `OPENVERSE_CLIENT_ID`/`OPENVERSE_CLIENT_SECRET`
/// (free to register at <https://api.openverse.org/v1/auth_tokens/register/>)
/// moves Openverse requests onto an `OAuth2` client-credentials token
/// instead, which gets its own quota independent of the requesting IP.
/// Still optional — unset, Openverse just falls back to (less reliable)
/// anonymous access, same "degrade, don't fail" spirit as `Storage`/
/// `EmailSender`.
#[derive(Clone)]
pub struct ImageSearch {
    client: reqwest::Client,
    pixabay_key: Option<String>,
    pixabay_base: String,
    openverse_base: String,
    openverse_credentials: Option<(String, String)>,
    openverse_token: Arc<Mutex<Option<CachedToken>>>,
}

impl ImageSearch {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            client: new_http_client(),
            pixabay_key: non_empty_env("PIXABAY_API_KEY"),
            pixabay_base: "https://pixabay.com".to_string(),
            openverse_base: "https://api.openverse.org".to_string(),
            openverse_credentials: openverse_credentials_from_env(),
            openverse_token: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    fn test_with(
        pixabay_base: String,
        openverse_base: String,
        pixabay_key: Option<String>,
        openverse_credentials: Option<(String, String)>,
    ) -> Self {
        Self {
            client: new_http_client(),
            pixabay_key,
            pixabay_base,
            openverse_base,
            openverse_credentials,
            openverse_token: Arc::new(Mutex::new(None)),
        }
    }

    /// Queries every available provider concurrently and keeps going even
    /// if one provider's request fails outright — a Pixabay outage
    /// shouldn't mean zero results when Openverse is fine. Order is
    /// Pixabay, then Openverse; not interleaved, simplicity over a fancier
    /// merge nobody asked for.
    pub async fn search(&self, query: &str) -> Vec<ImageResult> {
        let (pixabay, openverse) =
            tokio::join!(self.search_pixabay(query), self.search_openverse(query));
        let mut results = Vec::new();
        for (provider, outcome) in [("pixabay", pixabay), ("openverse", openverse)] {
            match outcome {
                Ok(mut found) => results.append(&mut found),
                Err(e) => log::warn!("image search: {provider} request failed: {e}"),
            }
        }
        results
    }

    async fn search_pixabay(&self, query: &str) -> Result<Vec<ImageResult>, String> {
        #[derive(Deserialize)]
        struct Response {
            hits: Vec<Hit>,
        }
        #[derive(Deserialize)]
        struct Hit {
            id: u64,
            #[serde(rename = "previewURL")]
            preview_url: String,
            tags: String,
        }

        let Some(key) = &self.pixabay_key else {
            return Ok(Vec::new());
        };

        // Pixabay's API rejects per_page below 3.
        let per_page = MAX_RESULTS_PER_PROVIDER.max(3).to_string();
        let response: Response = self
            .client
            .get(format!("{}/api/", self.pixabay_base))
            .query(&[
                ("key", key.clone()),
                ("q", query.to_string()),
                ("image_type", "photo".to_string()),
                ("per_page", per_page),
            ])
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

        Ok(response
            .hits
            .into_iter()
            .map(|h| ImageResult {
                provider: "pixabay".to_string(),
                id: h.id.to_string(),
                thumbnail_url: h.preview_url,
                description: h.tags,
            })
            .collect())
    }

    async fn search_openverse(&self, query: &str) -> Result<Vec<ImageResult>, String> {
        #[derive(Deserialize)]
        struct Response {
            results: Vec<Item>,
        }
        #[derive(Deserialize)]
        struct Item {
            id: String,
            thumbnail: String,
            title: Option<String>,
        }

        let token = self.openverse_bearer_token().await;
        let mut request = self
            .client
            .get(format!("{}/v1/images/", self.openverse_base))
            .query(&[
                ("q", query.to_string()),
                ("page_size", MAX_RESULTS_PER_PROVIDER.to_string()),
            ]);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }

        let response: Response = request
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

        Ok(response
            .results
            .into_iter()
            .map(|i| ImageResult {
                provider: "openverse".to_string(),
                id: i.id,
                thumbnail_url: i.thumbnail,
                description: i.title.unwrap_or_default(),
            })
            .collect())
    }

    /// Re-fetches a specific result's full-resolution image, bypassing the
    /// thumbnail URL entirely — server-side, from the provider's own "get
    /// by id" endpoint, never from a URL the client supplied. This is the
    /// whole SSRF mitigation: the client only ever echoes back a
    /// `(provider, id)` pair drawn from a search result this server itself
    /// just returned, so the worst a tampered client can do is ask for an
    /// id that doesn't exist — never redirect this server into fetching an
    /// arbitrary host.
    pub async fn fetch_by_id(&self, provider: &str, id: &str) -> Result<(Vec<u8>, String), String> {
        let image_url = match provider {
            "pixabay" => self.pixabay_full_url(id).await?,
            "openverse" => self.openverse_full_url(id).await?,
            other => return Err(format!("unknown image provider: {other}")),
        };

        let response = self
            .client
            .get(&image_url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        if !content_type.starts_with("image/") {
            return Err(format!(
                "expected an image, got content-type {content_type}"
            ));
        }
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(format!(
                "image is larger than the {MAX_IMAGE_BYTES}-byte limit"
            ));
        }
        Ok((bytes.to_vec(), content_type))
    }

    async fn pixabay_full_url(&self, id: &str) -> Result<String, String> {
        #[derive(Deserialize)]
        struct Response {
            hits: Vec<Hit>,
        }
        #[derive(Deserialize)]
        struct Hit {
            #[serde(rename = "largeImageURL")]
            large_image_url: String,
        }

        let key = self
            .pixabay_key
            .as_ref()
            .ok_or_else(|| "Pixabay isn't configured".to_string())?;

        let response: Response = self
            .client
            .get(format!("{}/api/", self.pixabay_base))
            .query(&[("key", key.as_str()), ("id", id)])
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        response
            .hits
            .into_iter()
            .next()
            .map(|h| h.large_image_url)
            .ok_or_else(|| "image not found".to_string())
    }

    async fn openverse_full_url(&self, id: &str) -> Result<String, String> {
        #[derive(Deserialize)]
        struct Item {
            url: String,
        }

        let token = self.openverse_bearer_token().await;
        let mut request = self
            .client
            .get(format!("{}/v1/images/{id}/", self.openverse_base));
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }

        let item: Item = request
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        Ok(item.url)
    }

    /// Returns a cached Openverse access token, fetching (and caching) a
    /// fresh one from the `OAuth2` client-credentials endpoint if there
    /// isn't a live one yet. Returns `None` when no credentials are
    /// configured, or if the token fetch itself fails — either way,
    /// callers just fall back to anonymous access rather than failing the
    /// whole search.
    async fn openverse_bearer_token(&self) -> Option<String> {
        let (client_id, client_secret) = self.openverse_credentials.as_ref()?;

        let cached = {
            let guard = self.openverse_token.lock().unwrap();
            guard
                .as_ref()
                .filter(|t| t.expires_at > Instant::now())
                .map(|t| t.access_token.clone())
        };
        if let Some(token) = cached {
            return Some(token);
        }

        match self.fetch_openverse_token(client_id, client_secret).await {
            Ok(token) => {
                let access_token = token.access_token.clone();
                *self.openverse_token.lock().unwrap() = Some(token);
                Some(access_token)
            }
            Err(e) => {
                log::warn!("image search: failed to obtain an Openverse access token: {e}");
                None
            }
        }
    }

    async fn fetch_openverse_token(
        &self,
        client_id: &str,
        client_secret: &str,
    ) -> Result<CachedToken, String> {
        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            expires_in: u64,
        }

        let response: TokenResponse = self
            .client
            .post(format!("{}/v1/auth_tokens/token/", self.openverse_base))
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", client_id),
                ("client_secret", client_secret),
            ])
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

        let ttl_secs = response
            .expires_in
            .saturating_sub(TOKEN_EXPIRY_SAFETY_MARGIN_SECS);
        Ok(CachedToken {
            access_token: response.access_token,
            expires_at: Instant::now() + std::time::Duration::from_secs(ttl_secs),
        })
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn openverse_credentials_from_env() -> Option<(String, String)> {
    Some((
        non_empty_env("OPENVERSE_CLIENT_ID")?,
        non_empty_env("OPENVERSE_CLIENT_SECRET")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_string_contains, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn image_search_with(
        pixabay: &MockServer,
        openverse: &MockServer,
        pixabay_key: Option<&str>,
        openverse_credentials: Option<(&str, &str)>,
    ) -> ImageSearch {
        ImageSearch::test_with(
            pixabay.uri(),
            openverse.uri(),
            pixabay_key.map(str::to_string),
            openverse_credentials.map(|(id, secret)| (id.to_string(), secret.to_string())),
        )
    }

    #[tokio::test]
    async fn search_merges_results_from_every_configured_provider() {
        let pixabay = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/"))
            .and(query_param("q", "dog"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "hits": [{"id": 2, "previewURL": "https://pixabay.example/2.jpg", "tags": "dog, pet"}]
            })))
            .mount(&pixabay)
            .await;

        let openverse = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/images/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [{"id": "abc-123", "thumbnail": "https://openverse.example/3.jpg", "title": "Dog"}]
            })))
            .mount(&openverse)
            .await;

        let search = image_search_with(&pixabay, &openverse, Some("pixabay-key"), None);
        let results = search.search("dog").await;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].provider, "pixabay");
        assert_eq!(results[1].provider, "openverse");
    }

    #[tokio::test]
    async fn search_skips_a_provider_with_no_key_configured() {
        // No mock registered for Pixabay — if ImageSearch called it without
        // a key, wiremock would reject the unexpected request.
        let pixabay = MockServer::start().await;
        let openverse = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/images/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [{"id": "abc-123", "thumbnail": "https://openverse.example/3.jpg", "title": "Dog"}]
            })))
            .mount(&openverse)
            .await;

        let search = image_search_with(&pixabay, &openverse, None, None);
        let results = search.search("dog").await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, "openverse");
    }

    #[tokio::test]
    async fn search_keeps_other_providers_results_when_one_fails() {
        let pixabay = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&pixabay)
            .await;

        let openverse = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/images/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [{"id": "abc-123", "thumbnail": "https://openverse.example/3.jpg", "title": "Dog"}]
            })))
            .mount(&openverse)
            .await;

        let search = image_search_with(&pixabay, &openverse, Some("pixabay-key"), None);
        let results = search.search("dog").await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, "openverse");
    }

    #[tokio::test]
    async fn search_openverse_authenticates_when_credentials_are_configured() {
        let pixabay = MockServer::start().await;
        let openverse = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/auth_tokens/token/"))
            .and(body_string_contains("grant_type=client_credentials"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "ov-test-token",
                "scope": "read",
                "expires_in": 36000,
                "token_type": "Bearer"
            })))
            .mount(&openverse)
            .await;
        // Only mounted for a request bearing the exact token obtained
        // above — if ImageSearch searched anonymously instead of using
        // it, this wouldn't match and the request would 404.
        Mock::given(method("GET"))
            .and(path("/v1/images/"))
            .and(header("authorization", "Bearer ov-test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [{"id": "abc-123", "thumbnail": "https://openverse.example/3.jpg", "title": "Dog"}]
            })))
            .mount(&openverse)
            .await;

        let search = image_search_with(
            &pixabay,
            &openverse,
            None,
            Some(("client-id", "client-secret")),
        );
        let results = search.search("dog").await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, "openverse");
    }

    #[tokio::test]
    async fn openverse_token_is_fetched_once_and_reused_across_searches() {
        let pixabay = MockServer::start().await;
        let openverse = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/auth_tokens/token/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "ov-test-token",
                "scope": "read",
                "expires_in": 36000,
                "token_type": "Bearer"
            })))
            .mount(&openverse)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/images/"))
            .and(header("authorization", "Bearer ov-test-token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})),
            )
            .mount(&openverse)
            .await;

        let search = image_search_with(
            &pixabay,
            &openverse,
            None,
            Some(("client-id", "client-secret")),
        );
        search.search("dog").await;
        search.search("cat").await;

        let token_requests = openverse
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.url.path() == "/v1/auth_tokens/token/")
            .count();
        assert_eq!(token_requests, 1);
    }

    #[tokio::test]
    async fn fetch_by_id_rejects_a_non_image_response() {
        let pixabay = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/"))
            .and(query_param("id", "42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "hits": [{"largeImageURL": format!("{}/not-an-image", pixabay.uri())}]
            })))
            .mount(&pixabay)
            .await;
        Mock::given(method("GET"))
            .and(path("/not-an-image"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("<html></html>")
                    .insert_header("content-type", "text/html"),
            )
            .mount(&pixabay)
            .await;

        let openverse = MockServer::start().await;
        let search = image_search_with(&pixabay, &openverse, Some("pixabay-key"), None);

        let result = search.fetch_by_id("pixabay", "42").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fetch_by_id_rejects_an_oversized_image() {
        let pixabay = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/"))
            .and(query_param("id", "42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "hits": [{"largeImageURL": format!("{}/huge.jpg", pixabay.uri())}]
            })))
            .mount(&pixabay)
            .await;
        let oversized = vec![0u8; MAX_IMAGE_BYTES + 1];
        Mock::given(method("GET"))
            .and(path("/huge.jpg"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(oversized)
                    .insert_header("content-type", "image/jpeg"),
            )
            .mount(&pixabay)
            .await;

        let openverse = MockServer::start().await;
        let search = image_search_with(&pixabay, &openverse, Some("pixabay-key"), None);

        let result = search.fetch_by_id("pixabay", "42").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fetch_by_id_returns_bytes_and_content_type_on_success() {
        let pixabay = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/"))
            .and(query_param("id", "42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "hits": [{"largeImageURL": format!("{}/real.jpg", pixabay.uri())}]
            })))
            .mount(&pixabay)
            .await;
        Mock::given(method("GET"))
            .and(path("/real.jpg"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(b"fake-jpeg-bytes".to_vec())
                    .insert_header("content-type", "image/jpeg"),
            )
            .mount(&pixabay)
            .await;

        let openverse = MockServer::start().await;
        let search = image_search_with(&pixabay, &openverse, Some("pixabay-key"), None);

        let (bytes, content_type) = search.fetch_by_id("pixabay", "42").await.unwrap();
        assert_eq!(bytes, b"fake-jpeg-bytes");
        assert_eq!(content_type, "image/jpeg");
    }

    // Deliberately NOT gated behind a "skip if unconfigured" check the way
    // storage.rs's upload_round_trips_against_configured_storage is. All
    // the other tests in this file prove the parsing/merge/degradation
    // logic is correct against responses shaped like the real ones; this
    // one is the only thing that actually proves the real Pixabay and
    // Openverse APIs are still reachable and still shaped the way this
    // code assumes. A provider that's down, rate-limited, or has changed
    // its response shape should fail this test and block a merge, not be
    // silently skipped — so a missing API key fails loudly too, rather
    // than letting the test pass by doing nothing. CI must have
    // PIXABAY_API_KEY and OPENVERSE_CLIENT_ID/OPENVERSE_CLIENT_SECRET
    // configured as real secrets for this to mean anything — anonymous
    // Openverse access is NOT good enough here: it's rate-limited by
    // source IP, and GitHub Actions' shared runner IP pool is already
    // saturated by everyone else's CI traffic, which is exactly what
    // first broke this test (see the OAuth2 client-credentials flow
    // above).
    #[tokio::test]
    async fn real_providers_all_respond_to_a_search() {
        assert!(
            non_empty_env("PIXABAY_API_KEY").is_some(),
            "PIXABAY_API_KEY must be set for this test to actually verify Pixabay connectivity"
        );
        assert!(
            openverse_credentials_from_env().is_some(),
            "OPENVERSE_CLIENT_ID and OPENVERSE_CLIENT_SECRET must both be set for this test \
             to actually verify Openverse connectivity — anonymous access is too rate-limited \
             on shared CI IPs to mean anything"
        );

        let results = ImageSearch::from_env().search("dog").await;

        for provider in ["pixabay", "openverse"] {
            assert!(
                results.iter().any(|r| r.provider == provider),
                "expected at least one real result from {provider}, got none — \
                 check API key validity, provider status, and rate limits"
            );
        }
    }
}
