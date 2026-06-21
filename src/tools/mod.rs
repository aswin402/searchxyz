use std::sync::Arc;

use rmcp::schemars::JsonSchema;
use rmcp::{
    handler::server::tool::ToolRouter, handler::server::wrapper::Parameters, tool, tool_router,
};
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::cache::Cache;
use crate::config::Config;
use crate::crawler::Crawler;
use crate::extractor::{ExtractedContent, ExtractionPipeline};
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
    #[schemars(
        description = "Crawl depth for recursive scoping. Defaults to 1 (only target URL). Max is 3."
    )]
    pub depth: Option<usize>,
    #[schemars(
        description = "Enable JavaScript rendering with a headless browser for dynamic or JS-heavy websites."
    )]
    pub render_js: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchAndReadRequest {
    #[schemars(description = "The search query string")]
    pub query: String,
    #[schemars(description = "How many top results to read (default: 3, max: 5)")]
    pub max_pages: Option<usize>,
    #[schemars(
        description = "Enable JavaScript rendering with a headless browser for dynamic or JS-heavy websites."
    )]
    pub render_js: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RecallRequest {
    #[schemars(description = "The search query for the local index")]
    pub query: String,
    #[schemars(description = "Max results (default: 5)")]
    pub max_results: Option<usize>,
    #[schemars(
        description = "Perform a semantic vector search instead of strict BM25 keyword matching (default: true)."
    )]
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
    #[schemars(
        description = "Enable JavaScript rendering with a headless browser for dynamic or JS-heavy websites."
    )]
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
    #[schemars(
        description = "Maximum number of discovered links to return (default: 100, max: 500)"
    )]
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

#[derive(Deserialize, JsonSchema)]
pub struct ReadGithubRepoRequest {
    #[schemars(
        description = "The GitHub repository URL (e.g. 'https://github.com/tokio-rs/tokio')"
    )]
    pub repo_url: String,
    #[schemars(
        description = "Optional branch name (e.g. 'master', 'main'). Defaults to the default branch."
    )]
    pub branch: Option<String>,
    #[schemars(
        description = "Optional list of file extensions to include (e.g. ['rs', 'md']). Defaults to standard code/text extensions."
    )]
    pub include_extensions: Option<Vec<String>>,
    #[schemars(
        description = "Optional list of folder/file paths to ignore. Defaults to standard ignore folders (target, node_modules, etc.)."
    )]
    pub exclude_paths: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ExportResearchRequest {
    #[schemars(
        description = "Optional query to filter exported documents. If omitted, all documents are exported."
    )]
    pub query: Option<String>,
    #[schemars(
        description = "Optional limit on how many documents to export (default 50, max 200)."
    )]
    pub limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ImportResearchRequest {
    #[schemars(description = "The serialized JSON research bundle payload.")]
    pub payload: String,
}

#[derive(serde::Serialize, serde::Deserialize, rmcp::schemars::JsonSchema)]
pub struct ResearchBundle {
    pub version: String,
    pub exported_at: String,
    pub documents: Vec<ExtractedContent>,
    pub graph: crate::graph::KnowledgeGraph,
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

