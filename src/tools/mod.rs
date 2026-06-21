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
    #[schemars(description = "Enable JavaScript rendering with a headless browser for dynamic or JS-heavy websites.")]
    pub render_js: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchAndReadRequest {
    #[schemars(description = "The search query string")]
    pub query: String,
    #[schemars(description = "How many top results to read (default: 3, max: 5)")]
    pub max_pages: Option<usize>,
    #[schemars(description = "Enable JavaScript rendering with a headless browser for dynamic or JS-heavy websites.")]
    pub render_js: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RecallRequest {
    #[schemars(description = "The search query for the local index")]
    pub query: String,
    #[schemars(description = "Max results (default: 5)")]
    pub max_results: Option<usize>,
    #[schemars(description = "Perform a semantic vector search instead of strict BM25 keyword matching (default: true).")]
    pub semantic: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ListSourcesRequest {
    #[schemars(description = "Filter by indexing source (e.g. 'read_url', 'manual', 'spider')")]
    pub source: Option<String>,
    #[schemars(description = "Maximum number of results to return (default: 50, max: 100)")]
    pub limit: Option<usize>,
    #[schemars(description = "Offset for pagination (default: 0)")]
    pub offset: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct DeepResearchRequest {
    #[schemars(description = "The research query or topic")]
    pub query: String,
    #[schemars(description = "Number of sub-queries to expand and execute (default: 3, max: 5)")]
    pub breadth: Option<usize>,
    #[schemars(description = "How many top pages to crawl per sub-query (default: 2, max: 4)")]
    pub max_pages_per_query: Option<usize>,
    #[schemars(description = "Enable JavaScript rendering with a headless browser for dynamic or JS-heavy websites.")]
    pub render_js: Option<bool>,
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

#[derive(Deserialize, JsonSchema)]
pub struct SiteMapRequest {
    #[schemars(description = "The root URL or domain to map (e.g. 'https://example.com')")]
    pub url: String,
    #[schemars(description = "Try to locate and parse sitemap.xml (default: true)")]
    pub use_sitemap: Option<bool>,
    #[schemars(description = "Fallback to spider crawling of internal links (default: true)")]
    pub crawl_links: Option<bool>,
    #[schemars(description = "Maximum number of discovered links to return (default: 100, max: 500)")]
    pub max_links: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct IndexRelationshipRequest {
    #[schemars(description = "Source entity name (e.g. 'Tokio')")]
    pub source: String,
    #[schemars(description = "Source entity type/label (e.g. 'Library')")]
    pub source_type: String,
    #[schemars(description = "Target entity name (e.g. 'Rust')")]
    pub target: String,
    #[schemars(description = "Target entity type/label (e.g. 'Language')")]
    pub target_type: String,
    #[schemars(description = "Relationship type/verb (e.g. 'written_in', 'depends_on')")]
    pub relationship: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct QueryGraphRequest {
    #[schemars(description = "The entity name to query (e.g. 'Rust')")]
    pub entity: String,
    #[schemars(description = "Max traversal depth (default: 2, max: 4)")]
    pub max_depth: Option<usize>,
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
    graph: Arc<Mutex<crate::graph::KnowledgeGraph>>,
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
        graph: Arc<Mutex<crate::graph::KnowledgeGraph>>,
        config: Config,
    ) -> Self {
        Self {
            tool_router: Self::tool_router(),
            dispatcher: Arc::new(dispatcher),
            crawler: Arc::new(crawler),
            extractor: Arc::new(extractor),
            index: Arc::new(index),
            cache,
            graph,
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
        let render_js = req.0.render_js.unwrap_or(false);

        if depth > 1 {
            let spider = crate::crawler::spider::Spider::new(self.crawler.clone(), self.extractor.clone());
            let crawled_pages = spider.crawl(url, depth, render_js).await?;
            
            // Index successful crawled pages
            for page in &crawled_pages {
                if let Err(e) = self.index.add_document(page, "spider").await {
                    tracing::warn!(url = %page.url, error = %e, "Failed to index page from spider (non-fatal)");
                }
                // Run automatic graph heuristics
                {
                    let mut graph = self.graph.lock().await;
                    graph.extract_heuristics(&page.url, &page.title, &page.content_markdown);
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
            let fetch_result = self.crawler.fetch_url(url, render_js).await?;
            let content = self.extractor.extract(url, &fetch_result.body, Some(&fetch_result.content_type))?;

            // Index the single crawled page too!
            if let Err(e) = self.index.add_document(&content, "read_url").await {
                tracing::warn!(url = %content.url, error = %e, "Failed to index page from read_url (non-fatal)");
            }

            // Run automatic graph heuristics
            {
                let mut graph = self.graph.lock().await;
                graph.extract_heuristics(url, &content.title, &content.content_markdown);
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
        let render_js = req.0.render_js.unwrap_or(false);
        let pipeline = SearchAndReadPipeline::new(
            self.dispatcher.clone(),
            self.crawler.clone(),
            self.extractor.clone(),
            self.index.clone(),
        );

        let results = pipeline.run(&req.0.query, max, render_js).await?;
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
        let use_semantic = req.0.semantic.unwrap_or(true);
        let results = if use_semantic {
            self.index.search_semantic(&req.0.query, max)?
        } else {
            self.index.search(&req.0.query, max)?
        };
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

    #[tool(description = "List all documents and cached pages in the local knowledge base with metadata.")]
    async fn list_sources(&self, req: Parameters<ListSourcesRequest>) -> Result<String, rmcp::ErrorData> {
        let source_filter = req.0.source.as_deref();
        let limit = req.0.limit.unwrap_or(50).min(100);
        let offset = req.0.offset.unwrap_or(0);

        let (entries, total_count) = self.index.list_documents(source_filter, limit, offset)?;

        if entries.is_empty() {
            return Ok("No documents found in the local index matching your filters.".to_string());
        }

        let mut output = format!("### Cached Sources (Total indexed: {})\n\n", total_count);
        for (i, entry) in entries.iter().enumerate() {
            output.push_str(&format!(
                "{}. **{}**\n   - **URL:** {}\n   - **Indexed At:** {}\n   - **Source:** {}\n\n",
                offset + i + 1,
                entry.title,
                entry.url,
                entry.indexed_at,
                entry.source
            ));
        }

        Ok(output)
    }

    #[tool(description = "Expand a query into multiple sub-queries, fetch and crawl their results in parallel, index all findings locally, and return a compiled markdown research report.")]
    async fn deep_research(&self, req: Parameters<DeepResearchRequest>) -> Result<String, rmcp::ErrorData> {
        let query = &req.0.query;
        let breadth = req.0.breadth.unwrap_or(3).min(5);
        let max_pages = req.0.max_pages_per_query.unwrap_or(2).min(4);
        let render_js = req.0.render_js.unwrap_or(false);

        // 1. Expand query.
        let expanded_queries = expand_query(query, breadth);

        // 2. Instantiate pipeline.
        let pipeline = SearchAndReadPipeline::new(
            self.dispatcher.clone(),
            self.crawler.clone(),
            self.extractor.clone(),
            self.index.clone(),
        );

        let mut output = format!("# Deep Research Dossier: {}\n\n", query);
        output.push_str(&format!("*Executed query expansion with breadth {}, crawling up to {} top pages per query.*\n\n", breadth, max_pages));

        // We can execute all pipelines concurrently.
        use futures::future::join_all;
        let mut futures = Vec::new();
        for q in &expanded_queries {
            futures.push(pipeline.run(q, max_pages, render_js));
        }

        let results = join_all(futures).await;

        let mut all_pages = std::collections::HashMap::new();
        let mut executed_count = 0;

        for (i, res) in results.into_iter().enumerate() {
            let sub_q = &expanded_queries[i];
            match res {
                Ok(pages) => {
                    output.push_str(&format!("## Sub-Query: `{}`\n", sub_q));
                    if pages.is_empty() {
                        output.push_str("   *No new pages crawled successfully.*\n\n");
                    } else {
                        output.push_str(&format!("   *Successfully retrieved {} pages.*\n\n", pages.len()));
                        for page in pages {
                            // Avoid duplicate display by grouping/storing globally in a map.
                            all_pages.insert(page.url.clone(), page);
                        }
                        executed_count += 1;
                    }
                }
                Err(e) => {
                    output.push_str(&format!("## Sub-Query: `{}`\n", sub_q));
                    output.push_str(&format!("   *Failed to search/crawl: {}*\n\n", e));
                }
            }
        }

        if all_pages.is_empty() {
            return Ok(format!("Deep Research failed to retrieve any results for the topic `{}`.", query));
        }

        output.push_str(&format!("*Summary: Executed {} sub-queries successfully, retrieving a total of {} unique pages.*\n\n", executed_count, all_pages.len()));

        output.push_str("---\n## Compiled Research Documents\n\n");
        for (url, page) in all_pages {
            output.push_str(&format!("### {}\n- **Source URL:** {}\n\n{}\n\n", page.title, url, page.content_markdown));
        }

        Ok(output)
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

        // Run automatic graph heuristics
        {
            let mut graph = self.graph.lock().await;
            graph.extract_heuristics(&req.0.url, &req.0.title, &req.0.content);
        }

        Ok(format!("Successfully indexed content for `{}`", req.0.url))
    }

    #[tool(description = "Map a website's structure by discovering all internal page URLs using sitemap.xml and/or fast recursive link crawling, without extracting page content.")]
    async fn site_map(&self, req: Parameters<SiteMapRequest>) -> Result<String, rmcp::ErrorData> {
        let url = &req.0.url;
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(rmcp::ErrorData::invalid_params("URL must start with http:// or https://", None));
        }

        let use_sitemap = req.0.use_sitemap.unwrap_or(true);
        let crawl_links = req.0.crawl_links.unwrap_or(true);
        let max_links = req.0.max_links.unwrap_or(100).min(500);

        let mut discovered_urls = std::collections::HashSet::new();

        if use_sitemap {
            let allowed_host = url::Url::parse(url)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_string()));
            match crate::crawler::sitemap::discover_sitemap_urls(&self.crawler, url).await {
                Ok(urls) => {
                    for u in urls {
                        if let Ok(parsed_u) = url::Url::parse(&u) {
                            if let Some(ref host) = allowed_host {
                                if parsed_u.host_str() == Some(host) {
                                    discovered_urls.insert(u);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(url, error = %e, "Sitemap discovery failed");
                }
            }
        }

        if crawl_links && (discovered_urls.is_empty() || discovered_urls.len() < max_links) {
            let spider = crate::crawler::fast_spider::LinkSpider::new(self.crawler.clone());
            match spider.discover_links(url, max_links).await {
                Ok(urls) => {
                    for u in urls {
                        discovered_urls.insert(u);
                    }
                }
                Err(e) => {
                    tracing::warn!(url, error = %e, "Link spider crawling failed");
                }
            }
        }

        let mut urls: Vec<String> = discovered_urls.into_iter().collect();
        urls.sort();

        if urls.is_empty() {
            return Ok(format!("No pages could be discovered for URL: {}", url));
        }

        let mut output = format!("### Site Map for {}\n\n", url);
        output.push_str(&format!("Found {} pages:\n", urls.len()));
        for u in urls {
            output.push_str(&format!("- {}\n", u));
        }

        Ok(output)
    }

    #[tool(description = "Store a semantic connection (edge) between two entities in the knowledge graph. Helps build custom knowledge associations.")]
    async fn index_relationship(&self, req: Parameters<IndexRelationshipRequest>) -> Result<String, rmcp::ErrorData> {
        {
            let mut graph = self.graph.lock().await;
            graph.add_edge(
                req.0.source.clone(),
                req.0.source_type.clone(),
                req.0.target.clone(),
                req.0.target_type.clone(),
                req.0.relationship.clone(),
            );
        }

        Ok(format!(
            "Successfully indexed relationship: **{}** ({}) -[{}]-> **{}** ({})",
            req.0.source, req.0.source_type, req.0.relationship, req.0.target, req.0.target_type
        ))
    }

    #[tool(description = "Query the local knowledge graph to discover entities and relationships connected to a starting concept, technology, or document.")]
    async fn query_graph(&self, req: Parameters<QueryGraphRequest>) -> Result<String, rmcp::ErrorData> {
        let start = &req.0.entity;
        let depth = req.0.max_depth.unwrap_or(2).min(4);

        let (nodes, edges) = {
            let graph = self.graph.lock().await;
            graph.query_neighbors(start, depth)
        };

        if nodes.is_empty() {
            return Ok(format!("Entity `{}` not found in the knowledge graph.", start));
        }

        let mut output = format!("### Knowledge Graph Query for `{}` (Depth: {})\n\n", start, depth);

        output.push_str("#### Entities:\n");
        for n in &nodes {
            output.push_str(&format!("- **{}** ({})\n", n.name, n.entity_type));
        }

        output.push_str("\n#### Connections:\n");
        if edges.is_empty() {
            output.push_str("No active connections found.\n");
        } else {
            for e in &edges {
                output.push_str(&format!("- **{}** -[{}]-> **{}**\n", e.source, e.relationship_type, e.target));
            }
        }

        Ok(output)
    }
}

fn expand_query(query: &str, breadth: usize) -> Vec<String> {
    let modifiers = vec![
        "",
        "documentation libraries",
        "examples tutorials guide",
        "comparison review github",
        "advanced pattern best practices",
    ];

    let mut expanded = Vec::new();
    for (i, modif) in modifiers.iter().enumerate() {
        if i >= breadth {
            break;
        }
        if modif.is_empty() {
            expanded.push(query.to_string());
        } else {
            expanded.push(format!("{} {}", query, modif));
        }
    }
    expanded
}
