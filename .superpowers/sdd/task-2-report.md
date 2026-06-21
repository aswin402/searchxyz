# Task 2 Report: Graph Pruning and DB Maintenance MCP Tools

## What was implemented
1. **Knowledge Graph Pruning and Clearing (`src/graph/mod.rs`):**
   - Implemented `prune_node(&mut self, name: &str)` on `KnowledgeGraph` which canonicalizes the input node name, removes it from `self.nodes`, and removes any edges connected to it from `self.edges`.
   - Implemented `clear(&mut self)` on `KnowledgeGraph` which clears all nodes and edges from the graph.
2. **SearchIndex Writer Exposure (`src/index/mod.rs`):**
   - Changed the visibility of the `writer` field in `SearchIndex` to `pub(crate)` to allow external index maintenance tools inside the `tools` module to acquire a lock on the `IndexWriter`.
3. **MCP DB Maintenance Tools (`src/tools/mod.rs`):**
   - Added `DeleteSourceRequest` and `ClearIndexRequest` schemas.
   - Implemented the `delete_source` tool method inside `SearchXyzServer` impl block which removes a source URL and all of its chunks from the search index (`self.index.delete_document`) and prunes it from the knowledge graph (`g.prune_node`).
   - Implemented the `clear_index` tool method inside `SearchXyzServer` impl block which locks the index writer, wipes all documents using `writer.delete_all_documents()`, commits the change, and clears the knowledge graph.
   - Registered both tools successfully using the `#[tool(...)]` attribute macro.

## TDD Evidence

### RED Phase
- **Command Run:** `OPENSSL_VENDORED=1 cargo test`
- **Failing Output:**
  ```text
  error[E0422]: cannot find struct, variant or union type `DeleteSourceRequest` in this scope
      --> src/tools/mod.rs:1073:39
  error[E0422]: cannot find struct, variant or union type `ClearIndexRequest` in this scope
      --> src/tools/mod.rs:1098:37
  error[E0599]: no method named `prune_node` found for struct `graph::KnowledgeGraph` in the current scope
     --> src/graph/mod.rs:375:15
  error[E0599]: no method named `delete_source` found for struct `tools::SearchXyzServer` in the current scope
      --> src/tools/mod.rs:1073:14
  error[E0599]: no method named `clear_index` found for struct `tools::SearchXyzServer` in the current scope
      --> src/tools/mod.rs:1098:14
  ```
- **Why the failure was expected:** The structs (`DeleteSourceRequest`, `ClearIndexRequest`) and methods (`prune_node`, `delete_source`, `clear_index`) were referenced in the newly written tests but not yet defined in the codebase.

### GREEN Phase
- **Command Run:** `OPENSSL_VENDORED=1 cargo test`
- **Passing Output:**
  ```text
  running 32 tests
  test config::tests::test_config_defaults ... ok
  test crawler::fingerprint::tests::test_random_headers ... ok
  test crawler::github::tests::test_parse_github_url ... ok
  test crawler::sitemap::tests::test_parse_sitemap_urls ... ok
  test crawler::sitemap::tests::test_parse_sitemaps_from_robots ... ok
  test config::tests::test_env_overrides ... ok
  test crawler::fast_spider::tests::test_extract_links_from_html ... ok
  test crawler::youtube::tests::test_extract_video_id ... ok
  test cache::tests::test_cache_serialization_deserialization ... ok
  test crawler::youtube::tests::test_parse_xml_transcript ... ok
  test crawler::github::tests::test_visit_dirs ... ok
  test graph::tests::test_extract_heuristics ... ok
  test graph::tests::test_graph_bfs_neighbors ... ok
  test extractor::tests::test_pdf_extraction_bypass ... ok
  test graph::tests::test_graph_crud ... ok
  test graph::tests::test_graph_pruning ... ok
  test index::tests::test_markdown_chunking_logic ... ok
  test index::tests::test_section_splitting_large ... ok
  test index::tests::test_sliding_window_fallback ... ok
  test search::google::tests::test_google_parsing ... ok
  test search::bing::tests::test_bing_parsing ... ok
  test extractor::tests::test_pdf_parsing ... ok
  test crawler::youtube::tests::test_fetch_youtube_transcript_cached ... ok
  test crawler::fast_spider::tests::test_link_spider_bfs_cached ... ok
  test crawler::tests::test_crawler_client_pooling ... ok
  test crawler::sitemap::tests::test_discover_sitemap_urls_cached ... ok
  test index::tests::test_prefix_deletion ... ok
  test index::tests::test_export_documents ... ok
  test index::tests::test_list_documents ... ok
  test index::tests::test_semantic_search ... ok
  test tools::tests::test_db_maintenance_tools ... ok
  test tools::tests::test_export_import_research ... ok

  test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.67s
  ```

## Files Changed
- `src/graph/mod.rs`: Implemented `KnowledgeGraph::prune_node` and `KnowledgeGraph::clear` along with unit tests.
- `src/index/mod.rs`: Changed visibility of `SearchIndex::writer` to `pub(crate)`.
- `src/tools/mod.rs`: Implemented and registered `delete_source` and `clear_index` tools, added parameter structs, and added unit tests.

## Self-Review Findings
- All tests pass compilation and execution seamlessly.
- Used idiomatic Rust error handling, mapped Tantivy errors correctly to keep signatures clean.
- Excluded unnecessary `.clone()` and avoided `.unwrap()` in the implementation.
