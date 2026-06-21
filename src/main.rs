#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(clippy::borrow_deref_ref)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::double_ended_iterator_last)]
#![allow(clippy::useless_vec)]

mod cache;
mod config;
mod crawler;
mod error;
mod extractor;
mod graph;
mod index;
mod pipeline;
mod search;
mod tools;

use std::sync::Arc;

use clap::Parser;
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

use cache::Cache;
use config::Config;
use crawler::Crawler;
use extractor::ExtractionPipeline;
use index::SearchIndex;
use search::{
    bing::BingBackend, brave::BraveBackend, duckduckgo::DuckDuckGoBackend, google::GoogleBackend,
    searxng::SearXngBackend, SearchDispatcher,
};
use tools::SearchXyzServer;

use rmcp::{transport::stdio, ServiceExt};

// ── CLI arguments ────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "searchxyz",
    about = "MCP search server — web search, crawl, extract, index for AI agents",
    version
)]
struct Cli {
    /// Path to config file (default: searchxyz.toml)
    #[arg(short, long)]
    config: Option<String>,

    /// Run as a remote HTTP server instead of stdio
    #[arg(long)]
    http: bool,

    /// Host to bind the HTTP server to (default: 127.0.0.1)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to listen on (default: 3000)
    #[arg(short, long, default_value = "3000")]
    port: u16,
}

// ── Entry point ──────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // ── 1. Load config ──
    let config = Config::load(cli.config.as_deref())?;

    // ── 2. Init tracing (MUST go to stderr — stdout is MCP) ──
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&config.server.log_level)),
        )
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();

    tracing::info!(
        name = %config.server.name,
        version = %config.server.version,
        "Starting searchxyz MCP server"
    );

    // ── 3. Build shared cache ──
    let cache = Arc::new(Mutex::new(Cache::load_from_file(
        config.cache.max_entries,
        config.cache.ttl_secs,
        &config.cache.path,
    )));

    // ── 4. Build components ──
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.crawler.timeout_secs))
        .user_agent(&config.crawler.user_agent)
        .build()?;

    // Search backends (in configured order).
    let mut backends: Vec<Box<dyn search::SearchBackend>> = Vec::new();
    for name in &config.search.backends {
        match name.as_str() {
            "duckduckgo" => {
                backends.push(Box::new(DuckDuckGoBackend::new(http_client.clone())));
            }
            "google" => {
                backends.push(Box::new(GoogleBackend::new(http_client.clone())));
            }
            "bing" => {
                backends.push(Box::new(BingBackend::new(http_client.clone())));
            }
            "brave" => {
                backends.push(Box::new(BraveBackend::new(
                    http_client.clone(),
                    config.brave.clone(),
                )));
            }
            "searxng" => {
                backends.push(Box::new(SearXngBackend::new(
                    http_client.clone(),
                    config.searxng.clone(),
                )));
            }
            other => {
                tracing::warn!(backend = other, "Unknown search backend, skipping");
            }
        }
    }

    let dispatcher = SearchDispatcher::new(backends);
    let crawler = Crawler::new(
        config.crawler.clone(),
        config.headless.clone(),
        config.proxy.clone(),
        cache.clone(),
    );
    let extractor = ExtractionPipeline::new(config.extractor.clone());
    let index = SearchIndex::open(&config.index)?;

    // Load Knowledge Graph
    let graph_path = std::path::Path::new(&config.index.path).join("graph.json");
    let graph = Arc::new(Mutex::new(graph::KnowledgeGraph::load_from_file(&graph_path).unwrap_or_else(|e| {
        tracing::warn!(path = ?graph_path, error = %e, "Failed to load knowledge graph, starting fresh");
        graph::KnowledgeGraph::new()
    })));

    // ── 5. Build MCP server ──
    let server = SearchXyzServer::new(
        dispatcher,
        crawler,
        extractor,
        index,
        cache.clone(),
        graph.clone(),
        config.clone(),
    );

    if cli.http {
        let addr = format!("{}:{}", cli.host, cli.port);
        tracing::info!(bind = %addr, "Starting searchxyz remote HTTP server");

        let streamable_service = rmcp::transport::streamable_http_server::StreamableHttpService::new(
            move || Ok(server.clone()),
            rmcp::transport::streamable_http_server::session::local::LocalSessionManager::default().into(),
            Default::default(),
        );

        let mut app = axum::Router::new().nest_service("/mcp", streamable_service);
        if let Some(ref auth_token) = config.server.auth_token {
            app = app.layer(axum::middleware::from_fn_with_state(
                auth_token.clone(),
                auth_middleware,
            ));
        }

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        tracing::info!("HTTP server listening on http://{}/mcp", addr);

        axum::serve(listener, app).await?;
    } else {
        // ── 6. Start MCP server on stdio ──
        tracing::info!("MCP server listening on stdio");

        let service = server.serve(stdio()).await.inspect_err(|e| {
            tracing::error!(error = %e, "Failed to start server");
        })?;

        // ── 7. Wait for shutdown ──
        tokio::select! {
            result = service.waiting() => {
                if let Err(e) = result {
                    tracing::error!(error = %e, "Server error");
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received Ctrl+C, shutting down");
            }
        }
    }

    // ── 8. Save cache to disk ──
    tracing::info!("Saving cache to disk...");
    let cache_guard = cache.lock().await;
    if let Err(e) = cache_guard.save_to_file(&config.cache.path) {
        tracing::error!(error = %e, "Failed to save cache to disk");
    } else {
        tracing::info!(path = ?config.cache.path, "Cache saved successfully");
    }

    // Save Knowledge Graph to disk
    tracing::info!("Saving knowledge graph to disk...");
    let graph_guard = graph.lock().await;
    if let Err(e) = graph_guard.save_to_file(&graph_path) {
        tracing::error!(path = ?graph_path, error = %e, "Failed to save knowledge graph to disk");
    } else {
        tracing::info!(path = ?graph_path, "Knowledge graph saved successfully");
    }

    tracing::info!("searchxyz server stopped");

    Ok(())
}

async fn auth_middleware(
    axum::extract::State(expected_token): axum::extract::State<String>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    if let Some(auth) = auth_header {
        if auth.starts_with("Bearer ") && auth[7..] == expected_token {
            return Ok(next.run(req).await);
        }
    }
    Err(axum::http::StatusCode::UNAUTHORIZED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_auth_middleware_blocked() {
        let app = Router::new().route("/", get(|| async { "ok" })).layer(
            axum::middleware::from_fn_with_state("secret-token".to_string(), auth_middleware),
        );

        // 1. Missing header (should fail/401, but will return 200 in dummy)
        let response = app
            .clone()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // 2. Mismatched token
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("Authorization", "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // 3. Bad header format
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("Authorization", "Basic secret-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_middleware_allowed() {
        let app = Router::new().route("/", get(|| async { "ok" })).layer(
            axum::middleware::from_fn_with_state("secret-token".to_string(), auth_middleware),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("Authorization", "Bearer secret-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
