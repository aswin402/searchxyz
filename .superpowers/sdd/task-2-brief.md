### Task 2: Graph Pruning and DB Maintenance MCP Tools

**Files:**
- Modify: `src/graph/mod.rs`
- Modify: `src/tools/mod.rs`
- Test: `src/graph/mod.rs` (verify prune node inside `tests`), `src/tools/mod.rs` (testing new tools `delete_source` and `clear_index`)

**Interfaces:**
- Consumes: `SearchIndex::delete_document`, `KnowledgeGraph::nodes`, `KnowledgeGraph::edges`
- Produces: `KnowledgeGraph::prune_node(&mut self, name: &str)`, `KnowledgeGraph::clear(&mut self)`, and MCP tools `delete_source` and `clear_index`.

- [ ] **Step 1: Write the failing tests**
  1. Add `test_graph_pruning` to `tests` module in `src/graph/mod.rs`:
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
  2. Add unit tests for the tools to `tests` module in `src/tools/mod.rs` (e.g. testing `delete_source` and `clear_index` by calling them directly on the test server, checking that they wipe matching document chunks and graphs).

- [ ] **Step 2: Run test to verify it fails**
  Run: `OPENSSL_VENDORED=1 cargo test`
  Expected: Compile errors or test failures because `prune_node`, `clear`, `delete_source` and `clear_index` are not defined.

- [ ] **Step 3: Write minimal implementation**
  1. Add graph methods inside `src/graph/mod.rs`:
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
  2. Register `DeleteSourceRequest` and `ClearIndexRequest` structs and implementations inside `src/tools/mod.rs`.
     Remember to register them with `#[tool]` attributes and import any required traits/macros.
     For `delete_source`:
     ```rust
     #[derive(Deserialize, JsonSchema)]
     pub struct DeleteSourceRequest {
         pub url: String,
     }

     #[derive(Deserialize, JsonSchema)]
     pub struct ClearIndexRequest {}
     ```
     Within `impl SearchXyzServer`:
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
     Wait, make sure `delete_all_documents` is correct and doesn't require extra handling. If `writer.delete_all_documents()` returns `Result<(), tantivy::TantivyError>`, then map/handle it correctly or wrap with `?` if error types align (which they do since `SearchXyzError` implements `From<tantivy::TantivyError>`).

- [ ] **Step 4: Run test to verify it passes**
  Run: `OPENSSL_VENDORED=1 cargo test`
  Expected: All tests pass.

- [ ] **Step 5: Commit**
  Run:
  ```bash
  git add src/graph/mod.rs src/tools/mod.rs
  git commit -m "feat: implement knowledge graph pruning and register delete_source and clear_index tools"
  ```
