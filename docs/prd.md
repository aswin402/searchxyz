# searchxyz — Product Requirements Document

| Field   | Value                |
|---------|----------------------|
| Version | 0.1.0                |
| Status  | Draft                |
| Author  | searchxyz team       |
| Date    | 2026-06-21           |

---

## 1. Executive Summary

**searchxyz** is a high-performance MCP (Model Context Protocol) server written in Rust that provides AI agents with a complete research pipeline through a single, cohesive toolset. Instead of requiring agents to orchestrate multiple fragile services — a search API here, a scraper there, a separate indexing engine — searchxyz unifies **search, crawl, extract, index, and recall** into well-designed MCP tools exposed over standard transports.

Built on Rust for maximum throughput, minimal memory footprint, and zero-dependency deployment, searchxyz targets the growing ecosystem of AI agents (Claude, GPT, local LLMs) and the MCP hosts that orchestrate them (Claude Desktop, Cursor, Windsurf, VS Code + Cline). The server ships as a single static binary with no Python or Node.js runtime dependencies, starts in under 100 ms, and respects rate limits, `robots.txt`, and user privacy by default.

The MVP (v0.1.0) delivers six core tools — `search_web`, `read_url`, `index_content`, `recall`, `list_sources`, and structured error handling — with a clear roadmap toward combined pipeline tools, deep research mode, semantic search, and knowledge graphs.

---

## 2. Problem Statement

AI agents today face a fragmented research landscape:

1. **Tool sprawl** — Agents need separate MCP servers or API wrappers for search, crawling, extraction, and memory, leading to complex configurations and brittle integrations.
2. **Performance bottlenecks** — Python/Node-based tools consume excessive memory, start slowly, and struggle under concurrent agent workloads.
3. **Privacy and control** — Cloud-only search APIs leak queries and content to third parties; many require paid subscriptions for basic functionality.
4. **No persistent memory** — Most search tools are stateless. Agents cannot recall previously researched content without re-fetching and re-processing.
5. **Inconsistent output quality** — Raw HTML dumps are noisy and token-expensive. Agents waste context window capacity on boilerplate, ads, and navigation elements.

searchxyz solves all five problems in a single, fast, privacy-respecting binary.

---

## 3. Target Users

| # | User Segment | Description |
|---|-------------|-------------|
| 1 | **AI agent developers** | Engineers building agents on Claude, GPT, Gemini, or local LLMs who need reliable, fast research tools accessible via MCP. |
| 2 | **MCP ecosystem users** | Users of Claude Desktop, Cursor, Windsurf, VS Code + Cline, or any MCP-compatible host seeking a drop-in research server. |
| 3 | **RAG pipeline builders** | Developers constructing Retrieval-Augmented Generation systems who need a local indexing and recall layer without external vector DBs. |
| 4 | **Privacy-conscious researchers** | Users who want local-first, fast search and content extraction without query logging or mandatory cloud dependencies. |

---

## 4. User Stories

### US-01: Web Search
> **As an** AI agent developer,
> **I want to** search the web for a query and receive structured, ranked results,
> **so that** my agent can find relevant sources without leaving the MCP protocol.

### US-02: Page Reading
> **As an** AI agent,
> **I want to** fetch a URL and receive clean Markdown content (not raw HTML),
> **so that** I can consume page content efficiently within my context window.

### US-03: Search and Read
> **As a** RAG pipeline builder,
> **I want to** issue a single tool call that searches, selects the best results, and returns extracted content,
> **so that** I reduce round-trips and latency in my retrieval pipeline.

### US-04: Content Indexing
> **As a** developer building a research agent,
> **I want to** index crawled content locally with full-text search,
> **so that** my agent can recall previously researched material without re-fetching.

### US-05: Content Recall
> **As an** AI agent,
> **I want to** query my local index with natural-language terms and get relevant snippets,
> **so that** I can ground my responses in previously gathered research.