    #[tool(
        description = "Search the web for a query. Returns titles, URLs, and snippets. Use for finding pages on any topic."
    )]
    async fn search_web(
        &self,
        req: Parameters<SearchWebRequest>,
    ) -> Result<String, rmcp::ErrorData> {
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

    #[tool(
        description = "Fetch a URL and extract its content as clean markdown. Strips ads, nav, scripts. Returns page title and readable text."
    )]
    async fn read_url(&self, req: Parameters<ReadUrlRequest>) -> Result<String, rmcp::ErrorData> {
        let url = &req.0.url;
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(rmcp::ErrorData::invalid_params(
                "URL must start with http:// or https://",
                None,
            ));
        }

        let depth = req.0.depth.unwrap_or(1).min(3);
        let render_js = req.0.render_js.unwrap_or(false);

        // ── Check for YouTube video URLs ──
        if crate::crawler::youtube::extract_video_id(url).is_some() {
            let transcript =
                crate::crawler::youtube::fetch_youtube_transcript(&self.crawler, url).await?;
            let title = format!("YouTube Video Transcript - {}", url);
            let extracted = ExtractedContent {
                url: url.clone(),
                title: title.clone(),
                description: String::new(),
                content_markdown: transcript.clone(),
                links: Vec::new(),
            };

            // Index the transcript
            if let Err(e) = self.index.add_document(&extracted, "youtube").await {
                tracing::warn!(url = %url, error = %e, "Failed to index YouTube transcript (non-fatal)");
            }

            // Run automatic graph heuristics
            {
                let mut graph = self.graph.lock().await;
                graph.extract_heuristics(url, &title, &transcript);
            }

            let text = format!(
                "# {}\n\n**Source:** {}\n\n---\n\n{}",
                title, url, transcript
            );
            return Ok(text);
        }

        // ── Check for GitHub repository URLs ──
        if crate::crawler::github::parse_github_url(url).is_some() {
            let summary = crate::crawler::github::clone_and_index_repo(
                &self.index,
                &self.graph,
                url,
                None,
                None,
                None,
            )
            .await?;
            return Ok(summary);
        }

        if depth > 1 {
            let spider =
                crate::crawler::spider::Spider::new(self.crawler.clone(), self.extractor.clone());
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
            let content = self.extractor.extract(
                url,
                &fetch_result.body,
                Some(&fetch_result.content_type),
            )?;

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

    #[tool(
        description = "Search the web AND read the top results. Returns full page content for each result. Best for research tasks."
    )]
    async fn search_and_read(
        &self,
        req: Parameters<SearchAndReadRequest>,
    ) -> Result<String, rmcp::ErrorData> {
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

    #[tool(
        description = "Search your local knowledge base of previously read pages. Use to find information from earlier research."
    )]
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

    #[tool(
        description = "List all documents and cached pages in the local knowledge base with metadata."
    )]
    async fn list_sources(
        &self,
        req: Parameters<ListSourcesRequest>,
    ) -> Result<String, rmcp::ErrorData> {
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

    #[tool(
        description = "Expand a query into multiple sub-queries, fetch and crawl their results in parallel, index all findings locally, and return a compiled markdown research report."
    )]
    async fn deep_research(
        &self,
        req: Parameters<DeepResearchRequest>,
    ) -> Result<String, rmcp::ErrorData> {
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
                        output.push_str(&format!(
                            "   *Successfully retrieved {} pages.*\n\n",
                            pages.len()
                        ));
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
            return Ok(format!(
                "Deep Research failed to retrieve any results for the topic `{}`.",
                query
            ));
        }

        output.push_str(&format!("*Summary: Executed {} sub-queries successfully, retrieving a total of {} unique pages.*\n\n", executed_count, all_pages.len()));

        output.push_str("---\n## Compiled Research Documents\n\n");
        for (url, page) in all_pages {
            output.push_str(&format!(
                "### {}\n- **Source URL:** {}\n\n{}\n\n",
                page.title, url, page.content_markdown
            ));
        }

        Ok(output)
    }

    #[tool(
        description = "Store text in the local knowledge base for later recall. Useful for saving important research findings."
    )]
    async fn index_content(
        &self,
        req: Parameters<IndexContentRequest>,
    ) -> Result<String, rmcp::ErrorData> {
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

    #[tool(
        description = "Map a website's structure by discovering all internal page URLs using sitemap.xml and/or fast recursive link crawling, without extracting page content."
    )]
    async fn site_map(&self, req: Parameters<SiteMapRequest>) -> Result<String, rmcp::ErrorData> {
        let url = &req.0.url;
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(rmcp::ErrorData::invalid_params(
                "URL must start with http:// or https://",
                None,
            ));
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

    #[tool(
        description = "Store a semantic connection (edge) between two entities in the knowledge graph. Helps build custom knowledge associations."
    )]
    async fn index_relationship(
        &self,
        req: Parameters<IndexRelationshipRequest>,
    ) -> Result<String, rmcp::ErrorData> {
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

    #[tool(
        description = "Query the local knowledge graph to discover entities and relationships connected to a starting concept, technology, or document."
    )]
    async fn query_graph(
        &self,
        req: Parameters<QueryGraphRequest>,
    ) -> Result<String, rmcp::ErrorData> {
        let start = &req.0.entity;
        let depth = req.0.max_depth.unwrap_or(2).min(4);

        let (nodes, edges) = {
            let graph = self.graph.lock().await;
            graph.query_neighbors(start, depth)
        };

        if nodes.is_empty() {
            return Ok(format!(
                "Entity `{}` not found in the knowledge graph.",
                start
            ));
        }

        let mut output = format!(
            "### Knowledge Graph Query for `{}` (Depth: {})\n\n",
            start, depth
        );

        output.push_str("#### Entities:\n");
        for n in &nodes {
            output.push_str(&format!("- **{}** ({})\n", n.name, n.entity_type));
        }

        output.push_str("\n#### Connections:\n");
        if edges.is_empty() {
            output.push_str("No active connections found.\n");
        } else {
            for e in &edges {
                output.push_str(&format!(
                    "- **{}** -[{}]-> **{}**\n",
                    e.source, e.relationship_type, e.target
                ));
            }
        }

        Ok(output)
    }

    #[tool(
        description = "Clone and index a GitHub repository, parsing its files and README into the local knowledge base and returning a markdown summary of the codebase."
    )]
    async fn read_github_repo(
        &self,
        req: Parameters<ReadGithubRepoRequest>,
    ) -> Result<String, rmcp::ErrorData> {
        let include_exts = req.0.include_extensions.as_deref();
        let exclude_paths = req.0.exclude_paths.as_deref();
        let summary = crate::crawler::github::clone_and_index_repo(
            &self.index,
            &self.graph,
            &req.0.repo_url,
            req.0.branch.as_deref(),
            include_exts,
            exclude_paths,
        )
        .await?;
        Ok(summary)
    }

    #[tool(
        description = "Export indexed documents and knowledge graph relationships connected to a research topic into a portable JSON bundle."
    )]
    async fn export_research(
        &self,
        req: Parameters<ExportResearchRequest>,
    ) -> Result<String, rmcp::ErrorData> {
        let limit = req.0.limit.unwrap_or(50).min(200);
        let documents = self.index.export_documents(req.0.query.as_deref(), limit)?;

        let graph = {
            let g = self.graph.lock().await;
            g.clone()
        };

        let bundle = ResearchBundle {
            version: "1.0".to_string(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            documents,
            graph,
        };

        let json = serde_json::to_string_pretty(&bundle).map_err(|e| {
            rmcp::ErrorData::internal_error(
                format!("Failed to serialize research bundle: {}", e),
                None,
            )
        })?;

        Ok(json)
    }

    #[tool(
        description = "Import a research bundle payload into the local index and knowledge graph."
    )]
    async fn import_research(
        &self,
        req: Parameters<ImportResearchRequest>,
    ) -> Result<String, rmcp::ErrorData> {
        let bundle: ResearchBundle = serde_json::from_str(&req.0.payload).map_err(|e| {
            rmcp::ErrorData::invalid_params(format!("Invalid research bundle payload: {}", e), None)
        })?;

        let doc_count = bundle.documents.len();

        for doc in &bundle.documents {
            // Index the document locally (this also generates local semantic vector embeddings!)
            if let Err(e) = self.index.add_document(doc, "imported").await {
                tracing::warn!(url = %doc.url, error = %e, "Failed to index imported document (non-fatal)");
            }
        }

        // Merge the imported nodes and edges into the local knowledge graph
        let mut graph_edges_count = 0;
        {
            let mut g = self.graph.lock().await;
            for edge in &bundle.graph.edges {
                // Find node types from bundle node map if available
                let source_type = bundle
                    .graph
                    .nodes
                    .get(&edge.source)
                    .map(|n| n.entity_type.clone())
                    .unwrap_or_else(|| "Concept".to_string());
                let target_type = bundle
                    .graph
                    .nodes
                    .get(&edge.target)
                    .map(|n| n.entity_type.clone())
                    .unwrap_or_else(|| "Concept".to_string());

                g.add_edge(
                    edge.source.clone(),
                    source_type,
                    edge.target.clone(),
                    target_type,
                    edge.relationship_type.clone(),
                );
                graph_edges_count += 1;
            }

            // Also merge any standalone nodes
            for (name, node) in &bundle.graph.nodes {
                g.add_node(name.clone(), node.entity_type.clone());
            }
        }

        // Force reload the index reader so that imported documents are immediately searchable
        if let Err(e) = self.index.reload() {
            tracing::warn!(error = %e, "Failed to reload index reader after import (non-fatal)");
        }

        Ok(format!(
            "### Import Summary\n\n\
            - **Documents Imported:** {}\n\
            - **Knowledge Graph Connections Merged:** {}\n\n\
            Research bundle successfully imported and fully indexed locally for instant search and recall.",
            doc_count, graph_edges_count
        ))
    }
}

