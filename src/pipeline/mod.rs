use std::sync::Arc;

use tokio::task::JoinSet;

use crate::crawler::Crawler;
use crate::error::SearchXyzError;
use crate::extractor::{ExtractedContent, ExtractionPipeline};
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
    /// 1. Search for `query`
    /// 2. Take the top `max_pages` result URLs
    /// 3. Crawl them in parallel
    /// 4. Extract content from each
    /// 5. Index all successful extractions
    /// 6. Return results (partial failures are tolerated)
    pub async fn run(
        &self,
        query: &str,
        max_pages: usize,
        render_js: bool,
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
                let fetch_result = crawler.fetch_url(&url, render_js).await?;

                // Extract content.
                let content = extractor.extract(
                    &url,
                    &fetch_result.body,
                    Some(&fetch_result.content_type),
                )?;

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
