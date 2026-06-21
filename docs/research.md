# searchxyz — Research Document

> **Last updated:** 2026-06-21
> **Status:** Active Research
> **Purpose:** Comprehensive reference for architecture decisions, crate selection, competitor analysis, and best practices driving the searchxyz MCP tool server.

---

## Table of Contents

1. [MCP Protocol & Rust SDK](#1-mcp-protocol--rust-sdk)
2. [Rust Crates Ecosystem](#2-rust-crates-ecosystem)
3. [DuckDuckGo Integration](#3-duckduckgo-integration)
4. [Competitor Deep Dives & Inspirations](#4-competitor-deep-dives)
5. [Search API Landscape (2026)](#5-search-api-landscape-2026)
6. [Performance Research](#6-performance-research)
7. [Error Handling Best Practices](#7-error-handling-best-practices)
8. [Content Extraction Best Practices](#8-content-extraction-best-practices)
9. [Key Design Insights](#9-key-design-insights)

---

## 1. MCP Protocol & Rust SDK

### Overview

The **Model Context Protocol (MCP)** is the emerging standard for connecting AI agents to external tools and data sources. It defines a JSON-RPC 2.0 based protocol where a **host** (the AI agent runtime) communicates with **servers** (tool providers like searchxyz) through a well-defined request/response lifecycle.

### SDK: `rmcp`

- **Crate:** [`rmcp`](https://crates.io/crates/rmcp) — the official Rust SDK for MCP
- **Version:** `~0.16.0` (pin to minor for stability)
- **Feature flags:**
  - `server` — enables server-side primitives (tool registration, request handling)
  - `client` — enables client-side primitives (not needed for searchxyz)
  - `transport-stdio` — stdio transport layer
  - `transport-sse` — SSE/Streamable HTTP transport layer

### Transport Modes

| Transport | Use Case | Characteristics |
|-----------|----------|-----------------|
| **stdio** | Local agents (Claude Desktop, Cursor, etc.) | Fastest, no network overhead, stdin/stdout pipes |
| **Streamable HTTP** | Networked / remote agents | HTTP-based, supports multiple concurrent clients, firewall-friendly |

**searchxyz targets stdio as the primary transport** for local AI agent integration. Streamable HTTP is a future consideration for hosted deployments.

### Critical Rules

> [!CAUTION]
> **NEVER use `println!()` with stdio transport.** The MCP stdio transport uses stdout as its JSON-RPC communication channel. Any stray `println!()` output will inject non-JSON bytes into the stream, corrupting the protocol and causing the agent to disconnect or crash. Use `eprintln!()` or a proper logging framework (`tracing`) that writes to stderr.

> [!IMPORTANT]
> **Tool descriptions are the most important metadata you write.** The LLM reads tool descriptions to decide *which* tool to call and *how* to call it. A vague description means the agent will misuse or skip the tool entirely. Write descriptions as if you're writing API docs for a developer who can't see the source code.

### Tool Design Principles

1. **Snake_case names** — `web_search`, `extract_content`, not `webSearch` or `ExtractContent`
2. **Idempotent** — calling a tool twice with the same input should produce the same output
3. **Deterministic** — avoid randomness unless explicitly documented
4. **Actionable errors** — error messages should tell the AI agent *what to do next*, not just *what went wrong*
5. **Minimal side effects** — search/read tools should never modify external state

### Macro-Driven API

The `rmcp` crate provides procedural macros for ergonomic tool definitions:

```rust
use rmcp::tool;

#[tool(
    name = "web_search",
    description = "Search the web using DuckDuckGo and return ranked results with titles, URLs, and snippets. Use this when you need to find information on the internet."
)]
async fn web_search(
    #[arg(description = "The search query string")] query: String,
    #[arg(description = "Maximum number of results to return (1-10, default 5)")] max_results: Option<u32>,
) -> Result<Vec<SearchResult>, ToolError> {
    // implementation
}
```

### JSON-RPC Lifecycle

```
Agent (Client)                    searchxyz (Server)
     |                                  |
     |--- initialize ------------------>|
     |<-- initialize response ----------|
     |--- initialized notification ---->|
     |                                  |
     |--- tools/list ------------------>|
     |<-- tools/list response ----------|
     |                                  |
     |--- tools/call (web_search) ----->|
     |<-- tools/call response ----------|
     |                                  |
```

### Logging Strategy

- Use the `tracing` crate for structured logging
- All log output goes to **stderr** (never stdout)
- Log levels: `ERROR` for failures, `WARN` for degraded behavior, `INFO` for tool invocations, `DEBUG`/`TRACE` for development

---

## 2. Rust Crates Ecosystem

### HTTP & Networking

| Crate | Purpose | Why Chosen | Key Features |
|-------|---------|------------|--------------|
| **reqwest** | HTTP client | De facto standard, async, robust | Connection pooling, TLS, cookie jar, redirect control, proxy support, `rustls` backend |
| **tokio** | Async runtime | Industry standard, mature ecosystem | Multi-threaded work-stealing, timers, I/O, channels, `#[tokio::main]` macro |
| **governor** | Rate limiting | Token bucket algorithm, composable | Per-key limits, burst allowance, async-compatible, zero-alloc fast path |

**Notes:**
- `reqwest` with `rustls-tls` avoids OpenSSL dependency headaches on various platforms
- Connection pooling in `reqwest` is automatic — a single `Client` instance reuses connections
- `governor` integrates cleanly with `tokio` for async rate limiting without blocking threads

### HTML Parsing

| Crate | Purpose | Why Chosen | Key Features |
|-------|---------|------------|--------------|
| **scraper** | CSS selector-based HTML parsing | Ergonomic API, battle-tested | CSS selectors, DOM traversal, text extraction, built on `html5ever` |
| **html5ever** | Low-level HTML5 parser | Spec-compliant, Servo heritage | Streaming parser, handles malformed HTML gracefully, tree construction |

**Notes:**
- `scraper` wraps `html5ever` with a jQuery-like selector API — ideal for extracting specific elements from DuckDuckGo result pages
- For most use cases, `scraper` is sufficient; drop to `html5ever` only for streaming/memory-constrained scenarios

### Content Extraction

| Crate | Purpose | Why Chosen | Key Features |
|-------|---------|------------|--------------|
| **rs-trafilatura** | Full-page content extraction | Best quality for articles, academic papers, blog posts | Extracts main content, removes boilerplate, handles complex layouts, metadata extraction |
| **readability-rust** | Mozilla Readability port | Proven algorithm (used in Firefox Reader View) | Distills page to main content, scoring-based, handles most news/blog pages |
| **defuddle-rs** | Simple content extraction | Lightweight, fast, minimal dependencies | Quick extraction for simple pages, good fallback option |

**Decision:** Use **rs-trafilatura** as primary extractor, **readability-rust** as fallback for pages where trafilatura struggles, and **defuddle-rs** for quick/simple extraction needs.

**Notes:**
- rs-trafilatura is a Rust port of the Python `trafilatura` library which is widely regarded as the best open-source content extractor
- Content extraction quality directly impacts token efficiency — a good extractor can reduce token usage by 50-80% compared to raw HTML

### HTML → Markdown Conversion

| Crate | Purpose | Why Chosen | Key Features |
|-------|---------|------------|--------------|
| **mdka** | Fast HTML→Markdown | Speed, minimal overhead | Handles common HTML elements, lightweight, good for bulk conversion |
| **html-to-markdown-rs** | CommonMark-compliant conversion | Spec correctness | Full CommonMark output, handles tables, code blocks, nested lists |
| **spider_transformations** | AI-focused conversion | Optimized for LLM consumption | Cleans output for AI readability, handles edge cases in web content |

**Decision:** Use **mdka** as default for speed, fall back to **html-to-markdown-rs** when CommonMark compliance matters (e.g., structured content with tables).

### Search Indexing

| Crate | Purpose | Why Chosen | Key Features |
|-------|---------|------------|--------------|
| **tantivy** | Full-text search engine | 2x faster than Lucene, pure Rust | BM25 scoring, memory-mapped files, concurrent indexing, faceted search, phrase queries |

**Notes:**
- Tantivy is the clear choice for any local search indexing needs
- Memory-mapped I/O means indexes can exceed available RAM
- `MmapDirectory` keeps memory footprint minimal even with large indexes
- BM25 scoring out of the box — same relevance algorithm used by Elasticsearch
- Future use: local caching of search results, building a searchable index of crawled content

### Error Handling

| Crate | Purpose | Why Chosen | Key Features |
|-------|---------|------------|--------------|
| **thiserror** | Structured error types | Derive macro for `Display` + `Error`, zero runtime cost | `#[error()]` attribute, `#[from]` for conversion, `#[source]` for chaining |
| **anyhow** | Application-level errors | Context chaining, `?` operator ergonomics | `.context()`, `.with_context()`, downcasting, `bail!()` macro |
| **backoff** | Exponential retry | Configurable, async-native | Jitter, max elapsed time, max retries, async/sync support |

**Decision:** Use **thiserror** for the library/core error types (matchable, structured), **anyhow** in binary/application code (context-rich), and **backoff** for any network I/O that may transiently fail.

### Web Crawling (Future / Reference)

| Crate | Purpose | Why Chosen | Key Features |
|-------|---------|------------|--------------|
| **spider** | High-performance web crawler | Fastest Rust crawler, production-tested | Concurrent crawling, robots.txt, sitemap parsing, configurable depth |
| **chromiumoxide** | Headless Chrome automation | Full JS rendering capability | CDP protocol, page screenshots, JS evaluation, network interception |

**Notes:**
- Not needed for MVP, but relevant for future `crawl_site` tool
- `chromiumoxide` is the path to JavaScript-rendered page support (SPAs, dynamic content)

---

## 3. DuckDuckGo Integration

### API Landscape

DuckDuckGo does **not** offer an official full web search API. What exists:

| Endpoint | What It Does | Limitations |
|----------|-------------|-------------|
| **Instant Answer API** (`api.duckduckgo.com`) | Returns instant answers, abstracts, related topics | No full web search results, no organic links, no snippets |
| **DuckDuckGo Lite** (`lite.duckduckgo.com`) | Minimal HTML search results page | Must be scraped, no official support, subject to blocking |

### Scraping Strategy

Since we need full search results (titles, URLs, snippets), we must scrape **DuckDuckGo Lite HTML**:

1. **Send GET request** to `https://lite.duckduckgo.com/lite/?q={query}`
2. **Parse HTML** response with `scraper` crate
3. **Extract** result titles, URLs, and snippets from the table-based layout
4. **Decode** redirect URLs (DuckDuckGo wraps links in tracking redirects)

### Available Crates

| Crate | Status | Notes |
|-------|--------|-------|
| **duckduckgo** | Community crate | Wraps DuckDuckGo scraping, may be outdated |
| **websearch** | Community crate | Generic web search abstraction, includes DuckDuckGo |

**Decision:** Evaluate community crates first but be prepared to implement scraping directly — community crates may lag behind DDG HTML changes.

### Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| **IP bans** | Search requests blocked | Rate limiting (governor), request spacing |
| **CAPTCHAs** | Requires human intervention | User-Agent rotation, reduced request frequency |
| **HTML structure changes** | Parser breaks | Defensive parsing, multiple CSS selector fallbacks |
| **Geo-blocking** | Different results by region | Accept `kl` parameter for region control |

### Mitigation Strategies

1. **Rate limiting:** Use `governor` to enforce max 1 request per second to DuckDuckGo
2. **User-Agent rotation:** Maintain a pool of realistic browser User-Agent strings, rotate per request
3. **Request spacing:** Add random jitter (100-500ms) between consecutive requests
4. **Fallback chain:** DuckDuckGo → Brave Search API → cached results
5. **Error detection:** Detect CAPTCHA pages and rate-limit responses, surface actionable error to agent

### Request Headers

```
User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ...
Accept: text/html,application/xhtml+xml
Accept-Language: en-US,en;q=0.9
Referer: https://lite.duckduckgo.com/
```

---

## 4. Competitor Deep Dives

### 4.1 Exa Architecture

**What it is:** AI-native search engine with neural/embedding-based semantic search.

**Core Technology:**
- Custom **transformer models** trained specifically for web content similarity
- **Vector database** storing embeddings of billions of web pages
- Embedding-based retrieval — queries are embedded and matched against page embeddings via approximate nearest neighbor (ANN) search

**Pipeline Orchestrator: "Canon"**
- Internal system that models search as a **directed acyclic graph (DAG)** of parallelizable processing nodes
- Nodes include: query understanding, embedding generation, ANN search, re-ranking, snippet extraction, content fetching
- Enables flexible pipeline composition — different search types activate different node subgraphs

**Search Types:**

| Type | Latency | Mechanism | Use Case |
|------|---------|-----------|----------|
| **Instant** | 200-450ms | Embedding similarity, pre-computed results | Quick factual queries |
| **Deep** | 4-40s | Full re-ranking, content fetching, synthesis | Research, complex queries |

**Key Features:**
- **Highlights:** Returns relevant text excerpts (highlights) alongside URLs to reduce token consumption
- **Content fetching:** Can return cleaned page content directly (no separate scraping needed)
- **Similarity search:** "Find pages similar to this URL" — unique capability
- **Date filtering:** Filter by publication date, recency

**Relevance to searchxyz:**
- Exa's highlight extraction is a pattern worth emulating — return the most relevant snippets, not full pages
- The DAG pipeline concept could inform our internal tool architecture
- We cannot replicate neural search locally, but we can approximate relevance ranking with BM25 (tantivy)

### 4.2 Firecrawl Architecture

**What it is:** AI-focused web scraping and crawling service, optimized for LLM consumption.

**Multi-Stage Pipeline:**

```
URL → Render → Extract → Convert → Output
       │          │          │         │
       ▼          ▼          ▼         ▼
   Headless    Noise      HTML →    Markdown
   Chrome    Filtering   Markdown    + metadata
   (full JS)  (nav,ads,              (clean,
              footer,                 token-
              scripts,               optimized)
              cookies)
```

**Stage Details:**

1. **Render:** Full JavaScript rendering via headless Chrome / Playwright
   - Handles SPAs, lazy-loaded content, infinite scroll
   - Browser actions: click, scroll, wait for element
   - Screenshot capture for visual analysis

2. **Extract:** Intelligent noise filtering
   - Removes: navigation bars, footers, advertisements, cookie banners, script tags, style blocks
   - Keeps: main article content, images, tables, code blocks
   - Uses heuristic scoring similar to Mozilla Readability

3. **Convert:** HTML → Markdown transformation
   - Preserves semantic structure (headings, lists, tables, code)
   - Optimized for LLM token efficiency
   - Optional: structured data extraction with LLM-powered schemas

4. **Output:** Clean markdown + metadata
   - Title, description, language, word count
   - Links extracted and categorized
   - Optional: screenshots, structured JSON

**Specialized Pipelines:**
- **PDF Pipeline:** Dedicated PDF processing with OCR support for scanned documents
- **Map endpoint:** Sitemap discovery and URL enumeration for a domain

**Infrastructure:**
- Proxy rotation to avoid IP bans
- Rate limiting (per-user and global)
- Anti-bot detection bypass (stealth browser configurations)
- Queue-based processing for large batch jobs

**Relevance to searchxyz:**
- The multi-stage pipeline (render → extract → convert) is the gold standard architecture
- For MVP, we skip the render stage (no headless Chrome) and go straight to HTTP fetch → extract → convert
- Firecrawl's noise filtering approach validates our choice of rs-trafilatura + markdown conversion
- Their PDF pipeline is a future feature consideration

### 4.3 Brave Search

**What it is:** Independent search engine with its own first-party index (not a Google/Bing wrapper).

**Key Characteristics:**
- **First-party index:** Brave built and maintains its own web index, crawled by their own infrastructure
- **No dependency on Google/Bing:** Results are independent, often surface different content
- **Clean API:** Well-structured JSON responses with title, URL, description, and rich snippets
- **Reliability:** Enterprise-grade uptime, consistent response format

**Pricing:**
- **$5 per 1,000 requests** (Pay-as-you-go)
- **Free tier:** 2,000 requests/month (sufficient for development/testing)
- No complex tiering — straightforward pricing

**API Response Quality:**
- Structured snippets with highlighted query terms
- Rich results: news, videos, FAQ snippets, infoboxes
- Location-aware results with geo-parameters
- Language and SafeSearch controls

**Relevance to searchxyz:**
- **Primary fallback** when DuckDuckGo scraping fails or is rate-limited
- Clean API means reliable, structured results without HTML parsing
- Cost-effective at scale for a paid tier
- Potential future default backend if DuckDuckGo scraping proves unreliable

### 4.4 Tavily

**What it is:** AI-agent-native search API — designed specifically for LLM/agent workflows.

**Key Characteristics:**
- **One-call search + extraction:** A single API call returns search results AND extracted page content
- **LLM-optimized output:** Responses are pre-formatted for token efficiency
- **Agent framework integration:** First-class support in LangChain, LlamaIndex, CrewAI, AutoGPT
- **Standard in agent tooling:** Has become the default search tool in many agent frameworks

**Features:**
- Search with configurable depth (basic, advanced)
- Content extraction with quality scoring
- Domain include/exclude filtering
- Real-time search capabilities
- Topic-based search categorization

**Pricing:**
- Free tier: 1,000 requests/month
- Paid plans scale with usage
- Good for prototyping, potentially expensive at scale

**Relevance to searchxyz:**
- Tavily validates the "search + extract in one call" pattern
- Their success proves there's demand for AI-native search tools
- searchxyz differentiates by being **local-first, open-source, and free** vs. Tavily's API-based model
- We can study their API response format for inspiration on our tool output schema

### 4.5 Crawlee — Bot Evasion & Header Fingerprinting

**What it is:** A web scraping and browser automation library by Apify, designed to mimic human browsing patterns to bypass anti-scraping protections.

**Key Characteristics & Mechanisms:**
- **Fingerprint Generation:** Dynamically generates authentic desktop and mobile HTTP headers (User-Agent, Accept, Accept-Language, Sec-Ch-Ua) matching specific browser engines (Chrome, Firefox, Safari).
- **Session Management:** Simulates realistic cookies, request intervals, and TCP/TLS handshakes to make requests indistinguishable from natural browser traffic.

**Relevance to searchxyz:**
- Inspired Phase 1 of our scraper evasion. We integrated randomized browser headers using a thread-safe static header generator (`src/crawler/fingerprint.rs`).
- Allows searchxyz to run keyless scrapers (like DuckDuckGo Lite, Google, and Bing) directly from arbitrary server IPs without immediately triggering CAPTCHAs or 403 Forbidden errors.

### 4.6 Katana — Scoped Recursive Spidering

**What it is:** A next-generation crawling and web spidering tool by ProjectDiscovery, written in Go, built for security research and endpoint discovery.

**Key Characteristics & Mechanisms:**
- **Scope Checking:** Validates links on parsed pages to ensure they remain inside allowed domains or subdomains, preventing the crawler from wandering off into external sites.
- **Recursive Queueing:** Parses all anchor elements (`<a>`) on an HTML page, filters them using scope logic, and queues child links for multi-threaded async crawling up to a configured depth.

**Relevance to searchxyz:**
- Inspired Phase 2 of our crawler. We built recursive scoped spidering (`src/crawler/spider.rs`) using Tokio's asynchronous `JoinSet` workers to parse HTML links dynamically and crawl up to a configured depth (default max depth = 3) restricted to the starting domain.

### 4.7 Websurfx & searxng-mcp — Native HTML Search Scrapers

**What it is:** `Websurfx` is a privacy-focused meta-search engine written in Rust that scrapes upstream search engines. `searxng-mcp` is an MCP server wrapping SearXNG JSON queries.

**Key Characteristics & Mechanisms:**
- **Zero API Key Requirement:** Scrapes the public HTML search results of engines (Google, Bing) directly inside the Rust binary, bypassing the need for commercial developer API keys.
- **HTML DOM Selection:** Uses CSS selectors and DOM traversing to extract structured search result arrays (title, URL, snippet) from engine result pages.

**Relevance to searchxyz:**
- Inspired Phase 3 of our server. We built native scraping engines for Google (`src/search/google.rs`) and Bing (`src/search/bing.rs`) directly in Rust, avoiding external Python/Docker/Node dependencies. We use combined CSS selectors (`div.g, div.MjjYud` for Google, `li.b_algo` for Bing) to parse result titles, URLs, and snippets in document order.

---

## 5. Search API Landscape (2026)

| Provider | Best For | Key Advantage | Price | Free Tier | API Quality |
|----------|----------|---------------|-------|-----------|-------------|
| **Brave** | Reliable web search | Independent first-party index, clean structured responses | $5/1K requests | 2,000 req/mo | ⭐⭐⭐⭐⭐ Excellent |
| **Tavily** | Agent prototyping | Search + extraction in one call, LangChain standard | Usage-based | 1,000 req/mo | ⭐⭐⭐⭐ Very Good |
| **Exa** | Semantic/research search | Neural embeddings, similarity search, highlights | Usage-based | 1,000 req/mo | ⭐⭐⭐⭐ Very Good |
| **Firecrawl** | Web scraping/crawling | Full JS rendering, multi-stage pipeline, PDF support | Usage-based | 500 credits/mo | ⭐⭐⭐⭐ Very Good |
| **DuckDuckGo** | Free, privacy-first | No API key needed, no tracking | Free (scraping) | Unlimited* | ⭐⭐ Fragile |

*\*DuckDuckGo is "free" but requires scraping, which is fragile and subject to blocking.*

### Analysis

**For searchxyz MVP:**
- **Primary:** DuckDuckGo (free, no API key barrier)
- **Fallback:** Brave Search API (reliable, affordable)
- **Future:** Consider Exa for semantic search capabilities

**Why not just use Brave?**
- API key requirement adds friction for first-time users
- Goal is zero-config out-of-the-box experience
- DuckDuckGo provides that, with Brave as the reliable safety net

**Why not Tavily?**
- searchxyz IS the Tavily replacement — local, free, open-source
- No point in wrapping another paid API when the goal is self-hosted tooling

---

## 6. Performance Research

### Memory Footprint

| Component | Expected Memory | Notes |
|-----------|----------------|-------|
| Rust HTTP crawler | 40-80 MB stable | reqwest client + response buffers |
| Tantivy index (idle) | 10-20 MB | Memory-mapped, only maps active segments |
| Tantivy index (searching) | 20-100 MB | Depends on query complexity, result set |
| MCP server overhead | < 5 MB | Minimal — just JSON-RPC handling |
| **Total (typical)** | **60-150 MB** | Well within agent host constraints |

### Tantivy Memory Optimization

- **MmapDirectory:** Uses memory-mapped files instead of loading entire index into RAM
  - OS manages page cache — only accessed pages consume physical memory
  - Index can be larger than available RAM
  - Graceful degradation under memory pressure (OS evicts pages)
- **Segment merging:** Background process consolidates small segments, keeping index healthy
- **Commit frequency:** Batch writes and commit periodically, not per-document

```rust
use tantivy::directory::MmapDirectory;
use tantivy::Index;

let directory = MmapDirectory::open("/path/to/index")?;
let index = Index::open(directory)?;
```

### HTTP Client Optimization

**Connection Pooling (reqwest):**
- Create a **single `reqwest::Client` instance** and reuse it for all requests
- The client automatically maintains a connection pool
- Connections are reused for subsequent requests to the same host
- Default pool idle timeout: 90 seconds

```rust
// GOOD: Single client, reused across requests
let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(30))
    .connect_timeout(Duration::from_secs(10))
    .pool_max_idle_per_host(5)
    .user_agent("searchxyz/0.1.0")
    .build()?;

// BAD: New client per request (no connection reuse)
// let client = reqwest::Client::new(); // Don't do this in a loop
```

### Rate Limiting with Governor

```rust
use governor::{Quota, RateLimiter};
use std::num::NonZeroU32;

// 1 request per second with burst of 3
let limiter = RateLimiter::direct(
    Quota::per_second(NonZeroU32::new(1).unwrap())
        .allow_burst(NonZeroU32::new(3).unwrap())
);

// Before each request:
limiter.until_ready().await;
```

### Backpressure with Bounded Channels

When processing multiple URLs concurrently, use bounded channels to prevent unbounded memory growth:

```rust
use tokio::sync::mpsc;

// Bounded channel — if buffer is full, sender blocks
let (tx, rx) = mpsc::channel::<CrawlResult>(100); // max 100 pending results

// Producer (crawler) will slow down when consumer (processor) can't keep up
// This prevents OOM from unbounded queuing
```

### Concurrency Strategy

- Use `tokio::spawn` for I/O-bound tasks (HTTP requests, file I/O)
- Use `tokio::sync::Semaphore` to limit concurrent requests to a single host
- Target: max 5 concurrent requests globally, max 1 per host

---

## 7. Error Handling Best Practices

### Error Type Strategy

> [!IMPORTANT]
> **Never use `Result<T, String>` in production code.** String errors are not matchable, not structured, and provide no programmatic context for recovery. Always define proper error enums with `thiserror`.

#### Library Errors (thiserror)

Use `thiserror` for all error types in the core library. These errors are:
- **Structured:** Enum variants for different failure modes
- **Matchable:** Callers can `match` on variants and handle specifically
- **Composable:** `#[from]` for automatic conversion from underlying errors

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SearchError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Rate limited by {provider} — retry after {retry_after_secs}s")]
    RateLimited {
        provider: String,
        retry_after_secs: u64,
    },

    #[error("Failed to parse search results from {url}: {reason}")]
    ParseError {
        url: String,
        reason: String,
    },

    #[error("No results found for query: {query}")]
    NoResults {
        query: String,
    },

    #[error("Content extraction failed for {url}: {reason}")]
    ExtractionError {
        url: String,
        reason: String,
    },
}
```

#### Application Errors (anyhow)

Use `anyhow` in the binary/application layer for rich context chaining:

```rust
use anyhow::{Context, Result};

async fn handle_search_request(query: &str) -> Result<SearchResponse> {
    let results = search_duckduckgo(query)
        .await
        .context("DuckDuckGo search failed")?;

    let extracted = extract_content(&results[0].url)
        .await
        .with_context(|| format!("Failed to extract content from {}", results[0].url))?;

    Ok(SearchResponse { results, extracted })
}
```

### Retry Strategy (backoff)

```rust
use backoff::ExponentialBackoffBuilder;
use std::time::Duration;

let backoff = ExponentialBackoffBuilder::new()
    .with_initial_interval(Duration::from_millis(500))
    .with_multiplier(2.0)
    .with_randomization_factor(0.25)  // jitter
    .with_max_interval(Duration::from_secs(30))
    .with_max_elapsed_time(Some(Duration::from_secs(120)))  // NEVER retry forever
    .build();

let result = backoff::future::retry(backoff, || async {
    make_request().await.map_err(|e| {
        if e.is_transient() {
            backoff::Error::transient(e)
        } else {
            backoff::Error::permanent(e)
        }
    })
}).await?;
```

### Error Messages as Interfaces

> [!TIP]
> Error messages in an MCP tool are **interfaces for AI agents**. The agent reads the error message to decide what to do next. Write error messages that are **actionable**.

**Bad error messages:**
```
"Error: request failed"
"Something went wrong"
"Parse error"
```

**Good error messages (actionable for AI agents):**
```
"Rate limited by DuckDuckGo. Retry this search in 30 seconds, or use a different query."
"Failed to extract content from https://example.com — the page requires JavaScript rendering which is not supported. Try a different URL."
"No search results found for 'quantum chromodynamics applications'. Try broadening the query or using different keywords."
"Connection timeout after 30s for https://example.com. The site may be down. Try an alternative source."
```

### Retry Rules

1. **Always cap retries** — use `max_elapsed_time` or `max_retries`
2. **Always use jitter** — prevents thundering herd on shared resources
3. **Classify errors:** Only retry transient failures (network timeouts, 503s), never permanent failures (404s, auth errors)
4. **Log every retry** — at `WARN` level, include attempt number and backoff duration

---

## 8. Content Extraction Best Practices

### The Golden Rule

> [!CAUTION]
> **ALWAYS pair content extraction with markdown conversion.** Never run a markdown converter directly on full-page HTML. The pipeline MUST be: Fetch → **Extract** → Convert to Markdown.

### Why This Matters

Running a markdown converter on raw HTML produces:

| Approach | Output Size | Token Usage | Quality |
|----------|------------|-------------|---------|
| Raw HTML → Markdown | ~15,000 tokens | ❌ Extremely wasteful | ❌ Full of noise (nav, ads, footer, scripts) |
| Extract → Markdown | ~3,000 tokens | ✅ 80% reduction | ✅ Clean, focused content |

**The extraction step is not optional.** It is the single most impactful optimization for token efficiency in an AI-agent search tool.

### Extraction Pipeline

```
     URL
      │
      ▼
 ┌──────────┐
 │  Fetch    │  reqwest HTTP GET → raw HTML bytes
 └──────────┘
      │
      ▼
 ┌──────────┐
 │ Extract   │  rs-trafilatura / readability-rust → main content HTML
 └──────────┘
      │
      ▼
 ┌──────────┐
 │ Convert   │  mdka / html-to-markdown-rs → clean Markdown
 └──────────┘
      │
      ▼
  Clean Markdown
  (ready for LLM)
```

### Extractor Selection Guide

| Extractor | Best For | Strengths | Weaknesses |
|-----------|----------|-----------|------------|
| **rs-trafilatura** | Articles, blog posts, academic papers, news | Highest quality extraction, metadata support, handles complex layouts | Slightly heavier, may over-extract on simple pages |
| **readability-rust** | News articles, blog posts | Proven algorithm (Firefox Reader View), good general-purpose | Less effective on non-article pages (forums, wikis) |
| **defuddle-rs** | Quick extraction, simple pages | Fast, minimal dependencies, easy to integrate | Lower quality on complex pages |

### Fallback Strategy

```
rs-trafilatura → readability-rust → defuddle-rs → raw HTML truncated
```

If the primary extractor returns empty or very short content (< 100 characters), fall back to the next extractor in the chain.

### Token Budget Awareness

- Set a **maximum output length** for extracted content (e.g., 8,000 tokens / ~32,000 characters)
- Truncate intelligently at paragraph boundaries, not mid-sentence
- Include a `[content truncated]` indicator when truncation occurs
- Let the agent request specific sections if it needs more detail

---

## 9. Key Design Insights

> Collected learnings from all research above, distilled into actionable principles.

### Protocol & Transport
- MCP over stdio is the fastest path to agent integration — zero network overhead, works with every major AI coding tool
- `println!()` is a production-killing bug in stdio MCP servers — enforce this via clippy lint or code review
- Tool descriptions are the #1 UX element for AI agents — invest time in writing clear, comprehensive descriptions
- JSON-RPC error codes should map to actionable recovery strategies

### Architecture
- The fetch → extract → convert pipeline is the industry standard (Firecrawl, Exa, Tavily all use variants of it)
- Extraction before conversion is non-negotiable — 50-80% token savings
- Rate limiting must be built into the core, not bolted on — every external request goes through governor
- Connection pooling is free with reqwest — just reuse the client instance

### Search
- DuckDuckGo scraping is viable but fragile — always have Brave as a fallback
- No single search API is perfect — the best strategy is a fallback chain
- Brave Search offers the best reliability-to-cost ratio for a fallback provider
- Tavily's success proves the market wants AI-native search tools — searchxyz fills the open-source gap

### Content Quality
- rs-trafilatura is the best open-source content extractor available in any language
- Always truncate at semantic boundaries (paragraphs, sentences), never mid-word
- Return content length metadata so agents can decide if they need more or less detail
- Clean markdown output is worth more than perfectly formatted HTML

### Performance
- Rust gives us 40-80MB memory footprint — 10x less than equivalent Python/Node.js tools
- Tantivy's MmapDirectory means index size is bounded by disk, not RAM
- Bounded channels prevent OOM — never use unbounded channels in production
- Semaphore-based concurrency control is simpler and more correct than manual tracking

### Error Handling
- Error messages are an API for AI agents — they must be actionable, specific, and suggest recovery steps
- `thiserror` for library code, `anyhow` for application code — never mix them in the same layer
- Retry with jitter and caps — never retry forever, never retry without jitter
- Classify errors as transient vs. permanent — only retry transient failures

### Developer Experience
- Zero-config first experience — DDG search works without any API keys
- Fail fast with clear guidance — if Brave API key is missing, tell the agent exactly what to do
- Snake_case tool names for consistency with MCP ecosystem conventions
- Idempotent tools mean agents can safely retry without side effects

### Competitive Positioning
- searchxyz is the **open-source, local-first** answer to Tavily, Exa, and Firecrawl
- No API keys required for basic functionality (DuckDuckGo)
- Rust performance means it can run alongside resource-hungry LLMs without competition for RAM
- MCP-native from day one — not an HTTP API with an MCP wrapper bolted on

---

> [!NOTE]
> This document is a living reference. Update it as new crates are evaluated, competitor architectures evolve, or design decisions are refined during implementation.