### US-06: Source Management
> **As a** researcher,
> **I want to** list all indexed sources with metadata (title, URL, timestamp),
> **so that** I can audit what my agent has researched and manage my knowledge base.

### US-07: Rate Limit Compliance
> **As a** privacy-conscious user,
> **I want** the server to respect `robots.txt` and enforce rate limits automatically,
> **so that** I don't get blocked or violate site policies.

### US-08: Cross-Platform Deployment
> **As a** developer on macOS,
> **I want to** run searchxyz as a single binary without installing Python, Node, or Docker,
> **so that** setup is instant and my system stays clean.

### US-09: Deep Research
> **As a** researcher investigating a complex topic,
> **I want to** trigger a deep research mode that iteratively searches, reads, and synthesizes across multiple sources,
> **so that** I get a comprehensive analysis without manual orchestration.

### US-10: Error Resilience
> **As an** AI agent developer,
> **I want** the server to return structured errors with actionable messages instead of crashing,
> **so that** my agent can handle failures gracefully and retry when appropriate.

---

## 5. Core Requirements

### P0 — Must Have (MVP, v0.1.0)

| ID | Requirement | Description |
|----|------------|-------------|
| P0-01 | **Multi-source web search** | Search via DuckDuckGo (free, no API key) as default backend. Architecture supports pluggable backends (Brave, SearxNG, Google Custom Search) via a trait-based design. |
| P0-02 | **Web page crawling** | Fetch arbitrary URLs with configurable timeouts (default 30s), automatic retries (3x exponential backoff), and per-domain rate limiting. |
| P0-03 | **Content extraction** | Extract main content from HTML using readability-inspired algorithms. Strip navigation, ads, footers, and boilerplate. Preserve semantic structure (headings, lists, code blocks, tables). |
| P0-04 | **Markdown conversion** | Convert extracted HTML to clean, well-formatted Markdown suitable for LLM consumption. Preserve links, images (as references), and code blocks. |
| P0-05 | **Local full-text index** | Index extracted content using Tantivy for fast, local full-text search. Support field-based queries (title, body, URL, domain). Persist index to disk across server restarts. |
| P0-06 | **MCP protocol compliance** | Implement MCP server using the `rmcp` SDK. Support stdio transport. Expose tools with proper JSON Schema input/output definitions. Handle `initialize`, `tools/list`, and `tools/call` methods. |
| P0-07 | **Structured error handling** | Use `thiserror` for typed error variants. Return MCP-compliant error responses with error codes, human-readable messages, and retry hints. Never panic on user input. |
| P0-08 | **Rate limiting & robots.txt** | Enforce per-domain rate limits (configurable, default 1 req/s). Parse and respect `robots.txt` directives. User-agent identification. |

### P1 — Should Have (v1.1)

| ID | Requirement | Description |
|----|------------|-------------|
| P1-01 | **`search_and_read` pipeline** | Combined tool: search → rank → fetch top-N → extract → return structured content bundle. Reduces agent round-trips from 3+ to 1. |
| P1-02 | **Deep research mode** | Iterative research: search → read → identify gaps → search again → synthesize. Configurable depth (default 3 iterations) and breadth (default 5 sources per iteration). |
| P1-03 | **PDF extraction** | Extract text content from PDF URLs. Support for text-based PDFs via `pdf-extract` or similar. Return content as Markdown. |
| P1-04 | **TOML configuration** | Configurable settings via `searchxyz.toml`: search backends, rate limits, index path, cache size, timeouts, user-agent string. Sensible defaults for zero-config startup. |
| P1-05 | **LRU page cache** | In-memory LRU cache for recently crawled pages. Configurable max size (default 100 entries / 50 MB). Cache hit avoids network fetch. TTL-based expiration (default 1 hour). |

### P2 — Nice to Have (v2.0)

