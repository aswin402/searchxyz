use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::{SearchBackend, SearchQuery, SearchResult};
use crate::config::BraveConfig;
use crate::error::SearchXyzError;

/// Brave Web Search API backend.
/// Requires an API key from https://brave.com/search/api/.
pub struct BraveBackend {
    client: Client,
    config: BraveConfig,
}

impl BraveBackend {
    pub fn new(client: Client, config: BraveConfig) -> Self {
        Self { client, config }
    }
}

// ── API response DTOs ────────────────────────────────────────

#[derive(Deserialize)]
struct BraveApiResponse {
    web: Option<BraveWebResults>,
}

#[derive(Deserialize)]
struct BraveWebResults {
    results: Vec<BraveWebResult>,
}

#[derive(Deserialize)]
struct BraveWebResult {
    title: String,
    url: String,
    description: Option<String>,
}

// ── Trait implementation ─────────────────────────────────────

#[async_trait]
impl SearchBackend for BraveBackend {
    fn name(&self) -> &str {
        "brave"
    }

    fn is_available(&self) -> bool {
        self.config.api_key.is_some()
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>, SearchXyzError> {
        let api_key =
            self.config
                .api_key
                .as_deref()
                .ok_or_else(|| SearchXyzError::SearchFailed {
                    query: query.query.clone(),
                    reason: "Brave API key not configured. Set SEARCHXYZ_BRAVE_API_KEY.".into(),
                })?;

        // Brave API allows up to 3 retries with exponential backoff on 429.
        let mut attempt = 0u32;
        let max_retries = 3u32;

        loop {
            let resp = self
                .client
                .get("https://api.search.brave.com/res/v1/web/search")
                .header("Accept", "application/json")
                .header("Accept-Encoding", "gzip")
                .header("X-Subscription-Token", api_key)
                .query(&[
                    ("q", query.query.as_str()),
                    ("count", &query.max_results.min(20).to_string()),
                ])
                .send()
                .await
                .map_err(|e| SearchXyzError::SearchFailed {
                    query: query.query.clone(),
                    reason: format!("Brave API request failed: {e}"),
                })?;

            match resp.status().as_u16() {
                200 => {
                    // Success — parse and return.
                    let body: BraveApiResponse =
                        resp.json()
                            .await
                            .map_err(|e| SearchXyzError::SearchFailed {
                                query: query.query.clone(),
                                reason: format!("Failed to parse Brave response: {e}"),
                            })?;

                    let results = body
                        .web
                        .map(|w| w.results)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|r| SearchResult {
                            title: r.title,
                            url: r.url,
                            snippet: r.description.unwrap_or_default(),
                            source: "brave".into(),
                        })
                        .collect();

                    return Ok(results);
                }

                401 => {
                    return Err(SearchXyzError::SearchFailed {
                        query: query.query.clone(),
                        reason: "Brave API key is invalid or expired (HTTP 401). \
                                 Verify SEARCHXYZ_BRAVE_API_KEY."
                            .into(),
                    });
                }

                429 => {
                    // Rate limited — exponential backoff.
                    attempt += 1;
                    if attempt > max_retries {
                        return Err(SearchXyzError::RateLimited {
                            provider: "brave".into(),
                            retry_after_secs: 60,
                        });
                    }
                    let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt - 1));
                    tracing::warn!(
                        attempt,
                        delay_ms = delay.as_millis(),
                        "Brave API rate limited, backing off"
                    );
                    tokio::time::sleep(delay).await;
                    continue;
                }

                status @ (500..=599) => {
                    return Err(SearchXyzError::SearchFailed {
                        query: query.query.clone(),
                        reason: format!("Brave API server error (HTTP {status}). Try again later."),
                    });
                }

                other => {
                    return Err(SearchXyzError::SearchFailed {
                        query: query.query.clone(),
                        reason: format!("Unexpected Brave API status: {other}"),
                    });
                }
            }
        }
    }
}
