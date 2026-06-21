pub mod fingerprint;
pub mod spider;

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

            let headers = fingerprint::HeaderGenerator::random_headers();
            let resp = self
                .client
                .get(url)
                .headers(headers)
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
                                provider: domain,
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
