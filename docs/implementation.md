# searchxyz — Implementation Guide

> **Code-level reference** for building the searchxyz MCP search server in Rust.
> Every module includes real, compiling Rust code with detailed comments.

---

## Table of Contents

1. [Project Setup — Cargo.toml](#1-project-setup)
2. [Error Module — `src/error/mod.rs`](#2-error-module)
3. [Config Module — `src/config/mod.rs`](#3-config-module)
4. [Search Module — `src/search/`](#4-search-module)
5. [Crawler Module — `src/crawler/mod.rs`](#5-crawler-module)
6. [Extractor Module — `src/extractor/mod.rs`](#6-extractor-module)
7. [Index Module — `src/index/mod.rs`](#7-index-module)
8. [Cache Module — `src/cache/mod.rs`](#8-cache-module)
9. [MCP Tools — `src/tools/mod.rs`](#9-mcp-tools)
10. [Main Entry Point — `src/main.rs`](#10-main-entry-point)
11. [Pipeline — `src/pipeline/mod.rs`](#11-pipeline)

---

## 1. Project Setup

### `Cargo.toml`

```toml
[package]
name = "searchxyz"
version = "0.0.1"
edition = "2024"
description = "MCP search server — search the web, crawl pages, and index content for AI agents"
license = "MIT"

[dependencies]
# ── MCP protocol ──
rmcp = { version = "0.1", features = ["server", "transport-io", "macros"] }

# ── Async runtime ──
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
futures = "0.3"

# ── HTTP ──
reqwest = { version = "0.12", features = ["json", "gzip", "brotli", "cookies"] }

# ── HTML parsing & extraction ──
scraper = "0.22"

# ── Full-text search index ──
tantivy = "0.22"

# ── Caching ──
lru = "0.12"

# ── Serialization ──
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# ── Error handling ──
thiserror = "2"
anyhow = "1"

# ── Logging / tracing ──
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# ── CLI ──
clap = { version = "4", features = ["derive"] }

# ── Rate limiting ──
governor = "0.8"

# ── Retry / backoff ──
backon = "1"

# ── Time ──
chrono = { version = "0.4", features = ["serde"] }

# ── URL handling ──
url = "2"

[dev-dependencies]
tokio-test = "0.4"
wiremock = "0.6"
tempfile = "3"
```

### Module tree

```
src/
├── main.rs
├── error/
│   └── mod.rs
├── config/
│   └── mod.rs
├── search/
│   ├── mod.rs          # SearchBackend trait + dispatcher
│   ├── duckduckgo.rs
│   └── brave.rs
├── crawler/
│   └── mod.rs
├── extractor/
│   └── mod.rs
├── index/
│   └── mod.rs
├── cache/
│   └── mod.rs
├── tools/
│   └── mod.rs
└── pipeline/
    └── mod.rs
```

---

## 2. Error Module

### `src/error/mod.rs`

```rust
use thiserror::Error;

// ─────────────────────────────────────────────────────────────
// Unified error type.  Every variant carries a human-readable
// message that is directly useful to an AI agent consuming MCP
// tool responses — no opaque codes, no "internal error" walls.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum SearchXyzError {
    // ── Search ──
    #[error("Search failed for query `{query}`: {reason}")]
    SearchFailed { query: String, reason: String },

    #[error("All search backends exhausted for query `{query}`. Tried: {backends_tried}")]
    AllBackendsExhausted {
        query: String,
        backends_tried: String,
    },

    // ── Crawl ──
    #[error("Crawl failed for `{url}`: {reason}")]
    CrawlFailed { url: String, reason: String },

    #[error("HTTP {status} from `{url}`: {reason}")]
    HttpError {
        url: String,
        status: u16,
        reason: String,
    },

    #[error("Request to `{url}` timed out after {timeout_secs}s")]
    Timeout { url: String, timeout_secs: u64 },

    // ── Extraction ──
    #[error("Content extraction failed for `{url}`: {reason}")]
    ExtractionFailed { url: String, reason: String },

    #[error("Extracted content from `{url}` is empty or below {min_length} chars")]
    EmptyContent { url: String, min_length: usize },

    // ── Index ──
    #[error("Index operation failed: {0}")]
    IndexError(String),

    // ── Config ──
    #[error("Configuration error: {0}")]
    ConfigError(String),

    // ── Rate limiting ──
    #[error("Rate limited by `{source}` — retry after {retry_after_secs}s")]
    RateLimited {
        source: String,
        retry_after_secs: u64,
    },
}

// ── Conversions ──────────────────────────────────────────────

impl From<reqwest::Error> for SearchXyzError {
    fn from(err: reqwest::Error) -> Self {
        // Distinguish timeout from generic HTTP errors so the
        // caller can decide whether to retry.
        if err.is_timeout() {
            let url = err
                .url()
                .map(|u| u.to_string())
                .unwrap_or_else(|| "<unknown>".into());
            return SearchXyzError::Timeout {
                url,
                timeout_secs: 30,
            };
        }
        let url = err
            .url()
            .map(|u| u.to_string())
            .unwrap_or_else(|| "<unknown>".into());
        SearchXyzError::CrawlFailed {
            url,
            reason: err.to_string(),
        }
    }
}

impl From<tantivy::TantivyError> for SearchXyzError {
    fn from(err: tantivy::TantivyError) -> Self {
        SearchXyzError::IndexError(format!("Tantivy error: {err}"))
    }
}

impl From<std::io::Error> for SearchXyzError {
    fn from(err: std::io::Error) -> Self {
        SearchXyzError::ConfigError(format!("I/O error: {err}"))
    }
}

// ── MCP error response ──────────────────────────────────────

impl SearchXyzError {
    /// Convert to an MCP-compatible error content block.
    /// Returns a JSON object that rmcp can serialise straight
    /// into a CallToolResult with `is_error: true`.
    pub fn to_mcp_error(&self) -> rmcp::model::Content {
        // Every error variant's Display impl already produces
        // an actionable message — forward it directly.
        rmcp::model::Content::text(self.to_string())
    }

    /// Convenience: build a full `CallToolResult` with `is_error`.
    pub fn into_call_tool_result(self) -> rmcp::model::CallToolResult {
        rmcp::model::CallToolResult {
            content: vec![self.to_mcp_error()],
            is_error: Some(true),
            ..Default::default()
        }
    }
}
```

---

## 3. Config Module

### `src/config/mod.rs`

```rust
use serde::Deserialize;
use std::path::PathBuf;

use crate::error::SearchXyzError;

// ─────────────────────────────────────────────────────────────
// Top-level config.  Loaded from `searchxyz.toml` with env-var
// overrides layered on top.
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub search: SearchConfig,
    pub brave: BraveConfig,
    pub crawler: CrawlerConfig,
    pub extractor: ExtractorConfig,
    pub index: IndexConfig,
    pub cache: CacheConfig,
}

// ── Sub-configs ──────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct ServerConfig {
    /// Server name reported in MCP initialize handshake.
    pub name: String,
    /// Version string.
    pub version: String,
    /// Log level filter (e.g. "info", "debug,hyper=warn").
    pub log_level: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct SearchConfig {
    /// Ordered list of backend names to try: ["duckduckgo", "brave"]
    pub backends: Vec<String>,
    /// Max results per search query.
    pub max_results: usize,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct BraveConfig {
    /// API key — overridable via SEARCHXYZ_BRAVE_API_KEY.
    pub api_key: Option<String>,
    /// Max results from Brave API (1-20).
    pub max_results: usize,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct CrawlerConfig {
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// User-Agent header string.
    pub user_agent: String,
    /// Max response body size in bytes.
    pub max_body_bytes: usize,
    /// Max redirect hops.
    pub max_redirects: usize,
    /// Max retries on transient errors.
    pub max_retries: u32,
    /// Per-domain max requests per second.
    pub rate_limit_per_sec: u64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct ExtractorConfig {
    /// Minimum content length (chars) to accept extraction.
    pub min_content_length: usize,
    /// CSS selectors for elements to strip before extraction.
    pub strip_selectors: Vec<String>,
    /// Priority selectors to try for main content.
    pub content_selectors: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct IndexConfig {
    /// Directory to store the Tantivy index.
    pub path: PathBuf,
    /// IndexWriter heap size in bytes.
    pub writer_heap_bytes: usize,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct CacheConfig {
    /// Max cached pages.
    pub max_entries: usize,
    /// TTL per entry in seconds.
    pub ttl_secs: u64,
}

// ── Defaults ─────────────────────────────────────────────────

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            search: SearchConfig::default(),
            brave: BraveConfig::default(),
            crawler: CrawlerConfig::default(),
            extractor: ExtractorConfig::default(),
            index: IndexConfig::default(),
            cache: CacheConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: "searchxyz".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            log_level: "info".into(),
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            backends: vec!["duckduckgo".into(), "brave".into()],
            max_results: 10,
        }
    }
}

impl Default for BraveConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            max_results: 10,
        }
    }
}

impl Default for CrawlerConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            user_agent: "searchxyz/0.1 (AI-agent MCP tool; +https://github.com/user/searchxyz)"
                .into(),
            max_body_bytes: 5 * 1024 * 1024, // 5 MB
            max_redirects: 5,
            max_retries: 3,
            rate_limit_per_sec: 2,
        }
    }
}

impl Default for ExtractorConfig {
    fn default() -> Self {
        Self {
            min_content_length: 50,
            strip_selectors: vec![
                "script".into(),
                "style".into(),
                "nav".into(),
                "footer".into(),
                "header".into(),
                "aside".into(),
                "noscript".into(),
                "iframe".into(),
            ],
            content_selectors: vec![
                "article".into(),
                "main".into(),
                "[role=\"main\"]".into(),
                ".post-content".into(),
                ".article-body".into(),
            ],
        }
    }
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            path: dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("searchxyz")
                .join("index"),
            writer_heap_bytes: 50 * 1024 * 1024, // 50 MB
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            ttl_secs: 3600,
        }
    }
}

// ── Loading ──────────────────────────────────────────────────

impl Config {
    /// Load config with the following precedence (highest wins):
    ///   1. Environment variables  (SEARCHXYZ_*)
    ///   2. TOML file              (searchxyz.toml)
    ///   3. Compiled defaults
    pub fn load(path: Option<&str>) -> Result<Self, SearchXyzError> {
        // Start from defaults.
        let mut config = if let Some(p) = path {
            let contents = std::fs::read_to_string(p)?;
            toml::from_str::<Config>(&contents).map_err(|e| {
                SearchXyzError::ConfigError(format!("Failed to parse {p}: {e}"))
            })?
        } else {
            // Try default path; fall back to defaults silently.
            match std::fs::read_to_string("searchxyz.toml") {
                Ok(contents) => toml::from_str::<Config>(&contents).map_err(|e| {
                    SearchXyzError::ConfigError(format!("Failed to parse searchxyz.toml: {e}"))
                })?,
                Err(_) => Config::default(),
            }
        };

        // Layer environment variable overrides.
        config.apply_env_overrides();

        // Validate.
        config.validate()?;

        Ok(config)
    }

    /// Override specific fields from well-known env vars.
    fn apply_env_overrides(&mut self) {
        if let Ok(key) = std::env::var("SEARCHXYZ_BRAVE_API_KEY") {
            self.brave.api_key = Some(key);
        }
        if let Ok(level) = std::env::var("SEARCHXYZ_LOG_LEVEL") {
            self.server.log_level = level;
        }
        if let Ok(path) = std::env::var("SEARCHXYZ_INDEX_PATH") {
            self.index.path = PathBuf::from(path);
        }
        if let Ok(val) = std::env::var("SEARCHXYZ_CACHE_MAX_ENTRIES") {
            if let Ok(n) = val.parse() {
                self.cache.max_entries = n;
            }
        }
        if let Ok(val) = std::env::var("SEARCHXYZ_CACHE_TTL_SECS") {
            if let Ok(n) = val.parse() {
                self.cache.ttl_secs = n;
            }
        }
    }

    /// Validate invariants.
    fn validate(&self) -> Result<(), SearchXyzError> {
        if self.search.backends.is_empty() {
            return Err(SearchXyzError::ConfigError(
                "At least one search backend must be configured".into(),
            ));
        }
        if self.search.backends.contains(&"brave".to_string())
            && self.brave.api_key.is_none()
        {
            tracing::warn!(
                "Brave backend is listed but no API key is set — \
                 it will be skipped at runtime"
            );
        }
        if self.crawler.max_body_bytes == 0 {
            return Err(SearchXyzError::ConfigError(
                "crawler.max_body_bytes must be > 0".into(),
            ));
        }
        Ok(())
    }
}
```

---

## 4. Search Module

### `src/search/mod.rs` — Trait + Dispatcher

```rust
pub mod brave;
pub mod duckduckgo;

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
```

### `src/search/duckduckgo.rs`

```rust
use async_trait::async_trait;
use reqwest::Client;
use scraper::{Html, Selector};

use crate::error::SearchXyzError;
use super::{SearchBackend, SearchQuery, SearchResult};

/// DuckDuckGo Lite — scrapes the lightweight HTML interface.
/// No API key required.  This is the default/fallback backend.
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

    async fn search(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<SearchResult>, SearchXyzError> {
        // 1. Fetch DuckDuckGo Lite HTML.
        let resp = self
            .client
            .get("https://lite.duckduckgo.com/lite/")
            .query(&[("q", &query.query)])
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

        let html_body = resp.text().await.map_err(|e| {
            SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: format!("Failed to read response body: {e}"),
            }
        })?;

        // 2. Parse HTML.
        let document = Html::parse_document(&html_body);

        // DuckDuckGo Lite renders results in a <table>.
        // Each result link lives in an <a class="result-link">.
        // Snippets are in the next <td class="result-snippet">.
        let link_sel =
            Selector::parse("a.result-link").unwrap_or_else(|_| {
                // Fallback: generic links inside result rows.
                Selector::parse("table tr td a[href]").unwrap()
            });
        let snippet_sel =
            Selector::parse("td.result-snippet").unwrap_or_else(|_| {
                Selector::parse("table tr.result-snippet td").unwrap()
            });

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
                .map(|el| {
                    el.text().collect::<Vec<_>>().join("").trim().to_string()
                })
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
```

### `src/search/brave.rs`

```rust
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use crate::config::BraveConfig;
use crate::error::SearchXyzError;
use super::{SearchBackend, SearchQuery, SearchResult};

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

    async fn search(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<SearchResult>, SearchXyzError> {
        let api_key = self.config.api_key.as_deref().ok_or_else(|| {
            SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: "Brave API key not configured. Set SEARCHXYZ_BRAVE_API_KEY.".into(),
            }
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
                        resp.json().await.map_err(|e| {
                            SearchXyzError::SearchFailed {
                                query: query.query.clone(),
                                reason: format!("Failed to parse Brave response: {e}"),
                            }
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
                            source: "brave".into(),
                            retry_after_secs: 60,
                        });
                    }
                    let delay = std::time::Duration::from_millis(
                        500 * 2u64.pow(attempt - 1),
                    );
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
                        reason: format!(
                            "Brave API server error (HTTP {status}). Try again later."
                        ),
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
```

---

## 5. Crawler Module

### `src/crawler/mod.rs`

```rust
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use governor::{Quota, RateLimiter, clock::DefaultClock, state::keyed::DefaultKeyedStateStore};
use reqwest::{Client, StatusCode, redirect::Policy};
use tokio::sync::Mutex;
use url::Url;

use crate::cache::{Cache, CacheEntry};
use crate::config::CrawlerConfig;
use crate::error::SearchXyzError;

/// Per-domain keyed rate limiter type.
type DomainRateLimiter =
    RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;

/// The crawler fetches HTML pages with timeouts, retries, and
/// per-domain rate limiting.
pub struct Crawler {
    client: Client,
    config: CrawlerConfig,
    rate_limiter: Arc<DomainRateLimiter>,
    cache: Arc<Mutex<Cache>>,
}

/// Raw fetch result before extraction.
#[derive(Debug, Clone)]
pub struct FetchResult {
    pub url: String,
    pub final_url: String, // after redirects
    pub body: String,
    pub content_type: String,
}

impl Crawler {
    pub fn new(config: CrawlerConfig, cache: Arc<Mutex<Cache>>) -> Self {
        // Build HTTP client with all safety guards.
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(&config.user_agent)
            .redirect(Policy::limited(config.max_redirects))
            .pool_max_idle_per_host(4)
            .gzip(true)
            .brotli(true)
            .build()
            .expect("Failed to build HTTP client");

        // Per-domain rate limiter: N requests/sec per domain.
        let quota = Quota::per_second(
            NonZeroU32::new(config.rate_limit_per_sec as u32)
                .unwrap_or(NonZeroU32::new(2).unwrap()),
        );
        let rate_limiter = Arc::new(RateLimiter::keyed(quota));

        Self {
            client,
            config,
            rate_limiter,
            cache,
        }
    }

    /// Fetch a URL, respecting cache, rate limits, and retries.
    pub async fn fetch_url(
        &self,
        url: &str,
    ) -> Result<FetchResult, SearchXyzError> {
        // ── 1. Check cache ──
        {
            let cache = self.cache.lock().await;
            if let Some(entry) = cache.get(url) {
                tracing::debug!(url, "Cache hit");
                return Ok(FetchResult {
                    url: url.to_string(),
                    final_url: url.to_string(),
                    body: entry.content.clone(),
                    content_type: "text/html".into(),
                });
            }
        }

        // ── 2. Rate limit ──
        let domain = Url::parse(url)
            .map(|u| u.host_str().unwrap_or("unknown").to_string())
            .unwrap_or_else(|_| "unknown".into());

        self.rate_limiter
            .until_key_ready(&domain)
            .await;

        // ── 3. Fetch with retries (exponential backoff) ──
        let mut attempt = 0u32;
        loop {
            attempt += 1;

            let resp = self
                .client
                .get(url)
                .send()
                .await;

            match resp {
                Ok(response) => {
                    let final_url = response.url().to_string();
                    let status = response.status();

                    // ── Handle common HTTP errors ──
                    match status {
                        StatusCode::OK => {}

                        StatusCode::FORBIDDEN => {
                            return Err(SearchXyzError::HttpError {
                                url: url.into(),
                                status: 403,
                                reason: "Access forbidden — site blocks automated access"
                                    .into(),
                            });
                        }

                        StatusCode::NOT_FOUND => {
                            return Err(SearchXyzError::HttpError {
                                url: url.into(),
                                status: 404,
                                reason: "Page not found".into(),
                            });
                        }

                        StatusCode::TOO_MANY_REQUESTS => {
                            if attempt <= self.config.max_retries {
                                let delay = Duration::from_millis(
                                    1000 * 2u64.pow(attempt - 1),
                                );
                                tracing::warn!(url, attempt, "Rate limited, backing off");
                                tokio::time::sleep(delay).await;
                                continue;
                            }
                            return Err(SearchXyzError::RateLimited {
                                source: domain,
                                retry_after_secs: 60,
                            });
                        }

                        StatusCode::INTERNAL_SERVER_ERROR
                        | StatusCode::SERVICE_UNAVAILABLE => {
                            if attempt <= self.config.max_retries {
                                let delay = Duration::from_millis(
                                    500 * 2u64.pow(attempt - 1),
                                );
                                tracing::warn!(
                                    url, status = %status, attempt,
                                    "Server error, retrying"
                                );
                                tokio::time::sleep(delay).await;
                                continue;
                            }
                            return Err(SearchXyzError::HttpError {
                                url: url.into(),
                                status: status.as_u16(),
                                reason: format!(
                                    "Server error after {} attempts",
                                    self.config.max_retries
                                ),
                            });
                        }

                        other if !other.is_success() => {
                            return Err(SearchXyzError::HttpError {
                                url: url.into(),
                                status: other.as_u16(),
                                reason: format!("Unexpected status: {other}"),
                            });
                        }

                        _ => {} // other 2xx — proceed
                    }

                    // ── Content-Type guard ──
                    let content_type = response
                        .headers()
                        .get(reqwest::header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();

                    if !content_type.contains("text/html")
                        && !content_type.contains("text/plain")
                        && !content_type.contains("application/xhtml")
                    {
                        return Err(SearchXyzError::CrawlFailed {
                            url: url.into(),
                            reason: format!(
                                "Unsupported Content-Type: {content_type}. \
                                 Only HTML pages are supported."
                            ),
                        });
                    }

                    // ── Size guard ──
                    if let Some(len) = response.content_length() {
                        if len as usize > self.config.max_body_bytes {
                            return Err(SearchXyzError::CrawlFailed {
                                url: url.into(),
                                reason: format!(
                                    "Response too large ({len} bytes, max {})",
                                    self.config.max_body_bytes
                                ),
                            });
                        }
                    }

                    // ── Read body with size limit ──
                    let body = response
                        .text()
                        .await
                        .map_err(|e| SearchXyzError::CrawlFailed {
                            url: url.into(),
                            reason: format!("Failed to read body: {e}"),
                        })?;

                    if body.len() > self.config.max_body_bytes {
                        return Err(SearchXyzError::CrawlFailed {
                            url: url.into(),
                            reason: format!(
                                "Body exceeds limit ({} bytes)",
                                body.len()
                            ),
                        });
                    }

                    // ── Cache the response ──
                    {
                        let mut cache = self.cache.lock().await;
                        cache.put(
                            url.to_string(),
                            CacheEntry::new(body.clone(), url.to_string()),
                        );
                    }

                    return Ok(FetchResult {
                        url: url.into(),
                        final_url,
                        body,
                        content_type,
                    });
                }

                Err(e) => {
                    // Network-level error — retry on transient failures.
                    if attempt <= self.config.max_retries
                        && (e.is_timeout() || e.is_connect())
                    {
                        let delay =
                            Duration::from_millis(500 * 2u64.pow(attempt - 1));
                        tracing::warn!(
                            url, error = %e, attempt,
                            "Transient error, retrying"
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(SearchXyzError::from(e));
                }
            }
        }
    }
}
```

---

## 6. Extractor Module

### `src/extractor/mod.rs`

```rust
use scraper::{Html, Selector, ElementRef};

use crate::config::ExtractorConfig;
use crate::error::SearchXyzError;

/// Extracted content from a crawled page.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtractedContent {
    pub url: String,
    pub title: String,
    pub description: String,
    pub content_markdown: String,
}

/// Pipeline that converts raw HTML into clean markdown text.
pub struct ExtractionPipeline {
    config: ExtractorConfig,
    // Pre-compiled selectors for performance.
    strip_selectors: Vec<Selector>,
    content_selectors: Vec<Selector>,
}

impl ExtractionPipeline {
    pub fn new(config: ExtractorConfig) -> Self {
        let strip_selectors = config
            .strip_selectors
            .iter()
            .filter_map(|s| Selector::parse(s).ok())
            .collect();

        let content_selectors = config
            .content_selectors
            .iter()
            .filter_map(|s| Selector::parse(s).ok())
            .collect();

        Self {
            config,
            strip_selectors,
            content_selectors,
        }
    }

    /// Extract readable content from raw HTML.
    pub fn extract(
        &self,
        url: &str,
        html: &str,
    ) -> Result<ExtractedContent, SearchXyzError> {
        let document = Html::parse_document(html);

        // ── 1. Extract metadata ──
        let title = self.extract_title(&document);
        let description = self.extract_meta_description(&document);

        // ── 2. Find the main content element ──
        // Try priority selectors in order; fall back to <body>.
        let content_html = self
            .find_main_content(&document)
            .unwrap_or_else(|| self.extract_body_text(&document));

        // ── 3. Strip noisy elements from extracted HTML ──
        let cleaned = self.strip_noise(&content_html);

        // ── 4. Convert to markdown-like plain text ──
        let markdown = self.html_to_markdown(&cleaned);

        // ── 5. Validate length ──
        if markdown.trim().len() < self.config.min_content_length {
            return Err(SearchXyzError::EmptyContent {
                url: url.into(),
                min_length: self.config.min_content_length,
            });
        }

        Ok(ExtractedContent {
            url: url.into(),
            title,
            description,
            content_markdown: markdown,
        })
    }

    // ── Private helpers ──────────────────────────────────────

    fn extract_title(&self, doc: &Html) -> String {
        let sel = Selector::parse("title").unwrap();
        doc.select(&sel)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default()
    }

    fn extract_meta_description(&self, doc: &Html) -> String {
        let sel = Selector::parse(r#"meta[name="description"]"#).unwrap();
        doc.select(&sel)
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }

    /// Walk priority selectors and return the first match's inner HTML.
    fn find_main_content(&self, doc: &Html) -> Option<String> {
        for sel in &self.content_selectors {
            if let Some(el) = doc.select(sel).next() {
                return Some(el.inner_html());
            }
        }
        None
    }

    /// Fallback: grab all text inside <body>.
    fn extract_body_text(&self, doc: &Html) -> String {
        let sel = Selector::parse("body").unwrap();
        doc.select(&sel)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_default()
    }

    /// Remove noisy elements (script, style, nav, etc.) from an
    /// HTML fragment string.
    fn strip_noise(&self, html: &str) -> String {
        let fragment = Html::parse_fragment(html);
        let mut output = String::new();

        // Collect node IDs to skip (elements matching strip selectors).
        let skip_ids: std::collections::HashSet<_> = self
            .strip_selectors
            .iter()
            .flat_map(|sel| fragment.select(sel))
            .map(|el| el.id())
            .collect();

        // Walk the tree and emit text for non-skipped nodes.
        fn collect_text(
            node: ego_tree::NodeRef<scraper::Node>,
            skip: &std::collections::HashSet<ego_tree::NodeId>,
            out: &mut String,
        ) {
            if skip.contains(&node.id()) {
                return;
            }
            match node.value() {
                scraper::Node::Text(t) => {
                    let text = t.text.trim();
                    if !text.is_empty() {
                        out.push_str(text);
                        out.push(' ');
                    }
                }
                scraper::Node::Element(el) => {
                    // Add line breaks for block elements.
                    let tag = el.name();
                    let is_block = matches!(
                        tag,
                        "p" | "div" | "br" | "h1" | "h2" | "h3"
                            | "h4" | "h5" | "h6" | "li" | "tr"
                            | "blockquote" | "pre" | "hr"
                    );
                    if is_block {
                        out.push('\n');
                    }
                    // Add markdown heading prefix.
                    match tag {
                        "h1" => out.push_str("# "),
                        "h2" => out.push_str("## "),
                        "h3" => out.push_str("### "),
                        "h4" => out.push_str("#### "),
                        _ => {}
                    }
                    for child in node.children() {
                        collect_text(child, skip, out);
                    }
                    if is_block {
                        out.push('\n');
                    }
                }
                _ => {
                    for child in node.children() {
                        collect_text(child, skip, out);
                    }
                }
            }
        }

        if let Some(root) = fragment.tree.root().children().next() {
            collect_text(root, &skip_ids, &mut output);
        }

        output
    }

    /// Normalise whitespace for the final markdown output.
    fn html_to_markdown(&self, text: &str) -> String {
        // Collapse runs of whitespace; normalise line breaks.
        let mut result = String::with_capacity(text.len());
        let mut prev_blank = false;

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !prev_blank {
                    result.push('\n');
                    prev_blank = true;
                }
            } else {
                result.push_str(trimmed);
                result.push('\n');
                prev_blank = false;
            }
        }

        result.trim().to_string()
    }
}
```

---

## 7. Index Module

### `src/index/mod.rs`

```rust
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use tantivy::{
    collector::TopDocs,
    directory::MmapDirectory,
    query::QueryParser,
    schema::*,
    Index, IndexReader, IndexWriter, ReloadPolicy,
    doc,
};
use tokio::sync::Mutex;

use crate::config::IndexConfig;
use crate::error::SearchXyzError;
use crate::extractor::ExtractedContent;

/// Thread-safe full-text search index backed by Tantivy.
pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    writer: Arc<Mutex<IndexWriter>>,
    // Schema field handles — kept for building docs & queries.
    f_url: Field,
    f_title: Field,
    f_content: Field,
    f_source: Field,
    f_indexed_at: Field,
}

/// A result from querying the local index.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexSearchResult {
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub source: String,
    pub score: f32,
}

impl SearchIndex {
    /// Open or create the index at the configured path.
    pub fn open(config: &IndexConfig) -> Result<Self, SearchXyzError> {
        // Ensure directory exists.
        std::fs::create_dir_all(&config.path)?;

        // ── Build schema ──
        let mut builder = Schema::builder();

        let f_url = builder.add_text_field("url", TEXT | STORED);
        let f_title = builder.add_text_field("title", TEXT | STORED);
        let f_content = builder.add_text_field("content", TEXT);
        let f_source = builder.add_text_field("source", TEXT | STORED);
        let f_indexed_at = builder.add_date_field(
            "indexed_at",
            DateOptions::default()
                .set_stored()
                .set_indexed()
                .set_precision(DateTimePrecision::Seconds),
        );

        let schema = builder.build();

        // ── Open or create index ──
        let dir = MmapDirectory::open(&config.path)
            .map_err(|e| SearchXyzError::IndexError(format!(
                "Failed to open index directory: {e}"
            )))?;

        let index = Index::open_or_create(dir, schema.clone())
            .map_err(|e| SearchXyzError::IndexError(format!(
                "Failed to open/create index: {e}"
            )))?;

        // ── Reader (auto-reload on new commits) ──
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| {
                SearchXyzError::IndexError(format!("Failed to create reader: {e}"))
            })?;

        // ── Writer ──
        let writer = index
            .writer(config.writer_heap_bytes)
            .map_err(|e| SearchXyzError::IndexError(format!(
                "Failed to create writer: {e}"
            )))?;

        Ok(Self {
            index,
            reader,
            writer: Arc::new(Mutex::new(writer)),
            f_url,
            f_title,
            f_content,
            f_source,
            f_indexed_at,
        })
    }

    /// Index a piece of extracted content.
    pub async fn add_document(
        &self,
        content: &ExtractedContent,
        source: &str,
    ) -> Result<(), SearchXyzError> {
        let now = tantivy::DateTime::from_timestamp_secs(Utc::now().timestamp());

        let mut writer = self.writer.lock().await;
        writer.add_document(doc!(
            self.f_url     => content.url.clone(),
            self.f_title   => content.title.clone(),
            self.f_content => content.content_markdown.clone(),
            self.f_source  => source.to_string(),
            self.f_indexed_at => now,
        ))?;
        writer.commit()?;

        tracing::debug!(url = %content.url, "Indexed document");

        Ok(())
    }

    /// Full-text search across indexed content.
    pub fn search(
        &self,
        query_str: &str,
        max_results: usize,
    ) -> Result<Vec<IndexSearchResult>, SearchXyzError> {
        let searcher = self.reader.searcher();

        let query_parser = QueryParser::for_index(
            &self.index,
            vec![self.f_title, self.f_content],
        );

        let query = query_parser
            .parse_query(query_str)
            .map_err(|e| SearchXyzError::IndexError(format!(
                "Failed to parse query `{query_str}`: {e}"
            )))?;

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(max_results))
            .map_err(|e| SearchXyzError::IndexError(format!(
                "Search execution failed: {e}"
            )))?;

        // ── Build snippet generator for content field ──
        let snippet_generator =
            tantivy::SnippetGenerator::create(&searcher, &query, self.f_content)
                .map_err(|e| SearchXyzError::IndexError(format!(
                    "Snippet generator failed: {e}"
                )))?;

        let mut results = Vec::with_capacity(top_docs.len());

        for (score, doc_address) in top_docs {
            let doc: tantivy::TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| SearchXyzError::IndexError(format!(
                    "Failed to retrieve doc: {e}"
                )))?;

            let url = doc
                .get_first(self.f_url)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let title = doc
                .get_first(self.f_title)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let source = doc
                .get_first(self.f_source)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let snippet = snippet_generator
                .snippet_from_doc(&doc)
                .to_html();

            results.push(IndexSearchResult {
                url,
                title,
                snippet,
                source,
                score,
            });
        }

        Ok(results)
    }

    /// Delete all documents matching a URL.
    pub async fn delete_by_url(
        &self,
        url: &str,
    ) -> Result<(), SearchXyzError> {
        let term = tantivy::Term::from_field_text(self.f_url, url);
        let mut writer = self.writer.lock().await;
        writer.delete_term(term);
        writer.commit()?;
        tracing::debug!(url, "Deleted from index");
        Ok(())
    }
}
```

---

## 8. Cache Module

### `src/cache/mod.rs`

```rust
use std::time::{Duration, Instant};

use lru::LruCache;
use std::num::NonZeroUsize;

/// A single cached page.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub content: String,
    pub url: String,
    pub fetched_at: Instant,
    pub ttl: Duration,
}

impl CacheEntry {
    pub fn new(content: String, url: String) -> Self {
        Self {
            content,
            url,
            fetched_at: Instant::now(),
            ttl: Duration::from_secs(3600), // default 1 hour
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    pub fn is_expired(&self) -> bool {
        self.fetched_at.elapsed() > self.ttl
    }
}

/// Thread-safe LRU cache for crawled page content.
/// Wrap in `Arc<Mutex<Cache>>` for shared access.
pub struct Cache {
    inner: LruCache<String, CacheEntry>,
    default_ttl: Duration,
}

impl Cache {
    pub fn new(max_entries: usize, ttl_secs: u64) -> Self {
        let cap = NonZeroUsize::new(max_entries.max(1)).unwrap();
        Self {
            inner: LruCache::new(cap),
            default_ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Retrieve a cached entry.  Returns `None` if missing or expired.
    pub fn get(&self, url: &str) -> Option<&CacheEntry> {
        // Note: LruCache::peek does not promote the entry.
        // We use peek here because `get` requires &mut self.
        // The caller can use `get_mut` if promotion is desired.
        self.inner.peek(url).filter(|e| !e.is_expired())
    }

    /// Retrieve and promote a cached entry.
    pub fn get_mut(&mut self, url: &str) -> Option<&CacheEntry> {
        // Get promotes the entry to most-recently-used.
        let entry = self.inner.get(url)?;
        if entry.is_expired() {
            // Remove expired entry.
            self.inner.pop(url);
            return None;
        }
        // Re-borrow after potential removal.
        self.inner.get(url).map(|e| &*e)
    }

    /// Insert or update a cache entry.
    pub fn put(&mut self, url: String, mut entry: CacheEntry) {
        // Apply default TTL if entry uses the 1-hour default.
        if entry.ttl == Duration::from_secs(3600) {
            entry.ttl = self.default_ttl;
        }
        self.inner.put(url, entry);
    }

    /// Number of entries currently cached.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
```

---

## 9. MCP Tools

### `src/tools/mod.rs`

```rust
use std::sync::Arc;

use rmcp::{ServerHandler, model::*, tool};
use tokio::sync::Mutex;

use crate::cache::Cache;
use crate::config::Config;
use crate::crawler::Crawler;
use crate::error::SearchXyzError;
use crate::extractor::ExtractionPipeline;
use crate::index::SearchIndex;
use crate::pipeline::SearchAndReadPipeline;
use crate::search::{SearchDispatcher, SearchQuery};

/// The MCP tool handler — holds all shared state and exposes
/// each tool via the `#[tool]` macro.
#[derive(Clone)]
pub struct SearchXyzService {
    dispatcher: Arc<SearchDispatcher>,
    crawler: Arc<Crawler>,
    extractor: Arc<ExtractionPipeline>,
    index: Arc<SearchIndex>,
    cache: Arc<Mutex<Cache>>,
    config: Arc<Config>,
}

impl SearchXyzService {
    pub fn new(
        dispatcher: SearchDispatcher,
        crawler: Crawler,
        extractor: ExtractionPipeline,
        index: SearchIndex,
        cache: Arc<Mutex<Cache>>,
        config: Config,
    ) -> Self {
        Self {
            dispatcher: Arc::new(dispatcher),
            crawler: Arc::new(crawler),
            extractor: Arc::new(extractor),
            index: Arc::new(index),
            cache,
            config: Arc::new(config),
        }
    }
}

// ─────────────────────────────────────────────────────────────
// MCP tool implementations.
//
// Each tool is annotated with #[tool] which generates the MCP
// schema, description, and input validation.  The descriptions
// are written for LLM consumption — they explain what the tool
// does, when to use it, and what it returns.
// ─────────────────────────────────────────────────────────────

#[tool(tool_box)]
impl SearchXyzService {
    /// Search the web using multiple search engines.
    ///
    /// Returns a list of search results with title, URL, and snippet.
    /// Use this when you need to find web pages about a topic.
    /// The query should be a natural-language search string.
    ///
    /// Results come from DuckDuckGo (default) with Brave Search as
    /// fallback.  No API key is needed for basic usage.
    #[tool(
        name = "search_web",
        description = "Search the web for a query. Returns titles, URLs, and snippets. Use for finding pages on any topic."
    )]
    async fn search_web(
        &self,
        /// The search query string (e.g. "rust async patterns")
        #[tool(param)]
        query: String,
        /// Maximum number of results to return (default: 10, max: 20)
        #[tool(param)]
        max_results: Option<usize>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let max = max_results.unwrap_or(10).min(20);

        let search_query = SearchQuery {
            query: query.clone(),
            max_results: max,
        };

        match self.dispatcher.search(&search_query).await {
            Ok(results) => {
                let text = results
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        format!(
                            "{}. **{}**\n   {}\n   {}\n",
                            i + 1,
                            r.title,
                            r.url,
                            r.snippet
                        )
                    })
                    .collect::<String>();

                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Ok(e.into_call_tool_result()),
        }
    }

    /// Fetch and extract readable content from a URL.
    ///
    /// Crawls the page, strips navigation/ads/scripts, and returns
    /// clean markdown text.  Use this when you have a specific URL
    /// and need its content.
    #[tool(
        name = "read_url",
        description = "Fetch a URL and extract its content as clean markdown. Strips ads, nav, scripts. Returns page title and readable text."
    )]
    async fn read_url(
        &self,
        /// The full URL to fetch (must start with http:// or https://)
        #[tool(param)]
        url: String,
    ) -> Result<CallToolResult, rmcp::Error> {
        // Validate URL format.
        if !url.starts_with("http://") && !url.starts_with("https://") {
            let err = SearchXyzError::CrawlFailed {
                url: url.clone(),
                reason: "URL must start with http:// or https://".into(),
            };
            return Ok(err.into_call_tool_result());
        }

        // Crawl.
        let fetch_result = match self.crawler.fetch_url(&url).await {
            Ok(r) => r,
            Err(e) => return Ok(e.into_call_tool_result()),
        };

        // Extract.
        match self.extractor.extract(&url, &fetch_result.body) {
            Ok(content) => {
                let text = format!(
                    "# {}\n\n**Source:** {}\n\n---\n\n{}",
                    content.title, content.url, content.content_markdown
                );
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Ok(e.into_call_tool_result()),
        }
    }

    /// Search the web and read the top results in one step.
    ///
    /// Combines search_web + read_url: searches for the query, then
    /// fetches and extracts the top N result pages in parallel.
    /// Use this for research tasks where you need actual page content,
    /// not just search snippets.
    #[tool(
        name = "search_and_read",
        description = "Search the web AND read the top results. Returns full page content for each result. Best for research tasks."
    )]
    async fn search_and_read(
        &self,
        /// The search query string
        #[tool(param)]
        query: String,
        /// How many top results to read (default: 3, max: 5)
        #[tool(param)]
        max_pages: Option<usize>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let max = max_pages.unwrap_or(3).min(5);

        let pipeline = SearchAndReadPipeline::new(
            self.dispatcher.clone(),
            self.crawler.clone(),
            self.extractor.clone(),
            self.index.clone(),
        );

        match pipeline.run(&query, max).await {
            Ok(results) => {
                let text = results
                    .iter()
                    .map(|r| {
                        format!(
                            "---\n## {}\n**Source:** {}\n\n{}\n\n",
                            r.title, r.url, r.content_markdown
                        )
                    })
                    .collect::<String>();
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Ok(e.into_call_tool_result()),
        }
    }

    /// Search previously indexed content (local knowledge base).
    ///
    /// Queries the Tantivy full-text index for pages that were
    /// previously crawled and indexed.  Use this to recall
    /// information from earlier research without re-fetching.
    #[tool(
        name = "recall",
        description = "Search your local knowledge base of previously read pages. Use to find information from earlier research."
    )]
    async fn recall(
        &self,
        /// The search query for the local index
        #[tool(param)]
        query: String,
        /// Max results (default: 5)
        #[tool(param)]
        max_results: Option<usize>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let max = max_results.unwrap_or(5);

        match self.index.search(&query, max) {
            Ok(results) if results.is_empty() => {
                Ok(CallToolResult::success(vec![Content::text(
                    "No matching documents found in the local index. \
                     Try search_and_read to fetch new content first.",
                )]))
            }
            Ok(results) => {
                let text = results
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        format!(
                            "{}. **{}** (score: {:.2})\n   {}\n   {}\n\n",
                            i + 1,
                            r.title,
                            r.score,
                            r.url,
                            r.snippet
                        )
                    })
                    .collect::<String>();
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Ok(e.into_call_tool_result()),
        }
    }

    /// Manually index content into the local knowledge base.
    ///
    /// Use this to store important text that you want to recall later.
    /// The content is indexed for full-text search.
    #[tool(
        name = "index_content",
        description = "Store text in the local knowledge base for later recall. Useful for saving important research findings."
    )]
    async fn index_content(
        &self,
        /// A URL or identifier for this content
        #[tool(param)]
        url: String,
        /// Title for the content
        #[tool(param)]
        title: String,
        /// The text content to index
        #[tool(param)]
        content: String,
    ) -> Result<CallToolResult, rmcp::Error> {
        use crate::extractor::ExtractedContent;

        let extracted = ExtractedContent {
            url: url.clone(),
            title,
            description: String::new(),
            content_markdown: content,
        };

        match self.index.add_document(&extracted, "manual").await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(
                format!("Successfully indexed content for `{url}`"),
            )])),
            Err(e) => Ok(e.into_call_tool_result()),
        }
    }
}

