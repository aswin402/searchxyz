pub mod brave;
pub mod duckduckgo;
pub mod searxng;
pub mod google;
pub mod bing;

use async_trait::async_trait;
use crate::error::SearchXyzError;

// ─────────────────────────────────────────────────────────────
// Core data types
// ─────────────────────────────────────────────────────────────

/// Incoming search request from an MCP tool call.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub query: String,
    pub max_results: usize,
}

/// A single search result returned by any backend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source: String, // "duckduckgo", "brave", etc.
}

// ─────────────────────────────────────────────────────────────
// Backend trait — each search provider implements this.
// ─────────────────────────────────────────────────────────────

#[async_trait]
pub trait SearchBackend: Send + Sync {
    /// Human-readable backend name (for logs and error messages).
    fn name(&self) -> &str;

    /// Pre-flight check: is this backend configured and reachable?
    /// E.g. Brave returns false when no API key is set.
    fn is_available(&self) -> bool;

    /// Execute a search query and return results.
    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>, SearchXyzError>;
}

// ─────────────────────────────────────────────────────────────
// Dispatcher — tries backends in order until one succeeds.
// ─────────────────────────────────────────────────────────────

pub struct SearchDispatcher {
    backends: Vec<Box<dyn SearchBackend>>,
}

impl SearchDispatcher {
    pub fn new(backends: Vec<Box<dyn SearchBackend>>) -> Self {
        Self { backends }
    }

    /// Run the query against backends in configured order.
    /// Returns the first successful result set.
    pub async fn search(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<SearchResult>, SearchXyzError> {
        let mut tried: Vec<String> = Vec::new();

        for backend in &self.backends {
            // Skip unavailable backends (e.g. missing API key).
            if !backend.is_available() {
                tracing::debug!(
                    backend = backend.name(),
                    "Skipping unavailable backend"
                );
                continue;
            }

            tracing::info!(
                backend = backend.name(),
                query = %query.query,
                "Trying search backend"
            );

            match backend.search(query).await {
                Ok(results) if !results.is_empty() => {
                    tracing::info!(
                        backend = backend.name(),
                        count = results.len(),
                        "Search succeeded"
                    );
                    return Ok(results);
                }
                Ok(_empty) => {
                    tracing::warn!(
                        backend = backend.name(),
                        "Backend returned zero results"
                    );
                    tried.push(format!("{} (0 results)", backend.name()));
                }
                Err(err) => {
                    tracing::error!(
                        backend = backend.name(),
                        error = %err,
                        "Backend failed"
                    );
                    tried.push(format!("{} ({})", backend.name(), err));
                }
            }
        }

        Err(SearchXyzError::AllBackendsExhausted {
            query: query.query.clone(),
            backends_tried: tried.join(", "),
        })
    }
}
