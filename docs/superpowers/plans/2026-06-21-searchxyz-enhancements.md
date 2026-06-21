# searchxyz Enhancements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement markdown-aware chunking, index/graph deletion tools, custom embedding providers (local/cloud), SSE remote auth, and incremental git ingestion inside searchxyz.

**Architecture:** Use a consolidated codebase style in Rust, introducing clean APIs for prefix deletes, config structs, Axum auth layers, and incremental repository crawlers.

**Tech Stack:** Rust (2021 edition), Tokio async runtime, Axum web framework, Tantivy index, fastembed embedding generator, reqwest HTTP client.

## Global Constraints
- Target version: `0.0.17`
- Preserve compilation with `OPENSSL_VENDORED=1`
- Enforce formatting (`cargo fmt`) and clean linting (`cargo clippy -- -D warnings`)
- Follow TDD practices by writing failing tests before coding the implementations

---

## File Structure & Impact Map

| File | Change Type | Responsibility |
|:---|:---|:---|
| `docs/superpowers/plans/2026-06-21-searchxyz-enhancements.md` | Create | This execution plan |
| `src/index/mod.rs` | Modify | Update document ingestion with chunking; implement prefix deletion queries; support multi-provider embedding lookup. |
| `src/config/mod.rs` | Modify | Add schemas for `EmbeddingConfig` and `auth_token`. |
| `src/graph/mod.rs` | Modify | Implement `prune_node` and `clear` on `KnowledgeGraph`. |
| `src/tools/mod.rs` | Modify | Register `delete_source` and `clear_index` MCP tools. |
| `src/main.rs` | Modify | Add Bearer Token Axum authentication middleware. |
| `src/crawler/github.rs` | Modify | Implement incremental cloning, pulling, git diff matching, and partial updates. |
| `CHANGELOG.md` | Modify | Document version `0.0.17` updates. |

---

## Tasks

### Task 1: Markdown-Aware Chunking & Prefix Deletion

**Files:**
- Modify: `src/index/mod.rs`
- Test: `src/index/mod.rs` (adding unit tests to tests module)

**Interfaces:**
- Consumes: None
- Produces: `SearchIndex::delete_document(&self, url: &str)` and chunked indexing within `SearchIndex::add_document`.

- [ ] **Step 1: Write the failing tests**
  Add these tests to `mod tests` inside `src/index/mod.rs`:
  ```rust
  #[tokio::test]
  async fn test_markdown_chunking_logic() {
      let content = "Paragraph 1\n\n# Header 1\nSection 1 text here.\n\n## Header 2\nSection 2 text here.";
      let chunks = chunk_content(content, 1500, 200);
      assert_eq!(chunks.len(), 3);
      assert!(chunks[0].contains("Paragraph 1"));
      assert!(chunks[1].contains("# Header 1"));
      assert!(chunks[2].contains("## Header 2"));
  }

  #[tokio::test]
  async fn test_prefix_deletion() {
      let test_dir = std::env::temp_dir().join(format!("searchxyz_test_prefix_del_{}", rand::random::<u64>()));
      let config = IndexConfig { path: test_dir.clone(), writer_heap_bytes: 15_000_000 };
      let index = SearchIndex::open(&config).unwrap();
      
      // Index doc as chunks
      let doc = ExtractedContent {
          url: "https://example.com/prefix-del".to_string(),
          title: "Prefix Del".to_string(),
          description: "".to_string(),
          content_markdown: "Doc chunk 1\n\n# Header\nDoc chunk 2".to_string(),
          links: vec![],
      };
      index.add_document(&doc, "test").await.unwrap();
      index.reload().unwrap();

      // Deleting parent URL must wipe all chunks
      index.delete_document("https://example.com/prefix-del").await.unwrap();
      index.reload().unwrap();

      let results = index.search("Prefix", 10).unwrap();
      assert_eq!(results.len(), 0);
      let _ = std::fs::remove_dir_all(&test_dir);
  }
  ```

- [ ] **Step 2: Run test to verify it fails**
  Run: `OPENSSL_VENDORED=1 cargo test index::tests::test_markdown_chunking_logic`
  Expected: FAIL (types/functions undefined)