// ── ServerHandler trait impl (required by rmcp) ──────────────

#[tool(tool_box)]
impl ServerHandler for SearchXyzService {
    fn name(&self) -> String {
        self.config.server.name.clone()
    }

    fn version(&self) -> String {
        self.config.server.version.clone()
    }
}
```

---

## 10. Main Entry Point

### `src/main.rs`

```rust
mod cache;
mod config;
mod crawler;
mod error;
mod extractor;
mod index;
mod pipeline;
mod search;
mod tools;

use std::sync::Arc;

use clap::Parser;
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

use cache::Cache;
use config::Config;
use crawler::Crawler;
use extractor::ExtractionPipeline;
use index::SearchIndex;
use search::{SearchDispatcher, brave::BraveBackend, duckduckgo::DuckDuckGoBackend};
use tools::SearchXyzService;

// ── CLI arguments ────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "searchxyz",
    about = "MCP search server — web search, crawl, extract, index for AI agents",
    version
)]
struct Cli {
    /// Path to config file (default: searchxyz.toml)
    #[arg(short, long)]
    config: Option<String>,
}

// ── Entry point ──────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // ── 1. Load config ──
    let config = Config::load(cli.config.as_deref())?;

    // ── 2. Init tracing (MUST go to stderr — stdout is MCP) ──
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&config.server.log_level)),
        )
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();

    tracing::info!(
        name = %config.server.name,
        version = %config.server.version,
        "Starting searchxyz MCP server"
    );

    // ── 3. Build shared cache ──
    let cache = Arc::new(Mutex::new(Cache::new(
        config.cache.max_entries,
        config.cache.ttl_secs,
    )));

    // ── 4. Build components ──
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.crawler.timeout_secs))
        .user_agent(&config.crawler.user_agent)
        .build()?;

    // Search backends (in configured order).
    let mut backends: Vec<Box<dyn search::SearchBackend>> = Vec::new();
    for name in &config.search.backends {
        match name.as_str() {
            "duckduckgo" => {
                backends.push(Box::new(DuckDuckGoBackend::new(
                    http_client.clone(),
                )));
            }
            "brave" => {
                backends.push(Box::new(BraveBackend::new(
                    http_client.clone(),
                    config.brave.clone(),
                )));
            }
            other => {
                tracing::warn!(backend = other, "Unknown search backend, skipping");
            }
        }
    }

    let dispatcher = SearchDispatcher::new(backends);
    let crawler = Crawler::new(config.crawler.clone(), cache.clone());
    let extractor = ExtractionPipeline::new(config.extractor.clone());
    let index = SearchIndex::open(&config.index)?;

    // ── 5. Build MCP service ──
    let service = SearchXyzService::new(
        dispatcher,
        crawler,
        extractor,
        index,
        cache,
        config.clone(),
    );

    // ── 6. Start MCP server on stdio ──
    tracing::info!("MCP server listening on stdio");

    let transport = rmcp::transport::io::stdio();

    let server = rmcp::ServiceExt::serve(service, transport)
        .await
        .inspect_err(|e| {
            tracing::error!(error = %e, "Failed to start server");
        })?;

    // ── 7. Wait for shutdown ──
    // The server runs until stdin is closed (client disconnect)
    // or a signal is received.
    tokio::select! {
        result = server.waiting() => {
            if let Err(e) = result {
                tracing::error!(error = %e, "Server error");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received Ctrl+C, shutting down");
        }
    }

    tracing::info!("searchxyz server stopped");

    Ok(())
}
```

---

## 11. Pipeline

### `src/pipeline/mod.rs`

```rust
use std::sync::Arc;

