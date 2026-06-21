use serde::Deserialize;
use std::path::PathBuf;

use crate::error::SearchXyzError;

// ─────────────────────────────────────────────────────────────
// Top-level config. Loaded from `searchxyz.toml` with env-var
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
    pub searxng: SearXngConfig,
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
            searxng: SearXngConfig::default(),
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
            backends: vec![
                "duckduckgo".into(),
                "google".into(),
                "bing".into(),
                "brave".into(),
            ],
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
    /// 1. Environment variables (SEARCHXYZ_*)
    /// 2. TOML file (searchxyz.toml)
    /// 3. Compiled defaults
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
        if let Ok(url) = std::env::var("SEARCHXYZ_SEARXNG_URL") {
            self.searxng.instance_url = url;
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

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct SearXngConfig {
    /// Base URL of the SearXNG instance
    pub instance_url: String,
    /// List of target search engines to query (comma-separated, e.g. "google,bing")
    pub engines: Option<String>,
    /// Search request timeout in seconds
    pub timeout_secs: u64,
}

impl Default for SearXngConfig {
    fn default() -> Self {
        Self {
            instance_url: "http://localhost:8080".into(),
            engines: None,
            timeout_secs: 5,
        }
    }
}

