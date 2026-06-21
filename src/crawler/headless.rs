use crate::config::HeadlessConfig;
use crate::error::SearchXyzError;

/// Headless browser driver controlling Chromium/Chrome instance natively in Rust.
pub struct HeadlessBrowser {
    #[allow(dead_code)]
    config: HeadlessConfig,
}

impl HeadlessBrowser {
    pub fn new(config: HeadlessConfig) -> Self {
        Self { config }
    }

    #[cfg(feature = "js-rendering")]
    pub async fn fetch_html(&self, url: &str) -> Result<String, SearchXyzError> {
        use std::collections::HashMap;
        use std::time::Duration;
        use futures::StreamExt;
        use chromiumoxide::browser::{Browser, BrowserConfig};
        use chromiumoxide::cdp::browser_protocol::network::{SetExtraHttpHeadersParams, Headers};

        let mut config_builder = BrowserConfig::builder()
            .window_size(self.config.viewport_width, self.config.viewport_height);

        if let Some(ref path) = self.config.chrome_path {
            config_builder = config_builder.chrome_executable(path);
        }

        // Launch browser
        let (mut browser, mut handler) = Browser::launch(config_builder.build().map_err(|e| {
            SearchXyzError::CrawlFailed {
                url: url.to_string(),
                reason: format!("Failed to build browser config: {e}"),
            }
        })?).await.map_err(|e| {
            SearchXyzError::CrawlFailed {
                url: url.to_string(),
                reason: format!("Failed to launch headless browser: {e}"),
            }
        })?;

        // Spawn background handler
        tokio::spawn(async move {
            while let Some(h) = handler.next().await {
                if let Err(e) = h {
                    tracing::error!(error = %e, "Chrome event handler error");
                }
            }
        });

        // Open a blank page first
        let page = browser.new_page("about:blank").await.map_err(|e| {
            SearchXyzError::CrawlFailed {
                url: url.to_string(),
                reason: format!("Failed to open page in browser: {e}"),
            }
        })?;

        // Generate and set random headers
        let rand_headers = crate::crawler::fingerprint::HeaderGenerator::random_headers();
        let mut headers_map = HashMap::new();
        for (k, v) in rand_headers.iter() {
            if let Ok(value_str) = v.to_str() {
                headers_map.insert(k.as_str().to_string(), value_str.to_string());
            }
        }

        let params = SetExtraHttpHeadersParams::builder()
            .headers(Headers::new(serde_json::to_value(headers_map).unwrap()))
            .build()
            .map_err(|e| {
                SearchXyzError::CrawlFailed {
                    url: url.to_string(),
                    reason: format!("Failed to build CDP header params: {e}"),
                }
            })?;

        page.execute(params).await.map_err(|e| {
            SearchXyzError::CrawlFailed {
                url: url.to_string(),
                reason: format!("Failed to set browser headers: {e}"),
            }
        })?;

        // Navigate to URL
        page.goto(url).await.map_err(|e| {
            SearchXyzError::CrawlFailed {
                url: url.to_string(),
                reason: format!("Failed to navigate browser to URL: {e}"),
            }
        })?;

        // Wait for page navigation to complete
        page.wait_for_navigation().await.map_err(|e| {
            SearchXyzError::CrawlFailed {
                url: url.to_string(),
                reason: format!("Browser navigation wait failed: {e}"),
            }
        })?;

        // Extra sleep for SPA JavaScript execution to complete
        if self.config.wait_after_load_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.config.wait_after_load_ms)).await;
        }

        // Get fully rendered page source
        let html = page.content().await.map_err(|e| {
            SearchXyzError::CrawlFailed {
                url: url.to_string(),
                reason: format!("Failed to extract page HTML: {e}"),
            }
        })?;

        // Close page and browser
        let _ = page.close().await;
        let _ = browser.close().await;

        Ok(html)
    }

    #[cfg(not(feature = "js-rendering"))]
    pub async fn fetch_html(&self, url: &str) -> Result<String, SearchXyzError> {
        Err(SearchXyzError::ConfigError(format!(
            "JavaScript rendering is disabled or not compiled for URL: {}",
            url
        )))
    }
}
