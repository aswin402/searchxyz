# searchxyz

<p align="center">
  <img src="assets/logo.jpg" alt="searchxyz Logo" width="200" height="200" />
</p>

An extremely high-performance Model Context Protocol (MCP) search, crawl, and content-indexing server written in Rust.

**Version:** 0.0.15

---

## Features

- **🚀 Native Performance**: Pure Rust binary. Fast startup, low CPU usage, `<30MB` idle RAM, and `<100MB` under load. No Python or Hono/Node.js dependencies.
- **🔍 Multi-Backend Search Dispatcher**: Out-of-the-box support for DuckDuckGo Lite (completely free, keyless scraping), SearXNG (privacy-centric metasearch aggregator), and optional Brave Web Search API as fallback.
- **📄 Content Extraction & Boilerplate Reduction**: Crawls target URLs using `reqwest` (with `rustls`), parses them via CSS selectors, strips out noisy elements (nav, footer, styling, ads, iframe), and outputs clean, token-efficient Markdown. Natively supports parsing and extracting text from PDF files (`application/pdf`) as well.
- **⚡ Concurrent Crawls**: Crawls and extracts up to 5 top result pages concurrently using `tokio` asynchronous workers when executing `search_and_read`.
- **💾 Local Recall Index & Hybrid Semantic Search**: Integrates a Tantivy full-text index database and local vector embedding generator using `fastembed` (BGESmallENV15, 384-dimension). Supports both hybrid semantic search (cosine similarity ranking) and classic keyword-only search, acting as a smart search-recall memory layer for AI agents.
- **🐙 GitHub Repository Ingestion**: Clones (using `git clone --depth 1`) and indexes entire codebases/repositories recursively. Automatically walks codebase files, filters by extension/size, strips directory noise, runs graph entity heuristics, and stores content for search recall.
- **🛡️ Agent-Friendly Error Handling**: Detailed, descriptive typed errors are propagated over JSON-RPC to let the consuming LLM make smart fallback decisions.
- **🌐 Rotating SOCKS5/HTTP Proxy Support**: Pools multiple proxies and rotates them randomly per request attempt for both standard crawling and headless rendering (using Chromiumoxide). Helps bypass rate-limits and prevent IP bans.

---

## MCP Tools Exposed

1. `search_web`
   - **Description**: Search the web for a query. Returns titles, URLs, and snippets.
   - **Parameters**: `query: String`, `max_results: Option<usize>`
2. `read_url`
   - **Description**: Fetch a URL (HTML page, PDF document, YouTube video, or GitHub repository root) and extract its main content. Automatically extracts closed captions for YouTube links and clones/indexes codebases for GitHub URLs.
   - **Parameters**: `url: String`
3. `search_and_read`
   - **Description**: Search the web, crawl the top `N` results concurrently, convert to markdown, index them locally, and return the formatted content.
   - **Parameters**: `query: String`, `max_pages: Option<usize>`
4. `recall`
   - **Description**: Search your local database of previously crawled pages using local semantic vector embeddings (with classic keyword search fallback).
   - **Parameters**: `query: String`, `max_results: Option<usize>`, `semantic: Option<bool>`
5. `index_content`
   - **Description**: Manually index arbitrary text content for later recall by the agent.
   - **Parameters**: `url: String`, `title: String`, `content: String`
6. `read_github_repo`
   - **Description**: Clone and recursively index a GitHub repository, parsing its files and README into the search index/knowledge graph and returning a summary.
   - **Parameters**: `repo_url: String`, `branch: Option<String>`, `include_extensions: Option<Vec<String>>`, `exclude_paths: Option<Vec<String>>`

---

## How it Works & Handshake Verification

The server starts over stdio transport by default. You can verify the handshake by piping standard JSON-RPC requests:

```bash
# Verify tools list response
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test-client","version":"1.0"}}}\n{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}\n' | ./target/debug/searchxyz
```

## Remote HTTP Server & SSE Transport

`searchxyz` can also run as a remote HTTP server utilizing Server-Sent Events (SSE) transport. This is ideal for remote hosting or multi-user deployments.

```bash
# Run HTTP server binding to 127.0.0.1 on port 3000 (endpoints exposed at http://127.0.0.1:3000/mcp)
./target/debug/searchxyz --http --host 127.0.0.1 --port 3000
```

---

## Claude Desktop Setup

Add the server to your `claude_desktop_config.json` (typically at `~/.config/Claude/claude_desktop_config.json` on Linux/macOS or `%APPDATA%/Claude/claude_desktop_config.json` on Windows):

```json
{
  "mcpServers": {
    "searchxyz": {
      "command": "/absolute/path/to/searchxyz/target/debug/searchxyz",
      "args": [],
      "env": {
        "SEARCHXYZ_BRAVE_API_KEY": "your-optional-brave-key",
        "SEARCHXYZ_LOG_LEVEL": "info"
      }
    }
  }
}
```

---

## Configuration Settings

You can customize runtime behavior by creating a `searchxyz.toml` in your working directory:

```toml
[server]
name = "searchxyz"
log_level = "info"

[search]
backends = ["duckduckgo", "brave"]
max_results = 10

[crawler]
timeout_secs = 30
max_body_bytes = 5242880 # 5MB
rate_limit_per_sec = 2

[cache]
max_entries = 1000
ttl_secs = 3600 # 1 hour
path = "~/.local/share/searchxyz/cache.json"

[proxy]
enabled = false
urls = [
    "http://proxy-host:8080",
    "socks5://proxy-host:1080"
]
```

- **SEARCHXYZ_BRAVE_API_KEY**: Overrides the Brave API key if set in env.
- **SEARCHXYZ_INDEX_PATH**: Overrides the index storage location.
- **SEARCHXYZ_LOG_LEVEL**: Overrides the server log level filter.
- **SEARCHXYZ_CACHE_PATH**: Overrides the persistent cache file location.
- **SEARCHXYZ_PROXY_ENABLED**: Set to `true` to enable proxy rotation.
- **SEARCHXYZ_PROXY_URLS**: Comma-separated list of SOCKS5 or HTTP proxy URLs to populate the rotation pool.

---

## Building Locally

```bash
# Build the project
cargo build --release

# Run unit tests
cargo test
```