| ID | Requirement | Description |
|----|------------|-------------|
| P2-01 | **Semantic/vector search** | Embed indexed content using local embedding models (e.g., `fastembed-rs`). Support cosine similarity search for semantic recall alongside keyword-based full-text search. |
| P2-02 | **Knowledge graph** | Build and query a local knowledge graph from extracted entities and relationships. Support graph-based reasoning for multi-hop queries. |
| P2-03 | **HTTP transport** | SSE-based HTTP transport for MCP, enabling remote connections and multi-client scenarios. |
| P2-04 | **Site mapping** | Crawl and index entire sites or sitemaps. Recursive crawl with depth limits, URL filtering, and deduplication. |
| P2-05 | **YouTube transcripts** | Extract and index YouTube video transcripts from video URLs. Return timestamped transcript as Markdown. |
| P2-06 | **GitHub repo reading** | Read and index GitHub repository files, READMEs, and documentation via GitHub API or raw URL fetching. |

---

## 6. MCP Tools Specification

| Tool | Description | Input Parameters | Output | Priority |
|------|-------------|-----------------|--------|----------|
| `search_web` | Search the web using configured backend(s). Returns ranked result list with titles, URLs, and snippets. | `query: string` (required), `num_results: int` (optional, default 10), `backend: string` (optional) | `{ results: [{ title, url, snippet, source }] }` | P0 |
| `read_url` | Fetch a URL, extract main content, convert to Markdown. Returns clean, LLM-ready content. | `url: string` (required), `extract_links: bool` (optional, default false), `include_metadata: bool` (optional, default true) | `{ title, url, content_markdown, word_count, links?: [], metadata?: {} }` | P0 |
| `search_and_read` | Combined pipeline: search → select top results → fetch and extract all → return bundled content. | `query: string` (required), `num_results: int` (optional, default 3), `max_content_length: int` (optional) | `{ query, results: [{ title, url, content_markdown, relevance_score }] }` | P1 |
| `recall` | Query the local full-text index. Returns matching snippets from previously indexed content. | `query: string` (required), `max_results: int` (optional, default 10), `domain_filter: string` (optional) | `{ results: [{ title, url, snippet, score, indexed_at }] }` | P0 |
| `index_content` | Index content into the local Tantivy index for later recall. Auto-indexes on `read_url` if enabled. | `url: string` (required), `title: string` (required), `content: string` (required), `metadata: object` (optional) | `{ indexed: bool, id: string, word_count: int }` | P0 |
| `list_sources` | List all indexed sources with metadata. Supports filtering and pagination. | `domain_filter: string` (optional), `limit: int` (optional, default 50), `offset: int` (optional) | `{ sources: [{ title, url, indexed_at, word_count }], total_count }` | P0 |
| `deep_research` | Iterative multi-step research pipeline. Searches, reads, identifies gaps, iterates, and synthesizes a final report. | `query: string` (required), `depth: int` (optional, default 3), `breadth: int` (optional, default 5) | `{ query, iterations: int, sources_consulted: int, synthesis_markdown: string, sources: [] }` | P1 |
| `site_map` | Discover all internal page URLs of a domain using sitemap.xml and/or fast recursive link crawling. | `url: string` (required), `use_sitemap: bool` (optional, default true), `crawl_links: bool` (optional, default true), `max_links: int` (optional, default 100) | Markdown listing of discovered page URLs. | P2 |


---

## 7. Non-Functional Requirements

### 7.1 Performance

| Metric | Target |
|--------|--------|
| Server cold start | < 100 ms |
| `search_web` latency (DuckDuckGo) | < 2 s (p95), network-bound |
| `read_url` latency (cached) | < 50 ms |
| `read_url` latency (uncached) | < 5 s (p95), network-bound |
| `recall` query latency | < 20 ms for 100k documents |
| Idle memory usage | < 30 MB RSS |
| Active memory usage | < 100 MB RSS under typical workload |
| Binary size (release, stripped) | < 25 MB |
| Index size | ~1 KB per document (metadata + content) |

### 7.2 Reliability

