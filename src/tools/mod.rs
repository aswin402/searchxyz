use std::sync::Arc;

use rmcp::{tool, tool_router, handler::server::tool::ToolRouter, handler::server::wrapper::Parameters};
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::cache::Cache;
use crate::config::Config;
use crate::crawler::Crawler;
use crate::extractor::{ExtractionPipeline, ExtractedContent};
use crate::index::SearchIndex;
use crate::pipeline::SearchAndReadPipeline;
use crate::search::{SearchDispatcher, SearchQuery};

// ── MCP tool request parameter schemas ─────────────────────────

#[derive(Deserialize, JsonSchema)]
pub struct SearchWebRequest {
    #[schemars(description = "The search query string (e.g. 'rust async patterns')")]
    pub query: String,
    #[schemars(description = "Maximum number of results to return (default: 10, max: 20)")]
    pub max_results: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ReadUrlRequest {
    #[schemars(description = "The full URL to fetch (must start with http:// or https://)")]
    pub url: String,
    #[schemars(description = "Crawl depth for recursive scoping. Defaults to 1 (only target URL). Max is 3.")]
    pub depth: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchAndReadRequest {
    #[schemars(description = "The search query string")]
    pub query: String,
    #[schemars(description = "How many top results to read (default: 3, max: 5)")]
    pub max_pages: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RecallRequest {
    #[schemars(description = "The search query for the local index")]
    pub query: String,
    #[schemars(description = "Max results (default: 5)")]
    pub max_results: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct IndexContentRequest {
    #[schemars(description = "A URL or identifier for this content")]
    pub url: String,
    #[schemars(description = "Title for the content")]
    pub title: String,
    #[schemars(description = "The text content to index")]
    pub content: String,
}

// ─────────────────────────────────────────────────────────────
// MCP Search Server
// ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SearchXyzServer {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    dispatcher: Arc<SearchDispatcher>,
    crawler: Arc<Crawler>,
    extractor: Arc<ExtractionPipeline>,
    index: Arc<SearchIndex>,
    cache: Arc<Mutex<Cache>>,
    config: Arc<Config>,
}

#[tool_router(server_handler)]
impl SearchXyzServer {
    pub fn new(
        dispatcher: SearchDispatcher,
        crawler: Crawler,
        extractor: ExtractionPipeline,
        index: SearchIndex,
        cache: Arc<Mutex<Cache>>,
        config: Config,
    ) -> Self {
        Self {
            tool_router: Self::tool_router(),
            dispatcher: Arc::new(dispatcher),
            crawler: Arc::new(crawler),
            extractor: Arc::new(extractor),
            index: Arc::new(index),
            cache,
            config: Arc::new(config),
        }
    }

    #[tool(description = "Search the web for a query. Returns titles, URLs, and snippets. Use for finding pages on any topic.")]
    async fn search_web(&self, req: Parameters<SearchWebRequest>) -> Result<String, rmcp::ErrorData> {
        let max = req.0.max_results.unwrap_or(10).min(20);
        let search_query = SearchQuery {
            query: req.0.query.clone(),
            max_results: max,
        };

        let results = self.dispatcher.search(&search_query).await?;
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

        Ok(text)
    }

    #[tool(description = "Fetch a URL and extract its content as clean markdown. Strips ads, nav, scripts. Returns page title and readable text.")]
    async fn read_url(&self, req: Parameters<ReadUrlRequest>) -> Result<String, rmcp::ErrorData> {
        let url = &req.0.url;
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(rmcp::ErrorData::invalid_params("URL must start with http:// or https://", None));
        }

        let depth = req.0.depth.unwrap_or(1).min(3);

        if depth > 1 {
            let spider = crate::crawler::spider::Spider::new(self.crawler.clone(), self.extractor.clone());
            let crawled_pages = spider.crawl(url, depth).await?;
            
            // Index successful crawled pages
            for page in &crawled_pages {
                if let Err(e) = self.index.add_document(page, "spider").await {
                    tracing::warn!(url = %page.url, error = %e, "Failed to index page from spider (non-fatal)");
                }
            }

            let text = crawled_pages
                .iter()
                .map(|p| {
                    format!(
                        "---\n## {}\n**Source:** {}\n\n{}\n\n",
                        p.title, p.url, p.content_markdown
                    )
                })
                .collect::<String>();
            Ok(text)
        } else {
            let fetch_result = self.crawler.fetch_url(url).await?;
            let content = self.extractor.extract(url, &fetch_result.body)?;
            
            // Index the single crawled page too!
            if let Err(e) = self.index.add_document(&content, "read_url").await {
                tracing::warn!(url = %content.url, error = %e, "Failed to index page from read_url (non-fatal)");
            }

            let text = format!(
                "# {}\n\n**Source:** {}\n\n---\n\n{}",
                content.title, content.url, content.content_markdown
            );
            Ok(text)
        }
    }

    #[tool(description = "Search the web AND read the top results. Returns full page content for each result. Best for research tasks.")]
    async fn search_and_read(&self, req: Parameters<SearchAndReadRequest>) -> Result<String, rmcp::ErrorData> {
        let max = req.0.max_pages.unwrap_or(3).min(5);
        let pipeline = SearchAndReadPipeline::new(
            self.dispatcher.clone(),
            self.crawler.clone(),
            self.extractor.clone(),
            self.index.clone(),
        );

        let results = pipeline.run(&req.0.query, max).await?;
        let text = results
            .iter()
            .map(|r| {
                format!(
                    "---\n## {}\n**Source:** {}\n\n{}\n\n",
                    r.title, r.url, r.content_markdown
                )
            })
            .collect::<String>();
        Ok(text)
    }

    #[tool(description = "Search your local knowledge base of previously read pages. Use to find information from earlier research.")]
    async fn recall(&self, req: Parameters<RecallRequest>) -> Result<String, rmcp::ErrorData> {
        let max = req.0.max_results.unwrap_or(5);
        let results = self.index.search(&req.0.query, max)?;
        if results.is_empty() {
            return Ok("No matching documents found in the local index. Try search_and_read to fetch new content first.".to_string());
        }
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
        Ok(text)
    }

    #[tool(description = "Store text in the local knowledge base for later recall. Useful for saving important research findings.")]
    async fn index_content(&self, req: Parameters<IndexContentRequest>) -> Result<String, rmcp::ErrorData> {
        let extracted = ExtractedContent {
            url: req.0.url.clone(),
            title: req.0.title.clone(),
            description: String::new(),
            content_markdown: req.0.content.clone(),
            links: Vec::new(),
        };

        self.index.add_document(&extracted, "manual").await?;
        Ok(format!("Successfully indexed content for `{}`", req.0.url))
    }
}
