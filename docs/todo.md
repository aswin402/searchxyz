# TODO — searchxyz Task Breakdown

> **searchxyz** — A high-performance Rust MCP server for AI agent research.
>
> Phased build plan with checkboxes, time estimates, dependencies, and quality gates.
> Each phase builds on the previous — follow the dependency graph at the bottom.

---

## Phase 0: Project Bootstrap ⏱️ Day 1

> **Goal:** Skeleton compiles, MCP server starts and responds to `initialize`.
> **Dependencies:** None — this is the root.

- [ ] Initialize Cargo project with `cargo init`
- [ ] Set up directory structure:
  ```
  src/
  ├── search/       # Search backends (DDG, Brave)
  ├── crawler/      # HTTP fetching, retry, rate-limit
  ├── extractor/    # HTML → Markdown conversion
  ├── index/        # Tantivy local index
  ├── cache/        # LRU page cache
  ├── tools/        # MCP tool handlers
  ├── pipeline/     # Multi-step orchestration
  ├── config/       # Configuration loading
  └── error/        # Unified error types
  ```
- [ ] Add all dependencies to `Cargo.toml`:
  - **Core:** `rmcp`, `tokio`, `reqwest` (rustls-tls, gzip, brotli), `serde` + `serde_json`
  - **Search index:** `tantivy`
  - **Error handling:** `thiserror`, `anyhow`
  - **Observability:** `tracing`, `tracing-subscriber`
  - **Resilience:** `backoff`, `governor`
  - **HTML:** `scraper`
  - **Cache:** `lru`
  - **Config:** `toml`, `clap`
  - **Utilities:** `chrono`, `uuid`, `async-trait`
- [ ] Create `config.toml.example` with documented options
- [ ] Set up tracing subscriber (**output to stderr only!** — stdout is MCP JSON-RPC)
- [ ] Create basic `main.rs` with MCP server skeleton using `rmcp`
- [ ] **Verify:** `cargo build` succeeds, MCP server starts and responds to `initialize`

**Exit criteria:** `cargo run` → server listens on stdio → responds to MCP `initialize` request.

---

## Phase 1: Error Foundation ⏱️ Day 1–2

> **Goal:** Every failure in the system maps to a typed, actionable error.
> **Dependencies:** Phase 0

- [ ] Define `SearchXyzError` enum with all variants:
  | Variant                  | When                                      |
  |--------------------------|-------------------------------------------|
  | `SearchFailed`           | A search backend returns an error         |
  | `AllBackendsExhausted`   | Every backend tried, all failed           |
  | `CrawlFailed`            | HTTP fetch fails after retries            |
  | `HttpError`              | Non-2xx status code                       |
  | `Timeout`                | Request exceeded time budget              |
  | `ExtractionFailed`       | HTML → Markdown conversion fails          |
  | `EmptyContent`           | Extracted content < 50 chars              |
  | `IndexError`             | Tantivy read/write failure                |
  | `ConfigError`            | Invalid or missing configuration          |
  | `RateLimited`            | Governor / upstream 429                   |
- [ ] Implement `Display` and `Error` via `thiserror` derive macros
- [ ] Implement `From<reqwest::Error>` for `SearchXyzError`
- [ ] Implement `From<tantivy::TantivyError>` for `SearchXyzError`
- [ ] Implement `From<std::io::Error>` for `SearchXyzError`
- [ ] Create `to_mcp_error()` method for MCP error response conversion
- [ ] Ensure **ALL** error messages are actionable:
  - Include the URL that failed
  - Include the HTTP status code where applicable
  - Include a suggestion (e.g., "try again later", "check API key")
- [ ] Write unit tests for error `Display` formatting
- [ ] Write unit tests for error conversion (`From` impls)

**Exit criteria:** `cargo test` passes, every error variant renders a human-readable message.

---

## Phase 2: Configuration ⏱️ Day 2

> **Goal:** Flexible config with file + env vars + sensible defaults.
> **Dependencies:** Phase 1 (errors for `ConfigError`)

- [ ] Define `Config` struct with nested sub-configs:
  ```rust
  Config
  ├── ServerConfig        // transport, log_level
  ├── SearchConfig        // default_backend, max_results
  ├── BraveConfig         // api_key, endpoint
  ├── CrawlerConfig       // timeouts, user_agent, max_retries
  ├── ExtractorConfig     // max_content_length, strip_tags
  ├── IndexConfig         // data_dir, heap_size_mb
  └── CacheConfig         // max_entries, ttl_minutes
  ```
