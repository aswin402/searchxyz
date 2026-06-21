use async_trait::async_trait;
use reqwest::Client;
use scraper::{Html, Selector};

use super::{SearchBackend, SearchQuery, SearchResult};
use crate::error::SearchXyzError;

/// DuckDuckGo Lite — scrapes the lightweight HTML interface.
/// No API key required. This is the default/fallback backend.
pub struct DuckDuckGoBackend {
    client: Client,
}

impl DuckDuckGoBackend {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl SearchBackend for DuckDuckGoBackend {
    fn name(&self) -> &str {
        "duckduckgo"
    }

    fn is_available(&self) -> bool {
        true // no key needed
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>, SearchXyzError> {
        // 1. Fetch DuckDuckGo Lite HTML.
        let resp = self
            .client
            .post("https://lite.duckduckgo.com/lite/")
            .form(&[("q", &query.query)])
            .send()
            .await
            .map_err(|e| SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: format!("HTTP request failed: {e}"),
            })?;

        if !resp.status().is_success() {
            return Err(SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: format!("DuckDuckGo returned HTTP {}", resp.status()),
            });
        }

        let html_body = resp
            .text()
            .await
            .map_err(|e| SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: format!("Failed to read response body: {e}"),
            })?;

        // 2. Parse HTML.
        let document = Html::parse_document(&html_body);

        // DuckDuckGo Lite renders results in a <table>.
        // Each result link lives in an <a class="result-link">.
        // Snippets are in the next <td class="result-snippet">.
        let link_sel = Selector::parse("a.result-link").unwrap_or_else(|_| {
            // Fallback: generic links inside result rows.
            Selector::parse("table tr td a[href]").unwrap()
        });
        let snippet_sel = Selector::parse("td.result-snippet")
            .unwrap_or_else(|_| Selector::parse("table tr.result-snippet td").unwrap());

        let links: Vec<_> = document.select(&link_sel).collect();
        let snippets: Vec<_> = document.select(&snippet_sel).collect();

        let mut results = Vec::new();

        for (i, link_el) in links.iter().enumerate() {
            if results.len() >= query.max_results {
                break;
            }

            let url = match link_el.value().attr("href") {
                Some(u) if u.starts_with("http") => u.to_string(),
                _ => continue,
            };

            let title = link_el
                .text()
                .collect::<Vec<_>>()
                .join("")
                .trim()
                .to_string();

            let snippet = snippets
                .get(i)
                .map(|el| el.text().collect::<Vec<_>>().join("").trim().to_string())
                .unwrap_or_default();

            if title.is_empty() {
                continue;
            }

            results.push(SearchResult {
                title,
                url,
                snippet,
                source: "duckduckgo".into(),
            });
        }

        if results.is_empty() {
            tracing::warn!(query = %query.query, "DuckDuckGo returned no parsable results");
        }

        Ok(results)
    }
}