- [ ] **Step 3: Write minimal implementation**
  Add the chunker logic and update `add_document` and `delete_document` in `src/index/mod.rs`:
  ```rust
  pub fn chunk_content(content: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
      let mut chunks = Vec::new();
      let mut current_chunk = String::new();
      
      for line in content.lines() {
          if line.starts_with("# ") || line.starts_with("## ") || line.starts_with("### ") {
              if !current_chunk.trim().is_empty() {
                  chunks.push(current_chunk.trim().to_string());
                  current_chunk = String::new();
              }
          }
          current_chunk.push_str(line);
          current_chunk.push('\n');
          
          if current_chunk.len() >= chunk_size {
              chunks.push(current_chunk.trim().to_string());
              current_chunk = String::new();
          }
      }
      if !current_chunk.trim().is_empty() {
          chunks.push(current_chunk.trim().to_string());
      }
      chunks
  }
  ```
  Update `add_document` to divide `content.content_markdown` into chunks using `chunk_content`. Index each chunk under `format!("{}#chunk-{}", content.url, index)` if chunks count > 1.
  Implement `delete_document`:
  ```rust
  pub async fn delete_document(&self, url: &str) -> Result<(), SearchXyzError> {
      let mut writer = self.writer.lock().await;
      let term = tantivy::Term::from_field_text(self.f_url, url);
      writer.delete_term(term);
      
      let prefix_term = tantivy::Term::from_field_text(self.f_url, &format!("{}#", url));
      let query = tantivy::query::PrefixQuery::new(prefix_term);
      writer.delete_query(&query)?;
      writer.commit()?;
      Ok(())
  }
  ```

- [ ] **Step 4: Run test to verify it passes**
  Run: `OPENSSL_VENDORED=1 cargo test index::tests`
  Expected: PASS

- [ ] **Step 5: Commit**
  Run:
  ```bash
  git add src/index/mod.rs
  git commit -m "feat: implement markdown-aware chunking and prefix deletion query"
  ```

---

### Task 2: Graph Pruning and DB Maintenance MCP Tools

**Files:**
- Modify: `src/graph/mod.rs`
- Modify: `src/tools/mod.rs`
- Test: `src/graph/mod.rs` (verify prune node inside `tests`), `src/tools/mod.rs` (testing new tools)

**Interfaces:**
- Consumes: `SearchIndex::delete_document`, `KnowledgeGraph::nodes`, `KnowledgeGraph::edges`
- Produces: `KnowledgeGraph::prune_node(&mut self, name: &str)`, `KnowledgeGraph::clear(&mut self)`, and MCP tools `delete_source` and `clear_index`.

- [ ] **Step 1: Write the failing tests**
  Add unit test inside `src/graph/mod.rs`:
  ```rust
  #[test]
  fn test_graph_pruning() {
      let mut graph = KnowledgeGraph::new();
      graph.add_edge("Doc1".to_string(), "Document".to_string(), "Rust".to_string(), "Concept".to_string(), "mentions".to_string());
      assert_eq!(graph.nodes.len(), 2);
      
      graph.prune_node("Doc1");
      assert_eq!(graph.nodes.len(), 1);
      assert_eq!(graph.edges.len(), 0);
  }
  ```

- [ ] **Step 2: Run test to verify it fails**
  Run: `OPENSSL_VENDORED=1 cargo test graph::tests::test_graph_pruning`
  Expected: FAIL

- [ ] **Step 3: Write minimal implementation**
  Add graph methods inside `src/graph/mod.rs`:
  ```rust
  pub fn prune_node(&mut self, name: &str) {
      let canonical_name = self.find_canonical_node_name(name).unwrap_or_else(|| name.to_string());
      self.nodes.remove(&canonical_name);
      self.edges.retain(|e| e.source != canonical_name && e.target != canonical_name);
  }

  pub fn clear(&mut self) {
      self.nodes.clear();
      self.edges.clear();
  }
  ```
  Register `delete_source` and `clear_index` structs and implementations inside `src/tools/mod.rs`:
  ```rust
  #[derive(Deserialize, JsonSchema)]
  pub struct DeleteSourceRequest {
      pub url: String,
  }

  #[derive(Deserialize, JsonSchema)]
  pub struct ClearIndexRequest {}
  ```
  Expose the tools:
  ```rust
  #[tool(description = "Wipe a specific source URL and its chunks from index and knowledge graph.")]
  async fn delete_source(&self, req: Parameters<DeleteSourceRequest>) -> Result<String, rmcp::ErrorData> {
      self.index.delete_document(&req.0.url).await?;
      {
          let mut g = self.graph.lock().await;
          g.prune_node(&req.0.url);
      }
      Ok(format!("Successfully deleted source `{}`", req.0.url))
  }

  #[tool(description = "Wipe all documents and graph relationships from the local index.")]
  async fn clear_index(&self, req: Parameters<ClearIndexRequest>) -> Result<String, rmcp::ErrorData> {
      let mut writer = self.index.writer.lock().await;
      writer.delete_all_documents()?;
      writer.commit()?;
      {
          let mut g = self.graph.lock().await;
          g.clear();
      }
      Ok("Successfully cleared search index and knowledge graph.".to_string())
  }
  ```

- [ ] **Step 4: Run test to verify it passes**
  Run: `OPENSSL_VENDORED=1 cargo test`
  Expected: PASS

- [ ] **Step 5: Commit**
  Run:
  ```bash
  git add src/graph/mod.rs src/tools/mod.rs
  git commit -m "feat: implement knowledge graph pruning and register delete_source and clear_index tools"
  ```