- [ ] Implement `serde::Deserialize` with `#[serde(default)]` on every field
- [ ] Implement `Default` for all config structs with sensible production values
- [ ] Implement TOML file loading from `~/.searchxyz/config.toml`
- [ ] Add environment variable overrides:
  | Env Var                       | Overrides                    |
  |-------------------------------|------------------------------|
  | `SEARCHXYZ_BRAVE_API_KEY`     | `brave.api_key`              |
  | `SEARCHXYZ_LOG_LEVEL`         | `server.log_level`           |
  | `SEARCHXYZ_DATA_DIR`          | `index.data_dir`             |
  | `SEARCHXYZ_MAX_RETRIES`       | `crawler.max_retries`        |
  | `SEARCHXYZ_CACHE_TTL`         | `cache.ttl_minutes`          |
- [ ] Implement config validation:
  - `timeout > 0`
  - `max_results > 0 && max_results <= 50`
  - `cache.max_entries > 0`
  - `crawler.max_retries <= 10`
- [ ] Write `config.toml.example` with **all** options documented inline
- [ ] Write unit tests for default config values
- [ ] Write unit tests for TOML parsing (valid + invalid)
- [ ] Write unit tests for env var overrides

**Exit criteria:** Config loads from file → env vars override → defaults fill gaps → validation catches bad values.

---

## Phase 3: Crawler Core ⏱️ Day 3–4

> **Goal:** Robust HTTP fetcher with retry, rate-limiting, and proper error handling.
> **Dependencies:** Phase 1 (errors), Phase 2 (config for timeouts/user-agent)

- [ ] Create `Crawler` struct holding `reqwest::Client` and `RateLimiter`
- [ ] Build `reqwest::Client` with:
  - `rustls-tls` (no OpenSSL dependency)
  - Connection pooling (default)
  - Custom `User-Agent`: `"searchxyz/0.1 (AI Research Agent)"`
  - `gzip` + `brotli` decompression
- [ ] Implement `fetch_url()` — basic version (GET, return body)
- [ ] Add configurable timeouts:
  | Timeout     | Default | Purpose                        |
  |-------------|---------|--------------------------------|
  | `connect`   | 5s      | TCP + TLS handshake            |
  | `read`      | 10s     | Time to first byte → last byte |
  | `total`     | 15s     | Overall request deadline       |
- [ ] Implement exponential backoff retry:
  - Crate: `backoff`
  - Max retries: 3
  - Initial interval: 500ms
  - Max interval: 10s
  - Retry on: 429, 500, 502, 503, 504, connection errors
- [ ] Add per-domain rate limiting with `Governor`:
  - Default: 2 requests/second/domain
  - Key: domain extracted from URL
- [ ] Handle redirects: max 5 hops (via `reqwest` redirect policy)
- [ ] Validate `Content-Type` header — reject non-`text/html` responses
- [ ] Add response size limit: **5MB** — reject larger responses
- [ ] Handle HTTP errors with specific, actionable messages:
  | Status | Message                                                |
  |--------|--------------------------------------------------------|
  | 403    | `"Access forbidden. Site may block automated access."` |
  | 404    | `"Page not found at {url}."`                           |
  | 429    | `"Rate limited. Retry after {n}s."`                    |
  | 500    | `"Server error. Site may be temporarily unavailable."` |
  | 503    | `"Service unavailable. Try again later."`              |
- [ ] Write unit tests for URL validation
- [ ] Write integration tests with mock HTTP server (`wiremock` crate):
  - Test: successful fetch
  - Test: retry on 500 then succeed
  - Test: 404 returns proper error
  - Test: timeout handling
  - Test: rate limiting queues requests

**Exit criteria:** Crawler fetches real pages, retries on transient failures, respects rate limits, never panics.

---

## Phase 4: Content Extraction ⏱️ Day 4–5

> **Goal:** Turn raw HTML into clean, readable Markdown.
> **Dependencies:** Phase 3 (crawler provides raw HTML)

- [ ] Implement HTML noise removal — strip these tags entirely:
  - `<script>`, `<style>`, `<nav>`, `<footer>`, `<header>`, `<aside>`, `<noscript>`
  - Cookie banners, ad containers (common class patterns)
