use thiserror::Error;

// ─────────────────────────────────────────────────────────────
// Unified error type. Every variant carries a human-readable
// message that is directly useful to an AI agent consuming MCP
// tool responses.
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
    #[error("Rate limited by `{provider}` — retry after {retry_after_secs}s")]
    RateLimited {
        provider: String,
        retry_after_secs: u64,
    },
}

// ── Conversions ──────────────────────────────────────────────

impl From<reqwest::Error> for SearchXyzError {
    fn from(err: reqwest::Error) -> Self {
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

impl From<SearchXyzError> for rmcp::ErrorData {
    fn from(err: SearchXyzError) -> Self {
        rmcp::ErrorData::internal_error(err.to_string(), None)
    }
}