---

### Task 3: Embedding Model Customization (Local/Cloud)

**Files:**
- Modify: `src/config/mod.rs`
- Modify: `src/index/mod.rs`
- Test: Add embedding generator tests to `src/index/mod.rs`

**Interfaces:**
- Consumes: Config structures
- Produces: Dynamic vector generation matching the selected provider.

- [ ] **Step 1: Write the failing tests**
  Add mock cloud API responses or test embedding provider configuration checks inside `tests` of `src/index/mod.rs`.

- [ ] **Step 2: Run test to verify it fails**
  Run: `OPENSSL_VENDORED=1 cargo test index::tests`
  Expected: FAIL

- [ ] **Step 3: Write minimal implementation**
  Define `EmbeddingConfig` inside `src/config/mod.rs` and read it.
  In `src/index/mod.rs`, replace direct `TextEmbedding` with `EmbeddingGenerator` enum. Implement `embed_text(&self, text: &str)` which maps:
  - `local`: uses `TextEmbedding` ONNX model.
  - `openai`: POSTs to `https://api.openai.com/v1/embeddings` using bearer token authorization.
  - `gemini`: POSTs to Google Gemini embeddings API.
  - `cohere`: POSTs to Cohere embeddings API.
  Adjust vector search logic in `search_semantic` to read embeddings query dimension dynamically.

- [ ] **Step 4: Run test to verify it passes**
  Run: `OPENSSL_VENDORED=1 cargo test`
  Expected: PASS

- [ ] **Step 5: Commit**
  Run:
  ```bash
  git add src/config/mod.rs src/index/mod.rs
  git commit -m "feat: implement local/cloud custom embedding models configuration and HTTP api dispatchers"
  ```

---

### Task 4: Bearer Token SSE HTTP Authentication Middleware

**Files:**
- Modify: `src/config/mod.rs`
- Modify: `src/main.rs`
- Test: Create a mock client request to verify authentication blocks unauthorized connections

- [ ] **Step 1: Write the failing tests**
  Add an integration test validating HTTP authentication failures.

- [ ] **Step 2: Run test to verify it fails**
  Run: `OPENSSL_VENDORED=1 cargo test`
  Expected: FAIL

- [ ] **Step 3: Write minimal implementation**
  Add `auth_token` to `ServerConfig` inside `src/config/mod.rs`.
  Add authentication middleware inside `src/main.rs` to intercept axum routes:
  ```rust
  async fn auth_middleware(
      req: axum::extract::Request,
      next: axum::middleware::Next,
      expected_token: String,
  ) -> Result<axum::response::Response, axum::http::StatusCode> {
      let auth_header = req.headers()
          .get(axum::http::header::AUTHORIZATION)
          .and_then(|h| h.to_str().ok());
      
      if let Some(auth) = auth_header {
          if auth.starts_with("Bearer ") && &auth[7..] == expected_token {
              return Ok(next.run(req).await);
          }
      }
      Err(axum::http::StatusCode::UNAUTHORIZED)
  }
  ```
  Register this layer on the HTTP router in `src/main.rs` if `config.server.auth_token` is present.

- [ ] **Step 4: Run test to verify it passes**
  Run: `OPENSSL_VENDORED=1 cargo test`
  Expected: PASS

- [ ] **Step 5: Commit**
  Run:
  ```bash
  git add src/config/mod.rs src/main.rs
  git commit -m "feat: add Bearer Token Axum HTTP authentication layer for SSE remote server"
  ```

---

### Task 5: Incremental Git Codebase Ingestion

**Files:**
- Modify: `src/crawler/github.rs`
- Test: Verify index changes dynamically upon running incremental sync

- [ ] **Step 1: Write the failing tests**
  Add an integration test indexing a mock repo, changing files, and verifying only delta changes are computed and re-indexed.

- [ ] **Step 2: Run test to verify it fails**
  Run: `OPENSSL_VENDORED=1 cargo test crawler::github::tests`
  Expected: FAIL

- [ ] **Step 3: Write minimal implementation**
  Modify `src/crawler/github.rs` target cloning directory: check if the target folder exists.
  If it exists:
  - Run git command to pull: `git pull origin <branch>`.
  - Fetch modified file list: `git diff --name-only HEAD@{1} HEAD`.
  - For modified/deleted files: delete their previous chunk paths from Tantivy.
  - For added/modified files: parse files, apply `chunk_content` to their code, and write to search index.
  If directory is absent, clone normally and index all codebase files recursively.

- [ ] **Step 4: Run test to verify it passes**
  Run: `OPENSSL_VENDORED=1 cargo test`
  Expected: PASS

- [ ] **Step 5: Commit & Push**
  Run:
  ```bash
  git add src/crawler/github.rs CHANGELOG.md
  git commit -m "feat: support incremental git code repository indexing based on git pull delta diffs"
  git push
  ```