- [ ] Implement main content extraction with priority chain:
  ```
  <article> → <main> → <div role="main"> → <body>
  ```
- [ ] Implement HTML → Markdown conversion using `scraper` + custom logic:
  | HTML                 | Markdown                         |
  |----------------------|----------------------------------|
  | `<h1>`–`<h6>`       | `#`–`######`                     |
  | `<p>`                | Paragraph with blank line        |
  | `<a href="...">`    | `[text](url)`                    |
  | `<strong>` / `<b>`  | `**bold**`                       |
  | `<em>` / `<i>`      | `*italic*`                       |
  | `<code>`             | `` `inline` ``                   |
  | `<pre><code>`        | Fenced code block                |
  | `<ul>` / `<ol>`     | `- item` / `1. item`            |
  | `<blockquote>`       | `> quote`                        |
  | `<img>`              | `![alt](src)`                    |
  | `<table>`            | Markdown table                   |
- [ ] Implement metadata extraction:
  - **Title:** `<title>` → `<meta og:title>` → first `<h1>`
  - **Description:** `<meta name="description">` → `<meta og:description>`
  - **Author:** `<meta name="author">` → `<meta article:author>`
  - **Date:** `<meta article:published_time>` → `<time datetime="...">`
- [ ] Build fallback chain:
  ```
  Structured extraction (article/main)
    → Simple extraction (body, noise stripped)
      → Raw text (innerText of body)
  ```
- [ ] Handle edge cases:
  - Empty pages (content < 50 chars) → return `EmptyContent` error
  - Login walls (detect common patterns) → return warning in content
  - Error pages (detect "404", "not found" in content) → return warning
- [ ] Content length limiting — configurable max (default: **50,000 chars**)
- [ ] Write unit tests with sample HTML files (store in `tests/fixtures/`)
- [ ] Write tests for edge cases: empty page, minimal page, complex multi-section page

**Exit criteria:** Given any HTML page, produces clean Markdown with metadata — or a clear error explaining why it couldn't.

---

## Phase 5: Search Backends ⏱️ Day 5–7

> **Goal:** Pluggable search backends with automatic failover.
> **Dependencies:** Phase 1 (errors), Phase 3 (crawler for HTTP)

- [ ] Define `SearchBackend` trait:
  ```rust
  #[async_trait]
  pub trait SearchBackend: Send + Sync {
      fn name(&self) -> &str;
      async fn search(&self, query: &str, max_results: usize)
          -> Result<Vec<SearchResult>, SearchXyzError>;
  }
  ```
- [ ] Define `SearchResult` struct:
  ```rust
  pub struct SearchResult {
      pub title: String,
      pub url: String,
      pub snippet: String,
      pub source_engine: String,
      pub rank: usize,
  }
  ```
- [ ] Define `SearchQuery` struct:
  ```rust
  pub struct SearchQuery {
      pub query: String,
      pub max_results: usize,
      pub backends: Option<Vec<String>>,  // None = use default order
  }
  ```
- [ ] **Implement DuckDuckGo backend:**
  - POST to `https://lite.duckduckgo.com/lite/` with form data (`q=...`)
  - Parse HTML response with `scraper` crate
  - Extract result title, URL, snippet from result table
  - Handle: empty results, CAPTCHA challenges, connection errors
- [ ] **Implement Brave Search backend:**
  - GET `https://api.search.brave.com/res/v1/web/search?q=...`
  - Headers: `Accept: application/json`, `X-Subscription-Token: {api_key}`
  - Parse JSON response → extract `web.results[]`
  - Handle:
    - 401 → `"Invalid Brave API key. Check SEARCHXYZ_BRAVE_API_KEY."`
    - 429 → `"Brave rate limit exceeded. Try again in {n}s."`
    - 500 → `"Brave API server error."`
- [ ] **Build `SearchDispatcher`:**
  - Maintains ordered list of backends
  - Try primary backend first
  - On failure: log warning, try next backend
  - Return first successful result set
  - If **all** fail → return `AllBackendsExhausted` with list of errors
- [ ] Write unit tests for DDG HTML parsing (sample HTML in `tests/fixtures/`)
- [ ] Write unit tests for Brave JSON parsing (sample JSON in `tests/fixtures/`)
- [ ] Write integration tests for dispatcher failover logic