- **Zero panics** on any user-provided input. All public APIs use `Result<T, E>`.
- **Graceful degradation** — if a search backend is unreachable, return a structured error, don't crash.
- **Automatic retries** — transient network failures trigger exponential backoff (3 retries, 1s/2s/4s).
- **Timeout enforcement** — all network operations have configurable timeouts. No indefinite hangs.
- **Clean shutdown** — flush index to disk on SIGTERM/SIGINT. No data corruption on forced exit.

### 7.3 Compatibility

| Platform | Support Level |
|----------|--------------|
| Linux (x86_64, aarch64) | Tier 1 — fully tested |
| macOS (x86_64, Apple Silicon) | Tier 1 — fully tested |
| Windows (x86_64) | Tier 2 — compiled, basic testing |

- **MCP protocol** — full compliance with MCP specification via `rmcp` SDK.
- **Rust edition** — 2021, MSRV 1.75.0.
- **Transport** — stdio (v0.1), HTTP/SSE (v2.0).

### 7.4 Security

- **Path traversal prevention** — validate all file paths against index directory boundaries.
- **robots.txt compliance** — parse and enforce `robots.txt` before crawling any domain.
- **No credential storage** — API keys (if any) read from environment variables only, never persisted.
- **Input sanitization** — all user inputs validated and bounded (max query length, max URL length).
- **No arbitrary code execution** — extracted content is treated as data, never evaluated.
- **Dependency auditing** — run `cargo audit` in CI; zero known vulnerabilities at release time.

---

## 8. Success Metrics

| # | Metric | Target | Measurement Method |
|---|--------|--------|--------------------|
| 1 | **MCP tool response success rate** | ≥ 99% for well-formed requests | Server-side metric logging |
| 2 | **Cold start time** | < 100 ms on reference hardware | Automated benchmark suite |
| 3 | **Content extraction quality** | ≥ 90% accuracy vs. manual extraction on test corpus of 100 pages | Manual evaluation + automated diff scoring |
| 4 | **Recall precision@10** | ≥ 85% on benchmark query set | Automated evaluation against labeled dataset |
| 5 | **Binary size** | < 25 MB stripped release build | CI build artifact measurement |
| 6 | **Zero-config startup success** | 100% — server starts and serves tools with no configuration file | Integration test: start binary, call `tools/list` |

---

## 9. Constraints

1. **100% Rust** — no Python, Node.js, or other runtime dependencies. Single static binary distribution.
2. **Offline recall** — `recall` and `list_sources` must work without network connectivity using the local Tantivy index.
3. **Rate limit compliance** — all network-facing tools must respect per-domain rate limits and `robots.txt`. No opt-out.
4. **No mandatory paid APIs** — the MVP must be fully functional using free search backends (DuckDuckGo). Paid backends (Brave Search, Google Custom Search) are optional enhancements.
5. **MCP protocol compliance** — all tools must conform to the MCP specification. No proprietary extensions to the protocol.
6. **Minimal dependencies** — prefer well-maintained, audited crates. Avoid transitive dependency bloat.
7. **No telemetry** — the server must not phone home, collect analytics, or transmit any data beyond explicit user-initiated requests.

---

## 10. Assumptions & Dependencies

### Assumptions

1. AI agents interact exclusively via the MCP protocol (JSON-RPC 2.0 over stdio or HTTP).
2. DuckDuckGo's HTML search interface remains accessible for scraping-based search (no official API).
3. Users have a writable filesystem location for persisting the Tantivy index (default: `~/.searchxyz/index`).
4. Network connectivity is available for `search_web` and `read_url` (but not required for `recall`).
5. Target websites serve content over standard HTTP/HTTPS (no JavaScript-heavy SPAs without pre-rendering).

### External Dependencies

