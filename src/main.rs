mod cache;
mod config;
mod crawler;
mod error;
mod extractor;
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
use search::{SearchDispatcher, brave::BraveBackend, duckduckgo::DuckDuckGoBackend};
use tools::SearchXyzServer;

use rmcp::{ServiceExt, transport::stdio};

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
    let cache = Arc::new(Mutex::new(Cache::new(
        config.cache.max_entries,
        config.cache.ttl_secs,
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
                backends.push(Box::new(DuckDuckGoBackend::new(
                    http_client.clone(),
                )));
            }
            "brave" => {
                backends.push(Box::new(BraveBackend::new(
                    http_client.clone(),
                    config.brave.clone(),
                )));
            }
            other => {
                tracing::warn!(backend = other, "Unknown search backend, skipping");
            }
        }
    }

    let dispatcher = SearchDispatcher::new(backends);
    let crawler = Crawler::new(config.crawler.clone(), cache.clone());
    let extractor = ExtractionPipeline::new(config.extractor.clone());
    let index = SearchIndex::open(&config.index)?;

    // ── 5. Build MCP server ──
    let server = SearchXyzServer::new(
        dispatcher,
        crawler,
        extractor,
        index,
        cache,
        config.clone(),
    );

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

    tracing::info!("searchxyz server stopped");

    Ok(())
}