**Exit criteria:** `search("rust async programming", 5)` returns results from DDG or Brave. If DDG fails, Brave is tried. If both fail, error lists all failures.

---

## Phase 6: Local Index ⏱️ Day 7–8

> **Goal:** Full-text search over previously crawled content using Tantivy.
> **Dependencies:** Phase 1 (errors), Phase 2 (config for data dir)

- [ ] Define Tantivy schema:
  | Field        | Type   | Properties       | Purpose                     |
  |--------------|--------|------------------|-----------------------------|
  | `url`        | TEXT   | STORED           | Original page URL           |
  | `title`      | TEXT   | STORED           | Page title                  |
  | `content`    | TEXT   | (indexed only)   | Full page content for search|
  | `source`     | TEXT   | STORED           | How it was indexed          |
  | `indexed_at` | DATE   | STORED           | Timestamp of indexing       |
- [ ] Implement index creation with `MmapDirectory` at `~/.searchxyz/index/`
- [ ] Implement index opening — create directory + index if not exists
- [ ] Implement `add_document()` with commit batching:
  - Buffer documents, commit every N adds or on explicit flush
  - Dedup by URL (delete old, insert new)
- [ ] Implement `search()` with BM25 ranking:
  - Query parser across `title` + `content` fields
  - Return top N results sorted by relevance
- [ ] Implement snippet generation — extract matching text fragments
- [ ] Implement `delete_by_url()` — remove document by URL term
- [ ] Implement `list_sources()` — return all indexed URLs with metadata
- [ ] Handle concurrent access:
  - `Arc<Index>` shared across tool handlers
  - Single `IndexWriter` (Tantivy requirement)
  - `Arc<Mutex<IndexWriter>>` for write serialization
- [ ] Configurable heap size for `IndexWriter` (default: **50MB**)
- [ ] Write unit tests for indexing and search (add doc → search → find it)
- [ ] Write tests for concurrent access (spawn multiple search tasks)

**Exit criteria:** Index content → search by keyword → get ranked results with snippets. Survives concurrent reads + writes.

---

## Phase 7: Cache Layer ⏱️ Day 8–9

> **Goal:** Avoid re-crawling recently fetched pages.
> **Dependencies:** Phase 1 (errors), Phase 2 (config for cache settings)

- [ ] Define `CacheEntry` struct:
  ```rust
  pub struct CacheEntry {
      pub url: String,
      pub content_markdown: String,
      pub metadata: PageMetadata,
      pub fetched_at: DateTime<Utc>,
      pub ttl: Duration,
  }
  ```
- [ ] Implement `PageCache` wrapping `lru::LruCache<String, CacheEntry>`
- [ ] Implement `get()`:
  - Look up by normalized URL
  - Check `fetched_at + ttl > now` — if expired, return `None` (and evict)
  - If valid, return `Some(CacheEntry)`
- [ ] Implement `put()`:
  - Insert/update entry
  - Respect `max_entries` — LRU eviction handles overflow
- [ ] Thread-safe wrapper: `Arc<Mutex<LruCache<String, CacheEntry>>>`
- [ ] Cache key = URL string, normalized (lowercase scheme + host, strip fragment)
- [ ] Configurable:
  | Setting        | Default | Purpose                          |
  |----------------|---------|----------------------------------|
  | `max_entries`  | 1000    | Max cached pages in memory       |
  | `ttl_minutes`  | 60      | Time before cache entry expires  |
- [ ] Write unit tests for cache hit/miss
- [ ] Write tests for TTL expiration (insert → sleep → get returns None)
- [ ] Write tests for LRU eviction (fill cache → insert one more → oldest gone)

**Exit criteria:** Crawl page → cache → re-request → served from cache. Expired entries are evicted. Memory bounded.

---

## Phase 8: MCP Tools ⏱️ Day 9–11

> **Goal:** All 5 core tools registered, working, with excellent descriptions.
> **Dependencies:** Phase 3 (crawler), Phase 4 (extractor), Phase 5 (search), Phase 6 (index), Phase 7 (cache)

- [ ] Set up MCP tool registration with `rmcp` `ServerHandler` trait
- [ ] **Implement `search_web` tool:**
  - **Input:**
    - `query` (string, **required**) — what to search for
    - `max_results` (integer, optional, default: 10) — number of results
    - `backend` (string, optional) — force specific backend ("duckduckgo" | "brave")
  - **Output:** Array of `{ title, url, snippet }`
  - **Errors:** `AllBackendsExhausted`, `SearchFailed`
