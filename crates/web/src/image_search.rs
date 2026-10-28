#![cfg(feature = "ssr")]
// The Err(String) on each of these already is the full explanation; an
// `# Errors` doc section would just restate it.
#![allow(clippy::missing_errors_doc)]

use serde::{Deserialize, Serialize};

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

fn new_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("building the image-search HTTP client should never fail")
}

/// Combined image search across Pexels, Pixabay, and Openverse. Pexels and
/// Pixabay need their own API key (`PEXELS_API_KEY`/`PIXABAY_API_KEY`) and
/// are simply skipped when unset — Openverse needs none, so a search
/// always returns *something* with zero configuration, same spirit as
/// `Storage`/`EmailSender` degrading gracefully rather than failing
/// outright when a provider isn't set up.
#[derive(Clone)]
pub struct ImageSearch {
    client: reqwest::Client,
    pexels_key: Option<String>,
    pixabay_key: Option<String>,
    pexels_base: String,
    pixabay_base: String,
    openverse_base: String,
}

impl ImageSearch {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            client: new_http_client(),
            pexels_key: non_empty_env("PEXELS_API_KEY"),
            pixabay_key: non_empty_env("PIXABAY_API_KEY"),
            pexels_base: "https://api.pexels.com".to_string(),
            pixabay_base: "https://pixabay.com".to_string(),
            openverse_base: "https://api.openverse.org".to_string(),
        }
    }

    #[cfg(test)]
    fn test_with(
        pexels_base: String,
        pixabay_base: String,
        openverse_base: String,
        pexels_key: Option<String>,
        pixabay_key: Option<String>,
    ) -> Self {
        Self {
            client: new_http_client(),
            pexels_key,
            pixabay_key,
            pexels_base,
            pixabay_base,
            openverse_base,
        }
    }

    /// Queries every available provider concurrently and keeps going even
    /// if one provider's request fails outright — a Pixabay outage
    /// shouldn't mean zero results when Pexels and Openverse are both
    /// fine. Order is Pexels, then Pixabay, then Openverse; not
    /// interleaved, simplicity over a fancier merge nobody asked for.
    pub async fn search(&self, query: &str) -> Vec<ImageResult> {
        let (pexels, pixabay, openverse) = tokio::join!(
            self.search_pexels(query),
            self.search_pixabay(query),
            self.search_openverse(query),
        );
        let mut results = Vec::new();
        for (provider, outcome) in [
            ("pexels", pexels),
            ("pixabay", pixabay),
            ("openverse", openverse),
        ] {
            match outcome {
                Ok(mut found) => results.append(&mut found),
                Err(e) => log::warn!("image search: {provider} request failed: {e}"),
            }
        }
        results
    }

    async fn search_pexels(&self, query: &str) -> Result<Vec<ImageResult>, String> {
        #[derive(Deserialize)]
        struct Response {
            photos: Vec<Photo>,
        }
        #[derive(Deserialize)]
        struct Photo {
            id: u64,
            src: Src,
            alt: Option<String>,
        }
        #[derive(Deserialize)]
        struct Src {
            medium: String,
        }

        let Some(key) = &self.pexels_key else {
            return Ok(Vec::new());
        };

        let response: Response = self
            .client
            .get(format!("{}/v1/search", self.pexels_base))
            .header("Authorization", key)
            .query(&[
                ("query", query.to_string()),
                ("per_page", MAX_RESULTS_PER_PROVIDER.to_string()),
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
            .photos
            .into_iter()
            .map(|p| ImageResult {
                provider: "pexels".to_string(),
                id: p.id.to_string(),
                thumbnail_url: p.src.medium,
                description: p.alt.unwrap_or_default(),
            })
            .collect())
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

        let response: Response = self
            .client
            .get(format!("{}/v1/images/", self.openverse_base))
            .query(&[
                ("q", query.to_string()),
                ("page_size", MAX_RESULTS_PER_PROVIDER.to_string()),
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
            "pexels" => self.pexels_full_url(id).await?,
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

    async fn pexels_full_url(&self, id: &str) -> Result<String, String> {
        #[derive(Deserialize)]
        struct Photo {
            src: Src,
        }
        #[derive(Deserialize)]
        struct Src {
            original: String,
        }

        let key = self
            .pexels_key
            .as_ref()
            .ok_or_else(|| "Pexels isn't configured".to_string())?;

        let photo: Photo = self
            .client
            .get(format!("{}/v1/photos/{id}", self.pexels_base))
            .header("Authorization", key)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        Ok(photo.src.original)
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

        let item: Item = self
            .client
            .get(format!("{}/v1/images/{id}/", self.openverse_base))
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
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn image_search_with(
        pexels: &MockServer,
        pixabay: &MockServer,
        openverse: &MockServer,
        pexels_key: Option<&str>,
        pixabay_key: Option<&str>,
    ) -> ImageSearch {
        ImageSearch::test_with(
            pexels.uri(),
            pixabay.uri(),
            openverse.uri(),
            pexels_key.map(str::to_string),
            pixabay_key.map(str::to_string),
        )
    }

    #[tokio::test]
    async fn search_merges_results_from_every_configured_provider() {
        let pexels = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "photos": [{"id": 1, "src": {"medium": "https://pexels.example/1.jpg"}, "alt": "a dog"}]
            })))
            .mount(&pexels)
            .await;

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

        let search = image_search_with(
            &pexels,
            &pixabay,
            &openverse,
            Some("pexels-key"),
            Some("pixabay-key"),
        );
        let results = search.search("dog").await;

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].provider, "pexels");
        assert_eq!(results[1].provider, "pixabay");
        assert_eq!(results[2].provider, "openverse");
    }

    #[tokio::test]
    async fn search_skips_a_provider_with_no_key_configured() {
        let pexels = MockServer::start().await;
        // No mock registered — if ImageSearch called Pexels without a
        // key, wiremock would reject the unexpected request.
        let pixabay = MockServer::start().await;
        let openverse = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/images/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [{"id": "abc-123", "thumbnail": "https://openverse.example/3.jpg", "title": "Dog"}]
            })))
            .mount(&openverse)
            .await;

        let search = image_search_with(&pexels, &pixabay, &openverse, None, None);
        let results = search.search("dog").await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, "openverse");
    }

    #[tokio::test]
    async fn search_keeps_other_providers_results_when_one_fails() {
        let pexels = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/search"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&pexels)
            .await;

        let pixabay = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "hits": [{"id": 2, "previewURL": "https://pixabay.example/2.jpg", "tags": "dog"}]
            })))
            .mount(&pixabay)
            .await;

        let openverse = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/images/"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})),
            )
            .mount(&openverse)
            .await;

        let search = image_search_with(
            &pexels,
            &pixabay,
            &openverse,
            Some("pexels-key"),
            Some("pixabay-key"),
        );
        let results = search.search("dog").await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, "pixabay");
    }

    #[tokio::test]
    async fn fetch_by_id_rejects_a_non_image_response() {
        let pexels = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/photos/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "src": {"original": format!("{}/not-an-image", pexels.uri())}
            })))
            .mount(&pexels)
            .await;
        Mock::given(method("GET"))
            .and(path("/not-an-image"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("<html></html>")
                    .insert_header("content-type", "text/html"),
            )
            .mount(&pexels)
            .await;

        let pixabay = MockServer::start().await;
        let openverse = MockServer::start().await;
        let search = image_search_with(&pexels, &pixabay, &openverse, Some("pexels-key"), None);

        let result = search.fetch_by_id("pexels", "42").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fetch_by_id_rejects_an_oversized_image() {
        let pexels = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/photos/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "src": {"original": format!("{}/huge.jpg", pexels.uri())}
            })))
            .mount(&pexels)
            .await;
        let oversized = vec![0u8; MAX_IMAGE_BYTES + 1];
        Mock::given(method("GET"))
            .and(path("/huge.jpg"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(oversized)
                    .insert_header("content-type", "image/jpeg"),
            )
            .mount(&pexels)
            .await;

        let pixabay = MockServer::start().await;
        let openverse = MockServer::start().await;
        let search = image_search_with(&pexels, &pixabay, &openverse, Some("pexels-key"), None);

        let result = search.fetch_by_id("pexels", "42").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fetch_by_id_returns_bytes_and_content_type_on_success() {
        let pexels = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/photos/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "src": {"original": format!("{}/real.jpg", pexels.uri())}
            })))
            .mount(&pexels)
            .await;
        Mock::given(method("GET"))
            .and(path("/real.jpg"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(b"fake-jpeg-bytes".to_vec())
                    .insert_header("content-type", "image/jpeg"),
            )
            .mount(&pexels)
            .await;

        let pixabay = MockServer::start().await;
        let openverse = MockServer::start().await;
        let search = image_search_with(&pexels, &pixabay, &openverse, Some("pexels-key"), None);

        let (bytes, content_type) = search.fetch_by_id("pexels", "42").await.unwrap();
        assert_eq!(bytes, b"fake-jpeg-bytes");
        assert_eq!(content_type, "image/jpeg");
    }
}