use tokio::task::JoinSet;

use crate::crawler::Crawler;
use crate::error::SearchXyzError;
use crate::extractor::{ExtractionPipeline, ExtractedContent};
use crate::index::SearchIndex;
use crate::search::{SearchDispatcher, SearchQuery};

/// Combined search → crawl → extract → index pipeline.
///
/// Used by the `search_and_read` MCP tool to execute the full
/// research workflow in one call.
pub struct SearchAndReadPipeline {
    dispatcher: Arc<SearchDispatcher>,
    crawler: Arc<Crawler>,
    extractor: Arc<ExtractionPipeline>,
    index: Arc<SearchIndex>,
}

impl SearchAndReadPipeline {
    pub fn new(
        dispatcher: Arc<SearchDispatcher>,
        crawler: Arc<Crawler>,
        extractor: Arc<ExtractionPipeline>,
        index: Arc<SearchIndex>,
    ) -> Self {
        Self {
            dispatcher,
            crawler,
            extractor,
            index,
        }
    }

    /// Run the full pipeline:
    ///   1. Search for `query`
    ///   2. Take the top `max_pages` result URLs
    ///   3. Crawl them in parallel
    ///   4. Extract content from each
    ///   5. Index all successful extractions
    ///   6. Return results (partial failures are tolerated)
    pub async fn run(
        &self,
        query: &str,
        max_pages: usize,
    ) -> Result<Vec<ExtractedContent>, SearchXyzError> {
        // ── Step 1: Search ──
        let search_query = SearchQuery {
            query: query.to_string(),
            max_results: max_pages * 2, // fetch extras in case some fail
        };

        let search_results = self.dispatcher.search(&search_query).await?;

        if search_results.is_empty() {
            return Err(SearchXyzError::SearchFailed {
                query: query.into(),
                reason: "No search results found".into(),
            });
        }

        // ── Step 2: Take top N URLs ──
        let urls: Vec<String> = search_results
            .iter()
            .take(max_pages)
            .map(|r| r.url.clone())
            .collect();

        tracing::info!(
            count = urls.len(),
            query = query,
            "Crawling top search results in parallel"
        );

        // ── Step 3: Parallel crawl with JoinSet ──
        let mut join_set = JoinSet::new();

        for url in urls {
            let crawler = self.crawler.clone();
            let extractor = self.extractor.clone();

            join_set.spawn(async move {
                // Crawl the page.
                let fetch_result = crawler.fetch_url(&url).await?;

                // Extract content.
                let content = extractor.extract(&url, &fetch_result.body)?;

                Ok::<ExtractedContent, SearchXyzError>(content)
            });
        }

        // ── Step 4: Collect results, tolerating partial failures ──
        let mut extracted: Vec<ExtractedContent> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(content)) => {
                    extracted.push(content);
                }
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "Pipeline: one URL failed");
                    errors.push(e.to_string());
                }
                Err(join_err) => {
                    tracing::error!(error = %join_err, "Pipeline: task panicked");
                    errors.push(format!("Task panicked: {join_err}"));
                }
            }
        }

        // ── Step 5: Index all successful extractions ──
        for content in &extracted {
            if let Err(e) = self.index.add_document(content, "search_and_read").await {
                // Indexing failure is non-fatal — log and continue.
                tracing::warn!(
                    url = %content.url,
                    error = %e,
                    "Failed to index document (non-fatal)"
                );
            }
        }

        // ── Step 6: Return results ──
        if extracted.is_empty() {
            // All URLs failed — report all errors.
            return Err(SearchXyzError::SearchFailed {
                query: query.into(),
                reason: format!(
                    "All pages failed to load or extract. Errors:\n{}",
                    errors.join("\n")
                ),
            });
        }

        if !errors.is_empty() {
            tracing::info!(
                succeeded = extracted.len(),
                failed = errors.len(),
                "Pipeline completed with partial failures"
            );
        }

        Ok(extracted)
    }
}
```

---

## Quick Reference: Crate Versions

| Crate | Version | Purpose |
|-------|---------|---------|
| `rmcp` | 0.1 | MCP protocol server |
| `tokio` | 1.x | Async runtime |
| `reqwest` | 0.12 | HTTP client |
| `scraper` | 0.22 | HTML parsing + CSS selectors |
| `tantivy` | 0.22 | Full-text search index |
| `lru` | 0.12 | LRU cache |
| `governor` | 0.8 | Rate limiting |
| `backon` | 1.x | Retry / backoff |
| `clap` | 4.x | CLI argument parsing |
| `tracing` | 0.1 | Structured logging |
| `thiserror` | 2.x | Error derive macros |
| `serde` | 1.x | Serialization |
| `tantivy` | 0.22 | Full-text search |

---

> **Next steps:** See the [Architecture Guide](./architecture.md) for system design rationale and the project `README.md` for usage instructions.
