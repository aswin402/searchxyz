# Changelog

All notable changes to the **searchxyz** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.0.8] - 2026-06-21

### Added
- **Local Vector Embeddings**: Integrated the `fastembed` crate to compute 384-dimension `BGESmallENV15` embeddings locally.
- **Embedded Document Storage**: Added `f_embedding` as a stored `BYTES` field in the local Tantivy database schema to preserve document vectors.
- **Hybrid Semantic Search**: Implemented `search_semantic` to perform cosine similarity ranking (dot-product) over all cached documents.
- **Recall Semantic Toggle**: Updated `RecallRequest` and the `recall` tool interface to expose a `semantic` boolean toggle (defaulting to `true` to execute semantic vector search).
- **Static OpenSSL Vendoring Resolver**: Configured Cargo `build-dependencies` with `openssl` and `openssl-sys` vendored flags to solve host environment dependency compilation failures.

## [0.0.7] - 2026-06-21

### Added
- **PDF Text Extraction**: Integrated the `pdf-extract` crate to natively extract text from digital PDF documents (`application/pdf`) downloaded during crawling.
- **Bytes-oriented Response Handling**: Refactored the HTTP crawler body retrieval from string-only text to binary bytes.
- **Page Extraction Pipeline Bypass**: Supports directly mapping extracted PDF plain text to `ExtractedContent` schemas, bypassing HTML noise stripping and conversion.
- **Dynamic PDF Tests**: Added unit tests constructing a valid PDF programmatically using `lopdf` and verifying that the text is correctly extracted.

---

## [0.0.6] - 2026-06-21

### Added
- **Persistent LRU Crawl Cache**: Implemented serializable LRU cache serialization and deserialization using `serde` and `serde_json`.
- **Startup Restore & Shutdown Save**: Restores crawled pages from disk on startup and automatically saves non-expired entries on graceful shutdown.
- **Cache Storage Path Configuration**: Added `[cache].path` setting to `searchxyz.toml` and `SEARCHXYZ_CACHE_PATH` environment variable override support.

---

## [0.0.5] - 2026-06-21

### Added
- **Rotating Proxy & SOCKS5 Support**: Pool multiple reqwest clients each bound to a specific proxy and randomly rotate them per request attempt.
- **SOCKS5 Protocol Support**: Enabled the `socks` feature flag for the `reqwest` HTTP client, enabling native SOCKS5/HTTP/HTTPS proxy URLs.
- **Headless Browser Proxy Support**: Randomly selects and configures a proxy via the `--proxy-server` command line argument when spawning Chromium/Chrome through `chromiumoxide`.
- **Environment Overrides**: Added support for overriding proxy configuration via `SEARCHXYZ_PROXY_ENABLED` (boolean) and `SEARCHXYZ_PROXY_URLS` (comma-separated list of proxy URLs).

---

## [0.0.4] - 2026-06-21

### Added
- **Headless JS Rendering**: Integrated optional JavaScript rendering using the `chromiumoxide` crate, controlled via the `js-rendering` Cargo feature flag.
- **Configurable Browser Session**: Added `[headless]` section in configuration parsing (`enabled`, `chrome_path`, `wait_after_load_ms`, `viewport`) and environment overrides.
- **Tool Schema parameters**: Updated `read_url` and `search_and_read` tools schema to accept `render_js` parameters, allowing AI agents to dynamically request headless browser rendering for client-side dynamic or JS-heavy websites.
- **Stealth Header injection**: Programmed custom HTTP headers rotation to be applied to the browser's CDP context prior to site navigation to protect against detection.

---

## [0.0.3] - 2026-06-21

### Added
- **Evasion & Header Randomization**: Thread-safe dynamic desktop/mobile User-Agent and Accept header rotation (inspired by Crawlee) to bypass anti-scraping defenses on public web endpoints.
- **Scoped Recursive Spidering**: Recursive link queueing using async Tokio task joins, executing parallel crawling up to a configurable max depth within the same starting domain scope (inspired by Katana).
- **Native Search Scrapers**: Added native, keyless scraper backends for Google and Bing search results pages directly inside the Rust engine (inspired by Websurfx and searxng-mcp).
- **Expanded Search Defaults**: Google and Bing are now registered in the default search dispatcher backends path, allowing query fallbacks without requiring API keys.

---

## [0.0.2] - 2026-06-21

### Added
- **SearXNG Backend**: Implemented a native SearXng search backend enabling self-hostable, metasearch queries aggregating results from Google, Bing, Wikipedia, etc., without requiring credentials.
- **SearXngConfig Options**: Integrated local configuration options for custom SearXNG instance URLs and target engine selections, including `SEARCHXYZ_SEARXNG_URL` environment overrides.

### Changed
- **DuckDuckGo Form Method**: Changed the DuckDuckGo Lite query requests from GET to form-urlencoded POST requests, aligning with the target portal's protocol and minimizing captcha detection.

---

## [0.0.1] - 2026-06-21

### Added
- **Initial MVP Release**: Scaffolded the project utilizing the `onpkg` CLI template.
- **Search Engine Dispatcher**: Supports DuckDuckGo Lite scraping and Brave Search API interfaces with automatic rate-limit backoffs.
- **Boilerplate Reduction Engine**: CSS selector-based stripping parser producing clean Markdown.
- **Parallel Crawling Pipeline**: Asynchronous concurrent crawls via `tokio::task::JoinSet`.
- **Tantivy Search Index**: Local search recall database with high-performance Tantivy full-text index queries.
- **LRU Cache**: Cache layer with TTL validation to prevent duplicative crawls.
- **MCP Stdio Server**: Fully integrated `rmcp` v1.7.0 macros and stdio server handler with tool JSON schemas.
