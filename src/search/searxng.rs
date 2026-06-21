use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

use crate::config::SearXngConfig;
use crate::error::SearchXyzError;
use super::{SearchBackend, SearchQuery, SearchResult};

/// SearXNG metasearch API backend.
pub struct SearXngBackend {
    client: Client,
    config: SearXngConfig,
}

impl SearXngBackend {
    pub fn new(client: Client, config: SearXngConfig) -> Self {
        Self { client, config }
    }
}

// ── SearXNG API Response Deserializer ──────────────────────────

#[derive(Deserialize)]
struct SearXngApiResponse {
    results: Vec<SearXngWebResult>,
}

#[derive(Deserialize)]
struct SearXngWebResult {
    title: String,
    url: String,
    content: Option<String>,
}

// ── SearchBackend Implementation ───────────────────────────────

#[async_trait]
impl SearchBackend for SearXngBackend {
    fn name(&self) -> &str {
        "searxng"
    }

    fn is_available(&self) -> bool {
        !self.config.instance_url.is_empty()
    }

    async fn search(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<SearchResult>, SearchXyzError> {
        let base_url = self.config.instance_url.trim_end_matches('/');
        
        let mut request = self.client
            .get(format!("{}/search", base_url))
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .query(&[
                ("q", query.query.as_str()),
                ("format", "json"),
                ("safesearch", "0"),
            ]);

        if let Some(ref engines) = self.config.engines {
            request = request.query(&[("engines", engines)]);
        }

        let response = request.send().await.map_err(|e| {
            SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: format!("Failed to reach SearXNG instance: {e}"),
            }
        })?;

        if !response.status().is_success() {
            return Err(SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: format!("SearXNG returned HTTP Status {}", response.status()),
            });
        }

        let body: SearXngApiResponse = response.json().await.map_err(|e| {
            SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: format!("Failed to parse SearXNG JSON response: {e}"),
            }
        })?;

        let results = body.results
            .into_iter()
            .take(query.max_results)
            .map(|r| SearchResult {
                title: r.title,
                url: r.url,
                snippet: r.content.unwrap_or_default(),
                source: "searxng".into(),
            })
            .collect();

        Ok(results)
    }
}