fn expand_query(query: &str, breadth: usize) -> Vec<String> {
    let modifiers = [
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, IndexConfig};
    use crate::graph::KnowledgeGraph;
    use crate::index::SearchIndex;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn test_export_import_research() {
        let test_dir =
            std::env::temp_dir().join(format!("searchxyz_test_tools_{}", rand::random::<u64>()));
        let _ = std::fs::remove_dir_all(&test_dir);

        let index_config = IndexConfig {
            path: test_dir.clone(),
            writer_heap_bytes: 15_000_000,
        };

        let index = SearchIndex::open(&index_config).unwrap();
        let graph = Arc::new(Mutex::new(KnowledgeGraph::new()));

        // Add dummy document
        let doc = ExtractedContent {
            url: "https://example.com/sharing".to_string(),
            title: "Sharing Content".to_string(),
            description: "".to_string(),
            content_markdown: "Shared research between agents is useful.".to_string(),
            links: vec![],
        };
        index.add_document(&doc, "manual").await.unwrap();
        index.reload().unwrap();

        // Add graph connection
        {
            let mut g = graph.lock().await;
            g.add_edge(
                "AgentA".to_string(),
                "Agent".to_string(),
                "AgentB".to_string(),
                "Agent".to_string(),
                "shares_with".to_string(),
            );
        }

        // Create server
        let cache = Arc::new(Mutex::new(crate::cache::Cache::new(10, 60)));
        let server = SearchXyzServer::new(
            crate::search::SearchDispatcher::new(vec![]),
            crate::crawler::Crawler::new(
                crate::config::CrawlerConfig::default(),
                crate::config::HeadlessConfig::default(),
                crate::config::ProxyConfig::default(),
                cache.clone(),
            ),
            crate::extractor::ExtractionPipeline::new(crate::config::ExtractorConfig::default()),
            index,
            cache,
            graph.clone(),
            Config::default(),
        );

        // Test export
        let json_payload = server
            .export_research(Parameters(ExportResearchRequest {
                query: None,
                limit: None,
            }))
            .await
            .unwrap();

        // Verify json payload structure
        let bundle: ResearchBundle = serde_json::from_str(&json_payload).unwrap();
        assert_eq!(bundle.documents.len(), 1);
        assert_eq!(bundle.documents[0].title, "Sharing Content");
        assert_eq!(bundle.graph.edges.len(), 1);
        assert_eq!(bundle.graph.edges[0].relationship_type, "shares_with");

        // Clean database/graph for import test
        let clean_dir = std::env::temp_dir().join(format!(
            "searchxyz_test_tools_clean_{}",
            rand::random::<u64>()
        ));
        let _ = std::fs::remove_dir_all(&clean_dir);

        let clean_index_config = IndexConfig {
            path: clean_dir.clone(),
            writer_heap_bytes: 15_000_000,
        };
        let clean_index = SearchIndex::open(&clean_index_config).unwrap();
        let clean_graph = Arc::new(Mutex::new(KnowledgeGraph::new()));
        let clean_cache = Arc::new(Mutex::new(crate::cache::Cache::new(10, 60)));

        let clean_server = SearchXyzServer::new(
            crate::search::SearchDispatcher::new(vec![]),
            crate::crawler::Crawler::new(
                crate::config::CrawlerConfig::default(),
                crate::config::HeadlessConfig::default(),
                crate::config::ProxyConfig::default(),
                clean_cache.clone(),
            ),
            crate::extractor::ExtractionPipeline::new(crate::config::ExtractorConfig::default()),
            clean_index,
            clean_cache,
            clean_graph.clone(),
            Config::default(),
        );

        // Import payload
        let result = clean_server
            .import_research(Parameters(ImportResearchRequest {
                payload: json_payload,
            }))
            .await
            .unwrap();

        assert!(result.contains("Documents Imported:** 1"));
        assert!(result.contains("Knowledge Graph Connections Merged:** 1"));

        // Verify clean index has document
        clean_server.index.reload().unwrap();
        let list = clean_server.index.search("sharing", 5).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Sharing Content");

        // Verify clean graph has edges
        {
            let g = clean_graph.lock().await;
            assert_eq!(g.edges.len(), 1);
            assert_eq!(g.edges[0].source, "AgentA");
        }

        let _ = std::fs::remove_dir_all(&test_dir);
        let _ = std::fs::remove_dir_all(&clean_dir);
    }
}
