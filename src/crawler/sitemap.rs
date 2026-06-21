use crate::crawler::Crawler;
use crate::error::SearchXyzError;

/// Discover sitemap URLs on a site.
pub async fn discover_sitemap_urls(
    crawler: &Crawler,
    start_url: &str,
) -> Result<Vec<String>, SearchXyzError> {
    let parsed_url = url::Url::parse(start_url).map_err(|e| SearchXyzError::CrawlFailed {
        url: start_url.to_string(),
        reason: format!("Invalid URL: {e}"),
    })?;

    let host = parsed_url
        .host_str()
        .ok_or_else(|| SearchXyzError::CrawlFailed {
            url: start_url.to_string(),
            reason: "URL has no host".to_string(),
        })?;

    let scheme = parsed_url.scheme();

    // 1. Try default sitemap path: /sitemap.xml
    let default_sitemap = format!("{}://{}/sitemap.xml", scheme, host);
    tracing::info!(sitemap = %default_sitemap, "Attempting default sitemap fetch");

    match crawler.fetch_url(&default_sitemap, false).await {
        Ok(result) => {
            let urls = parse_sitemap_urls(&result.body);
            if !urls.is_empty() {
                return Ok(urls);
            }
        }
        Err(e) => {
            tracing::warn!(sitemap = %default_sitemap, error = %e, "Default sitemap fetch failed");
        }
    }

    // 2. Try robots.txt sitemap declarations
    let robots_url = format!("{}://{}/robots.txt", scheme, host);
    tracing::info!(robots = %robots_url, "Attempting robots.txt fetch");

    if let Ok(result) = crawler.fetch_url(&robots_url, false).await {
        let sitemap_urls = parse_sitemaps_from_robots(&result.body);
        for s_url in sitemap_urls {
            tracing::info!(sitemap = %s_url, "Attempting sitemap fetch from robots.txt");
            if let Ok(res) = crawler.fetch_url(&s_url, false).await {
                let urls = parse_sitemap_urls(&res.body);
                if !urls.is_empty() {
                    return Ok(urls);
                }
            }
        }
    }

    Err(SearchXyzError::CrawlFailed {
        url: start_url.to_string(),
        reason: "No sitemaps found or successfully parsed on host".to_string(),
    })
}

/// Parse all URL `<loc>` tags from a sitemap XML payload.
pub fn parse_sitemap_urls(xml: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut cursor = xml;
    while let Some(start_idx) = cursor.find("<loc>") {
        let sub = &cursor[start_idx + 5..];
        if let Some(end_idx) = sub.find("</loc>") {
            let loc = sub[..end_idx].trim();
            if !loc.is_empty() {
                urls.push(loc.to_string());
            }
            cursor = &sub[end_idx + 6..];
        } else {
            break;
        }
    }
    urls
}

/// Parse all `Sitemap:` lines from robots.txt payload.
pub fn parse_sitemaps_from_robots(robots_txt: &str) -> Vec<String> {
    let mut sitemaps = Vec::new();
    for line in robots_txt.lines() {
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with("sitemap:") {
            let parts: Vec<&str> = trimmed.splitn(2, ':').collect();
            if parts.len() == 2 {
                let url = parts[1].trim();
                if !url.is_empty() {
                    sitemaps.push(url.to_string());
                }
            }
        }
    }
    sitemaps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sitemap_urls() {
        let xml = r#"
            <?xml version="1.0" encoding="UTF-8"?>
            <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
                <url>
                    <loc>https://example.com/</loc>
                    <changefreq>daily</changefreq>
                </url>
                <url>
                    <loc>https://example.com/about</loc>
                </url>
            </urlset>
        "#;
        let urls = parse_sitemap_urls(xml);
        assert_eq!(
            urls,
            vec![
                "https://example.com/".to_string(),
                "https://example.com/about".to_string(),
            ]
        );
    }

    #[test]
    fn test_parse_sitemaps_from_robots() {
        let robots = r#"
            User-agent: *
            Disallow: /private/
            
            Sitemap: https://example.com/sitemap1.xml
            Sitemap: https://example.com/sitemap2.xml
        "#;
        let sitemaps = parse_sitemaps_from_robots(robots);
        assert_eq!(
            sitemaps,
            vec![
                "https://example.com/sitemap1.xml".to_string(),
                "https://example.com/sitemap2.xml".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_discover_sitemap_urls_cached() {
        use crate::cache::{Cache, CacheEntry};
        use crate::config::{CrawlerConfig, HeadlessConfig, ProxyConfig};
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let cache = Arc::new(Mutex::new(Cache::new(100, 3600)));
        {
            let mut c = cache.lock().await;
            // robots.txt pointing to sitemap
            c.put(
                "https://example.com/robots.txt".to_string(),
                CacheEntry::new(
                    r#"
                    User-agent: *
                    Sitemap: https://example.com/my-sitemap.xml
                    "#
                    .to_string(),
                    "https://example.com/robots.txt".to_string(),
                ),
            );
            // sitemap content
            c.put(
                "https://example.com/my-sitemap.xml".to_string(),
                CacheEntry::new(
                    r#"
                    <?xml version="1.0" encoding="UTF-8"?>
                    <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
                        <url><loc>https://example.com/page-a</loc></url>
                        <url><loc>https://example.com/page-b</loc></url>
                    </urlset>
                    "#
                    .to_string(),
                    "https://example.com/my-sitemap.xml".to_string(),
                ),
            );
        }

        let crawler = Crawler::new(
            CrawlerConfig::default(),
            HeadlessConfig::default(),
            ProxyConfig::default(),
            cache,
        );

        // Discover sitemap URLs (robots.txt fallback path)
        let urls = discover_sitemap_urls(&crawler, "https://example.com/")
            .await
            .unwrap();
        assert_eq!(
            urls,
            vec![
                "https://example.com/page-a".to_string(),
                "https://example.com/page-b".to_string(),
            ]
        );
    }
}