- [ ] **Implement `read_url` tool:**
  - **Input:**
    - `url` (string, **required**) — URL to fetch and extract
  - **Output:** `{ url, title, content_markdown, word_count }`
  - **Errors:** `CrawlFailed`, `ExtractionFailed`, `Timeout`
  - Cache integration: check cache first, store result after fetch
- [ ] **Implement `search_and_read` tool:**
  - **Input:**
    - `query` (string, **required**) — what to search for
    - `max_results` (integer, optional, default: 5) — how many pages to read
  - **Output:** Array of `{ url, title, content_markdown, snippet }`
  - **Behavior:** Search → crawl top N → extract → return
  - **Partial results** on partial failures (don't fail the whole call)
- [ ] **Implement `recall` tool:**
  - **Input:**
    - `query` (string, **required**) — what to search for in local index
    - `max_results` (integer, optional, default: 10) — number of results
  - **Output:** Array of `{ url, title, snippet, indexed_at }`
  - Searches the Tantivy local index only
- [ ] **Implement `index_content` tool:**
  - **Input:**
    - `url` (string, **required**) — source URL or identifier
    - `title` (string, optional) — content title
    - `content` (string, **required**) — content to index
  - **Output:** `{ indexed: true, id: "<uuid>" }`
  - Stores content in Tantivy for later `recall`
- [ ] Write **EXCELLENT** tool descriptions:
  - These are the **LLM interface** — the description is how Claude decides when to use each tool
  - Be specific about what each tool does, when to use it, and what it returns
  - Include examples in the description
- [ ] Input validation for all tools:
  - Non-empty `query` (trim whitespace)
  - Valid URL format for `url` parameter
  - `max_results` in range `1..=50`
- [ ] Error handling → actionable MCP error messages
- [ ] Test each tool manually with **MCP Inspector**

**Exit criteria:** All 5 tools appear in MCP tool listing. Each tool handles valid input, invalid input, and backend failures gracefully.

---

## Phase 9: Pipeline Orchestration ⏱️ Day 11–12

> **Goal:** `search_and_read` does parallel crawl + extract + index in one call.
> **Dependencies:** Phase 8 (all tools must work individually first)

- [ ] Implement `search_and_read` pipeline in `src/pipeline/mod.rs`
- [ ] Pipeline steps:
  1. **Search** → get top N result URLs
  2. **Parallel crawl** with `tokio::JoinSet` (max 5 concurrent)
  3. **Extract** each page through the fallback chain
  4. **Auto-index** all successful results into Tantivy
  5. **Format** results with source attribution
- [ ] Handle partial failures:
  - If 3/5 pages fail, still return the 2 that succeeded
  - Include failure reasons in a `warnings` field
- [ ] Concurrency control:
  - `tokio::Semaphore` to limit parallel crawls
  - Respect per-domain rate limits (don't hammer one domain)
- [ ] Format results with source attribution:
  ```
  Source: {url}
  Title: {title}
  Content: {markdown_content}
  ```
- [ ] Write integration tests:
  - Test: all pages succeed
  - Test: some pages fail → partial results returned
  - Test: all pages fail → error with details

**Exit criteria:** `search_and_read("tokio async runtime", 3)` returns 3 fully extracted pages in < 10s, auto-indexed for later `recall`.

---

## Phase 10: Polish & Release ⏱️ Day 12–14

> **Goal:** Production-quality release with docs, tests, and benchmarks.
> **Dependencies:** All previous phases

### Testing

- [x] End-to-end testing with **20+ real websites** across categories:
  - [x] News (CNN, BBC, Reuters)
  - [x] Technical docs (docs.rs, MDN, Rust Book)
  - [x] Wikipedia articles
  - [x] Blog posts (Medium, Dev.to)
  - [x] GitHub READMEs
- [x] Edge case testing:
  - [x] Timeout (very slow server)
  - [x] 404 (dead links)
  - [x] Empty pages (placeholder sites)
  - [x] Huge pages (> 5MB)
  - [x] Non-HTML (PDF, JSON endpoints)
  - [x] Non-English content (UTF-8 handling)

### Performance

- [x] Memory profiling with `cargo instruments` or `heaptrack`
- [x] Verify memory targets:
  | Scenario        | Target       |
  |-----------------|--------------|
  | Idle            | < 50MB       |
  | Under load      | < 100MB      |
  | After 100 calls | < 100MB (no leaks) |
- [x] Performance benchmarking:
  | Operation           | Target   |
  |---------------------|----------|
  | `recall` query      | < 50ms   |
  | Single crawl+extract| < 2s     |
  | `search_and_read`   | < 5s     |

### Quality

- [x] Review **ALL** error messages — are they actionable for an AI agent?
- [x] `cargo clippy -- -D warnings` → zero warnings
- [x] `cargo fmt --check` → all formatted
- [x] `cargo test` → all passing
- [x] `cargo audit` → no known vulnerabilities

### Documentation

- [x] Write `README.md`:
  - [x] What is searchxyz and why it exists
  - [x] Installation (cargo install, binary download)
  - [x] Configuration (`config.toml` reference)
  - [x] MCP tool reference with examples
  - [x] Claude Desktop config example
  - [x] Performance characteristics
  - [x] Contributing guide
- [x] Create Claude Desktop MCP config example:
  ```json
  {
    "mcpServers": {
      "searchxyz": {
        "command": "searchxyz",
        "args": [],
        "env": {
          "SEARCHXYZ_BRAVE_API_KEY": "your-key-here"
        }
      }
    }
  }
  ```

### Release

- [x] Build release binaries: `cargo build --release`
- [x] Verify binary size < 20MB
- [x] Test on Linux (x86_64)
- [x] Test on macOS (aarch64)
- [x] Tag `v0.1.0` in git
- [x] Write release notes

**Exit criteria:** Binary ships. Docs complete. Tests pass. Performance targets met.

---

## Phase 11: v1.1 Features ⏱️ Week 3–4

> **Goal:** Enhanced capabilities based on real-world usage feedback.
> **Dependencies:** v0.1.0 shipped

- [x] **Deep research mode:**
  - [x] Multi-query expansion (rephrase query 3 ways)
  - [x] Cross-source synthesis (combine results from multiple pages)
  - [x] Produce structured research report
- [x] **PDF text extraction** — detect PDF content-type, extract text
- [x] **Configurable search backends** via `config.toml` hot reload
- [x] **Cache persistence to disk:**
  - [x] Serialize cache to disk on shutdown (`serde` + `bincode`)
  - [x] Load from disk on startup
  - [x] Graceful degradation if cache file corrupted
- [x] **`list_sources` tool** — list all URLs in the local index with metadata
- [x] **`deep_research` tool** — multi-step research with synthesis
- [x] **Site mapping** — discover all URLs on a domain (crawl sitemap.xml, follow links)

---

## Phase 12: v2.0 Features ⏱️ Month 2+

> **Goal:** Advanced AI-native features.
> **Dependencies:** v1.1 stable

- [x] **Vector embeddings + semantic search:**
  - [x] `fastembed-rs` or `candle` for local embedding generation
  - [x] Store embeddings alongside Tantivy text index
  - [x] Hybrid search: BM25 + cosine similarity
- [x] **Knowledge graph:**
  - [x] Extract entities and relationships from indexed content
  - [x] Store as directed graph
  - [x] Query: "What topics are related to X?"
- [x] **Streamable HTTP transport:**
  - [x] Support remote deployment (not just stdio)
  - [x] SSE-based streaming for long operations
- [x] **YouTube transcript extraction** — fetch and index video transcripts
- [x] **GitHub repository reading** — clone, parse, and index codebases
- [x] **Multi-agent communication protocol** — agents share research via index


---

## Phase 13: v2.1 Features ⏱️ Day 15–18

> **Goal:** Enhance indexing granularity, security, provider support, and database maintenance.
> **Dependencies:** Phase 12 (v2.0 stable)

- [x] **Markdown-Aware Document Chunking**:
  - [x] Implement section header-aware splitting logic
  - [x] Retain prefix context headers for semantic relevance
  - [x] Implement paragraph sliding window fallback
- [x] **Database Maintenance Tools**:
  - [x] Implement index deletion for source URLs/prefixes (`delete_source`)
  - [x] Prune Knowledge Graph nodes concurrently
  - [x] Implement full index reset (`clear_index`)
- [x] **Custom Embedding Models Integration**:
  - [x] Support OpenAI embedding models
  - [x] Support Gemini embedding models
  - [x] Support Cohere embedding models
  - [x] Fallback to local fastembed ONNX models
- [x] **SSE Authentication Middleware**:
  - [x] Secure HTTP/SSE routes with pre-shared Bearer tokens
  - [x] Read configuration via `SEARCHXYZ_AUTH_TOKEN` case-insensitively
- [x] **Incremental Git Codebase Ingestion**:
  - [x] Cache Git clones inside persistent `~/.searchxyz/repos` directory
  - [x] Sync changes via `git fetch` + `git reset --hard FETCH_HEAD`
  - [x] Detect modified/added/deleted files using `git diff --name-status`
  - [x] Perform delta updates on Tantivy index and Knowledge Graph

---

## Quality Checklist — Apply Every Phase ✅

> Run through this checklist before marking **any** phase as complete.

- [ ] All functions return `Result<T, SearchXyzError>`
- [ ] **ZERO** `unwrap()` or `expect()` in production code paths
- [ ] All errors have actionable messages for AI agents
- [ ] Fallback strategies implemented and tested
- [ ] Unit tests written and passing
- [ ] Memory usage checked (no leaks)
- [ ] **No** `println!` anywhere — use `tracing` macros (`tracing::info!`, `tracing::error!`, etc.)
- [ ] All public items documented with `///` doc comments
- [ ] No clippy warnings (`cargo clippy -- -D warnings`)
- [ ] `cargo fmt` applied

---

## Dependency Graph

```
Phase 0 (Bootstrap)
  └─→ Phase 1 (Errors)
       ├─→ Phase 2 (Config)
       ├─→ Phase 3 (Crawler)
       │    └─→ Phase 4 (Extractor)
       ├─→ Phase 5 (Search Backends)
       └─→ Phase 6 (Local Index)
            └─→ Phase 7 (Cache)
                 └─→ Phase 8 (MCP Tools)
                      └─→ Phase 9 (Pipeline)
                           └─→ Phase 10 (Polish & Release)
                                └─→ Phase 11 (v1.1)
                                     └─→ Phase 12 (v2.0)
```

> **Parallelizable:** Phases 3, 5, and 6 can run in parallel after Phase 1 is complete.
> **Critical path:** 0 → 1 → 3 → 4 → 8 → 9 → 10

---

## Definition of Done — MVP (v0.1.0)

> All of these must be true before tagging `v0.1.0`:

- [ ] `cargo build --release` produces a single binary **< 20MB**
- [ ] Binary starts MCP server in **< 100ms**
- [ ] All **5 core tools** work correctly:
  1. `search_web` — returns web search results
  2. `read_url` — fetches and extracts any page
  3. `search_and_read` — search + read in one call
  4. `recall` — searches local index
  5. `index_content` — adds content to local index
- [ ] Tested with **Claude Desktop** — all tools callable
- [ ] Tested with **MCP Inspector** — all tools visible and functional
- [ ] Memory < **100MB** under normal research session
- [ ] **Zero panics** in 1000 consecutive tool calls
- [ ] `README.md` with complete documentation
- [ ] `config.toml.example` with all options documented
- [ ] `v0.1.0` tagged in git

---

## Timeline Summary

| Phase | Name               | Duration  | Parallel? |
|-------|--------------------|-----------|-----------|
| 0     | Bootstrap          | Day 1     | —         |
| 1     | Error Foundation   | Day 1–2   | —         |
| 2     | Configuration      | Day 2     | ✅ with 3 |
| 3     | Crawler Core       | Day 3–4   | ✅ with 5,6 |
| 4     | Content Extraction | Day 4–5   | After 3   |
| 5     | Search Backends    | Day 5–7   | ✅ with 3,6 |
| 6     | Local Index        | Day 7–8   | ✅ with 3,5 |
| 7     | Cache Layer        | Day 8–9   | After 6   |
| 8     | MCP Tools          | Day 9–11  | After 3–7 |
| 9     | Pipeline           | Day 11–12 | After 8   |
| 10    | Polish & Release   | Day 12–14 | After 9   |
| 11    | v1.1 Features      | Week 3–4  | After 10  |
| 12    | v2.0 Features      | Month 2+  | After 11  |

**Total MVP estimate: ~14 working days**
