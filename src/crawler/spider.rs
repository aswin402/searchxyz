use std::collections::HashSet;
use std::sync::Arc;
use tokio::task::JoinSet;

use crate::crawler::Crawler;
use crate::extractor::{ExtractionPipeline, ExtractedContent};
use crate::error::SearchXyzError;

pub struct Spider {
    crawler: Arc<Crawler>,
    extractor: Arc<ExtractionPipeline>,
}

impl Spider {
    pub fn new(crawler: Arc<Crawler>, extractor: Arc<ExtractionPipeline>) -> Self {
        Self { crawler, extractor }
    }

    /// Crawl a root URL recursively up to max_depth.
    /// Only crawls URLs within the same domain scope.
    pub async fn crawl(
        &self,
        start_url: &str,
        max_depth: usize,
    ) -> Result<Vec<ExtractedContent>, SearchXyzError> {
        let mut visited = HashSet::new();
        let mut to_visit = vec![start_url.to_string()];
        let mut results = Vec::new();

        let allowed_host = match url::Url::parse(start_url) {
            Ok(u) => u.host_str().map(|h| h.to_string()),
            Err(_) => None,
        };

        for depth in 0..max_depth {
            if to_visit.is_empty() {
                break;
            }

            let current_batch: Vec<String> = to_visit
                .into_iter()
                .filter(|url| visited.insert(url.clone()))
                .collect();
            if current_batch.is_empty() {
                break;
            }

            let mut join_set = JoinSet::new();

            for url in current_batch {
                let crawler = self.crawler.clone();
                let extractor = self.extractor.clone();

                join_set.spawn(async move {
                    let fetch_result = crawler.fetch_url(&url).await?;
                    let content = extractor.extract(&url, &fetch_result.body)?;
                    Ok::<ExtractedContent, SearchXyzError>(content)
                });
            }

            let mut next_batch_links = HashSet::new();

            while let Some(res) = join_set.join_next().await {
                match res {
                    Ok(Ok(content)) => {
                        // Gather links for the next depth tier if we aren't at the limit.
                        if depth + 1 < max_depth {
                            for link in &content.links {
                                if let Ok(parsed_url) = url::Url::parse(link) {
                                    if let Some(ref host) = allowed_host {
                                        if parsed_url.host_str() == Some(host) {
                                            next_batch_links.insert(link.clone());
                                        }
                                    }
                                }
                            }
                        }
                        results.push(content);
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(error = %e, "Spider batch URL failed");
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Spider batch task panicked");
                    }
                }
            }

            to_visit = next_batch_links.into_iter().collect();
        }

        if results.is_empty() {
            return Err(SearchXyzError::CrawlFailed {
                url: start_url.to_string(),
                reason: "Failed to crawl any pages within scope".into(),
            });
        }

        Ok(results)
    }
}
