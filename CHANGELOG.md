# Changelog

All notable changes to the **searchxyz** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