| Dependency | Purpose | Version | License |
|-----------|---------|---------|---------|
| `rmcp` | MCP protocol SDK (server, tools, transport) | latest | MIT/Apache-2.0 |
| `tantivy` | Full-text search engine (indexing + querying) | ^0.22 | MIT |
| `reqwest` | HTTP client (async, TLS) | ^0.12 | MIT/Apache-2.0 |
| `scraper` | HTML parsing and CSS selector-based extraction | ^0.20 | MIT |
| `thiserror` | Ergonomic error type derivation | ^2.0 | MIT/Apache-2.0 |
| `tokio` | Async runtime | ^1.0 | MIT |
| `serde` / `serde_json` | Serialization / deserialization | ^1.0 | MIT/Apache-2.0 |
| `clap` | CLI argument parsing | ^4.0 | MIT/Apache-2.0 |
| `tracing` | Structured logging and diagnostics | ^0.1 | MIT |

---

## 11. Risk Assessment

| # | Risk | Impact | Probability | Mitigation |
|---|------|--------|-------------|------------|
| 1 | **DuckDuckGo blocks scraping** | High — MVP search breaks | Medium | Pluggable backend architecture; SearxNG as self-hosted fallback; Brave Search as paid alternative. |
| 2 | **Content extraction fails on complex sites** | Medium — reduced content quality | High | Multiple extraction strategies (readability, CSS selectors, fallback to raw text). Configurable per-domain rules. |
| 3 | **Tantivy index corruption** | High — loss of indexed data | Low | Write-ahead logging; periodic backups; rebuild-index CLI command; graceful shutdown handlers. |
| 4 | **MCP specification changes** | Medium — protocol incompatibility | Low | Pin to stable `rmcp` SDK version; monitor MCP spec evolution; maintain abstraction layer over transport. |
| 5 | **Rate limiting by target sites** | Medium — degraded crawl throughput | High | Configurable per-domain rate limits; exponential backoff; respect `Retry-After` headers; user-configurable delays. |
| 6 | **Binary size exceeds target** | Low — harder distribution | Medium | Feature flags for optional components; LTO and strip in release profile; monitor size in CI. |
| 7 | **JavaScript-rendered content** | Medium — empty extractions from SPAs | Medium | Document limitation clearly; P2 roadmap item for headless browser integration; recommend pre-rendered URLs. |
| 8 | **Dependency supply-chain attack** | Critical — compromised binary | Low | Pin dependency versions; run `cargo audit` in CI; review new dependencies manually; use `cargo vet`. |

---

## 12. Release Plan

### v0.1.0 — MVP (Target: Week 4)

- `search_web` (DuckDuckGo backend)
- `read_url` (crawl + extract + Markdown)
- `index_content` (Tantivy indexing)
- `recall` (full-text search)
- `list_sources` (index listing)
- Stdio MCP transport via `rmcp`
- Structured error handling
- Rate limiting + `robots.txt`
- README, installation instructions

### v0.2.0 — Pipeline & Cache (Target: Week 8)

- `search_and_read` combined tool
- LRU page cache
- TOML configuration file support
- Auto-indexing on `read_url`
- Performance benchmarks + CI

### v1.0.0 — Production Ready (Target: Week 14)

- Stability hardening and edge-case fixes
- Comprehensive test suite (unit, integration, MCP compliance)
- Cross-platform CI (Linux, macOS, Windows)
- Pre-built binaries for major platforms
- Documentation site
- Published to crates.io

### v1.1.0 — Deep Research (Target: Week 20)

- `deep_research` iterative pipeline tool
- PDF content extraction
- Additional search backends (Brave, SearxNG)
- Improved content extraction heuristics
- Telemetry-free analytics (local-only stats)

### v2.0.0 — Semantic Search & Beyond (Target: Week 30)

- Semantic/vector search via local embeddings
- Knowledge graph construction and querying
- HTTP/SSE transport for remote MCP connections
- Site mapping and recursive crawl
- YouTube transcript extraction
- GitHub repository reading

---

## 13. Version History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 0.1.0 | 2026-06-21 | searchxyz team | Initial draft — MVP scope, architecture, and roadmap defined. |

---

> [!NOTE]
> This is a living document. It will be updated as requirements evolve, user feedback is incorporated, and technical decisions are validated through implementation.
