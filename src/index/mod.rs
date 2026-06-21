use std::sync::Arc;

use chrono::Utc;
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use tantivy::{
    collector::TopDocs, directory::MmapDirectory, doc, query::QueryParser, schema::*, Index,
    IndexReader, IndexWriter, ReloadPolicy,
};
use tokio::sync::Mutex;

use crate::config::IndexConfig;
use crate::error::SearchXyzError;
use crate::extractor::ExtractedContent;

/// Thread-safe full-text search index backed by Tantivy.
pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    writer: Arc<Mutex<IndexWriter>>,
    embedding_model: std::sync::Mutex<TextEmbedding>,
    // Schema field handles — kept for building docs & queries.
    f_url: Field,
    f_title: Field,
    f_content: Field,
    f_source: Field,
    f_indexed_at: Field,
    f_embedding: Field,
}

/// A result from querying the local index.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexSearchResult {
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub source: String,
    pub score: f32,
}

/// An entry representing a document's source metadata.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceEntry {
    pub url: String,
    pub title: String,
    pub source: String,
    pub indexed_at: String,
}

impl SearchIndex {
    /// Open or create the index at the configured path.
    pub fn open(config: &IndexConfig) -> Result<Self, SearchXyzError> {
        // Ensure directory exists.
        std::fs::create_dir_all(&config.path)?;

        // ── Build schema ──
        let mut builder = Schema::builder();

        let f_url = builder.add_text_field("url", STRING | STORED);
        let f_title = builder.add_text_field("title", TEXT | STORED);
        let f_content = builder.add_text_field("content", TEXT | STORED);
        let f_source = builder.add_text_field("source", TEXT | STORED);
        let f_indexed_at = builder.add_date_field("indexed_at", INDEXED | STORED);
        let f_embedding =
            builder.add_bytes_field("embedding", BytesOptions::default().set_stored());

        let schema = builder.build();

        // ── Open or create index ──
        let dir = MmapDirectory::open(&config.path).map_err(|e| {
            SearchXyzError::IndexError(format!("Failed to open index directory: {e}"))
        })?;

        let index = Index::open_or_create(dir, schema.clone())
            .map_err(|e| SearchXyzError::IndexError(format!("Failed to open/create index: {e}")))?;

        // ── Reader (auto-reload on new commits) ──
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| {
                SearchXyzError::IndexError(format!("Failed to create reader: {e}"))
            })?;

        // ── Writer ──
        let writer = index
            .writer(config.writer_heap_bytes)
            .map_err(|e| SearchXyzError::IndexError(format!("Failed to create writer: {e}")))?;

        // ── Embeddings Model ──
        let embedding_model = TextEmbedding::try_new(
            TextInitOptions::new(EmbeddingModel::BGESmallENV15).with_show_download_progress(false),
        )
        .map_err(|e| {
            SearchXyzError::IndexError(format!("Failed to initialize embedding model: {e}"))
        })?;

        Ok(Self {
            index,
            reader,
            writer: Arc::new(Mutex::new(writer)),
            embedding_model: std::sync::Mutex::new(embedding_model),
            f_url,
            f_title,
            f_content,
            f_source,
            f_indexed_at,
            f_embedding,
        })
    }

    /// Index a piece of extracted content.
    pub async fn add_document(
        &self,
        content: &ExtractedContent,
        source: &str,
    ) -> Result<(), SearchXyzError> {
        let now = tantivy::DateTime::from_timestamp_secs(Utc::now().timestamp());

        let chunks = chunk_content(&content.content_markdown, 1500, 200);

        let texts_to_embed: Vec<String> = if chunks.len() > 1 {
            chunks
                .iter()
                .map(|chunk| {
                    let text = format!("passage: {}\n\n{}", content.title, chunk);
                    text.chars().take(4000).collect()
                })
                .collect()
        } else {
            let text = format!("passage: {}\n\n{}", content.title, content.content_markdown);
            vec![text.chars().take(4000).collect()]
        };

        let embeddings = {
            let mut model = self.embedding_model.lock().map_err(|e| {
                SearchXyzError::IndexError(format!("Embedding model mutex poisoned: {e}"))
            })?;
            let refs: Vec<&str> = texts_to_embed.iter().map(|s| s.as_str()).collect();
            model.embed(refs, None).map_err(|e| {
                SearchXyzError::IndexError(format!("Failed to generate embeddings: {e}"))
            })?
        };

        let mut writer = self.writer.lock().await;

        // Remove existing document with same URL to avoid duplicates.
        let term = tantivy::Term::from_field_text(self.f_url, &content.url);
        writer.delete_term(term);

        let pattern = format!("{}.*", escape_regex(&format!("{}#", content.url)));
        let query = tantivy::query::RegexQuery::from_pattern(&pattern, self.f_url)?;
        writer.delete_query(Box::new(query))?;

        if chunks.len() > 1 {
            for (idx, (chunk, embedding)) in chunks.into_iter().zip(embeddings).enumerate() {
                let chunk_url = format!("{}#chunk-{}", content.url, idx);
                let mut embedding_bytes = Vec::with_capacity(embedding.len() * 4);
                for val in &embedding {
                    embedding_bytes.extend_from_slice(&val.to_le_bytes());
                }

                writer.add_document(doc!(
                    self.f_url     => chunk_url,
                    self.f_title   => content.title.clone(),
                    self.f_content => chunk,
                    self.f_source  => source.to_string(),
                    self.f_indexed_at => now,
                    self.f_embedding => embedding_bytes,
                ))?;
            }
        } else {
            let chunk_content = if let Some(first) = chunks.into_iter().next() {
                first
            } else {
                content.content_markdown.clone()
            };
            let embedding = embeddings
                .into_iter()
                .next()
                .ok_or_else(|| SearchXyzError::IndexError("No embedding returned".to_string()))?;

            let mut embedding_bytes = Vec::with_capacity(embedding.len() * 4);
            for val in &embedding {
                embedding_bytes.extend_from_slice(&val.to_le_bytes());
            }

            writer.add_document(doc!(
                self.f_url     => content.url.clone(),
                self.f_title   => content.title.clone(),
                self.f_content => chunk_content,
                self.f_source  => source.to_string(),
                self.f_indexed_at => now,
                self.f_embedding => embedding_bytes,
            ))?;
        }
        writer.commit()?;

        tracing::debug!(url = %content.url, "Indexed document");

        Ok(())
    }

    /// Semantic vector search across indexed content.
    pub fn search_semantic(
        &self,
        query_str: &str,
        max_results: usize,
    ) -> Result<Vec<IndexSearchResult>, SearchXyzError> {
        let query_text = format!("query: {query_str}");
        let query_embeddings = {
            let mut model = self.embedding_model.lock().map_err(|e| {
                SearchXyzError::IndexError(format!("Embedding model mutex poisoned: {e}"))
            })?;
            model.embed(vec![query_text.as_str()], None).map_err(|e| {
                SearchXyzError::IndexError(format!("Failed to generate query embedding: {e}"))
            })?
        };
        let query_embedding = query_embeddings
            .into_iter()
            .next()
            .ok_or_else(|| SearchXyzError::IndexError("No query embedding returned".to_string()))?;

        let searcher = self.reader.searcher();
        use tantivy::query::AllQuery;
        let top_docs = searcher
            .search(&AllQuery, &TopDocs::with_limit(10000))
            .map_err(|e| {
                SearchXyzError::IndexError(format!(
                    "Failed to retrieve candidates for semantic search: {e}"
                ))
            })?;

        let mut scored_results = Vec::new();

        for (_tantivy_score, doc_address) in top_docs {
            let doc: tantivy::TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| SearchXyzError::IndexError(format!("Failed to retrieve doc: {e}")))?;

            let embedding_val = doc.get_first(self.f_embedding);
            if let Some(bytes_val) = embedding_val.and_then(|v| v.as_bytes()) {
                let mut doc_embedding = Vec::with_capacity(bytes_val.len() / 4);
                for chunk in bytes_val.chunks_exact(4) {
                    let array: [u8; 4] = chunk.try_into().unwrap();
                    doc_embedding.push(f32::from_le_bytes(array));
                }

                if doc_embedding.len() == query_embedding.len() {
                    let score: f32 = query_embedding
                        .iter()
                        .zip(&doc_embedding)
                        .map(|(a, b)| a * b)
                        .sum();

                    let url = doc
                        .get_first(self.f_url)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let title = doc
                        .get_first(self.f_title)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let source = doc
                        .get_first(self.f_source)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let content = doc
                        .get_first(self.f_content)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let snippet = content
                        .chars()
                        .take(250)
                        .collect::<String>()
                        .replace('\n', " ")
                        .replace("  ", " ");

                    scored_results.push((
                        score,
                        IndexSearchResult {
                            url,
                            title,
                            snippet,
                            source,
                            score,
                        },
                    ));
                }
            }
        }

        // Sort by score descending (f32 comparison)
        scored_results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Take top max_results
        let results: Vec<IndexSearchResult> = scored_results
            .into_iter()
            .take(max_results)
            .map(|(_, res)| res)
            .collect();

        Ok(results)
    }

    /// Full-text search across indexed content.
    pub fn search(
        &self,
        query_str: &str,
        max_results: usize,
    ) -> Result<Vec<IndexSearchResult>, SearchXyzError> {
        let searcher = self.reader.searcher();

        let query_parser = QueryParser::for_index(&self.index, vec![self.f_title, self.f_content]);

        let query = query_parser.parse_query(query_str).map_err(|e| {
            SearchXyzError::IndexError(format!("Failed to parse query `{query_str}`: {e}"))
        })?;

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(max_results))
            .map_err(|e| SearchXyzError::IndexError(format!("Search execution failed: {e}")))?;

        // ── Build snippet generator for content field ──
        let snippet_generator =
            tantivy::SnippetGenerator::create(&searcher, &query, self.f_content).map_err(|e| {
                SearchXyzError::IndexError(format!("Snippet generator failed: {e}"))
            })?;

        let mut results = Vec::with_capacity(top_docs.len());

        for (score, doc_address) in top_docs {
            let doc: tantivy::TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| SearchXyzError::IndexError(format!("Failed to retrieve doc: {e}")))?;

            let url = doc
                .get_first(self.f_url)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let title = doc
                .get_first(self.f_title)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let source = doc
                .get_first(self.f_source)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let snippet = snippet_generator.snippet_from_doc(&doc).to_html();

            results.push(IndexSearchResult {
                url,
                title,
                snippet,
                source,
                score,
            });
        }

        Ok(results)
    }

    /// Retrieve metadata of all indexed documents (sources) with optional filtering and pagination.
    pub fn list_documents(
        &self,
        source_filter: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<SourceEntry>, usize), SearchXyzError> {
        let searcher = self.reader.searcher();

        let query: Box<dyn tantivy::query::Query> = if let Some(src) = source_filter {
            let term = tantivy::Term::from_field_text(self.f_source, src);
            Box::new(tantivy::query::TermQuery::new(
                term,
                IndexRecordOption::WithFreqs,
            ))
        } else {
            use tantivy::query::AllQuery;
            Box::new(AllQuery)
        };

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(10000))
            .map_err(|e| SearchXyzError::IndexError(format!("Failed to search index: {e}")))?;

        let mut entries = Vec::new();
        for (_, doc_address) in top_docs {
            let doc: tantivy::TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| SearchXyzError::IndexError(format!("Failed to retrieve doc: {e}")))?;

            let url = doc
                .get_first(self.f_url)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let title = doc
                .get_first(self.f_title)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let source = doc
                .get_first(self.f_source)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let date_val = doc
                .get_first(self.f_indexed_at)
                .and_then(|v| v.as_datetime());
            let indexed_at = date_val
                .and_then(|dt: tantivy::DateTime| {
                    chrono::DateTime::from_timestamp(dt.into_timestamp_secs(), 0)
                })
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default();

            entries.push((
                date_val,
                SourceEntry {
                    url,
                    title,
                    source,
                    indexed_at,
                },
            ));
        }

        // Sort by date descending.
        entries.sort_by(|a, b| b.0.cmp(&a.0));

        let total_count = entries.len();

        // Paginate.
        let paginated = entries
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(_, item)| item)
            .collect();

        Ok((paginated, total_count))
    }

    /// Retrieve documents with full content for exporting.
    pub fn export_documents(
        &self,
        query_str: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ExtractedContent>, SearchXyzError> {
        let searcher = self.reader.searcher();

        let query: Box<dyn tantivy::query::Query> = if let Some(q) = query_str {
            if q.trim().is_empty() {
                use tantivy::query::AllQuery;
                Box::new(AllQuery)
            } else {
                let query_parser =
                    QueryParser::for_index(&self.index, vec![self.f_title, self.f_content]);
                query_parser.parse_query(q).map_err(|e| {
                    SearchXyzError::IndexError(format!("Failed to parse query `{q}`: {e}"))
                })?
            }
        } else {
            use tantivy::query::AllQuery;
            Box::new(AllQuery)
        };

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit))
            .map_err(|e| SearchXyzError::IndexError(format!("Search execution failed: {e}")))?;

        let mut results = Vec::with_capacity(top_docs.len());
        for (_, doc_address) in top_docs {
            let doc: tantivy::TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| SearchXyzError::IndexError(format!("Failed to retrieve doc: {e}")))?;

            let url = doc
                .get_first(self.f_url)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let title = doc
                .get_first(self.f_title)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let content_markdown = doc
                .get_first(self.f_content)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let source = doc
                .get_first(self.f_source)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            results.push(ExtractedContent {
                url,
                title,
                description: format!("Exported from source: {}", source),
                content_markdown,
                links: Vec::new(),
            });
        }

        Ok(results)
    }

    /// Delete all documents matching a URL.
    #[allow(dead_code)]
    pub async fn delete_by_url(&self, url: &str) -> Result<(), SearchXyzError> {
        let term = tantivy::Term::from_field_text(self.f_url, url);
        let mut writer = self.writer.lock().await;
        writer.delete_term(term);
        writer.commit()?;
        tracing::debug!(url, "Deleted from index");
        Ok(())
    }

    /// Reload the index reader to see new updates.
    #[allow(dead_code)]
    pub fn reload(&self) -> Result<(), SearchXyzError> {
        self.reader.reload().map_err(|e| {
            SearchXyzError::IndexError(format!("Failed to reload index reader: {}", e))
        })
    }

    /// Delete a document by url and any of its associated chunk documents.
    pub async fn delete_document(&self, url: &str) -> Result<(), SearchXyzError> {
        let mut writer = self.writer.lock().await;
        let term = tantivy::Term::from_field_text(self.f_url, url);
        writer.delete_term(term);

        let pattern = format!("{}.*", escape_regex(&format!("{}#", url)));
        let query = tantivy::query::RegexQuery::from_pattern(&pattern, self.f_url)?;
        writer.delete_query(Box::new(query))?;
        writer.commit()?;
        Ok(())
    }
}

pub fn chunk_content(content: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    let mut sections = Vec::new();
    let mut current_section = String::new();
    let mut has_headers = false;

    for line in content.lines() {
        if line.starts_with("# ") || line.starts_with("## ") || line.starts_with("### ") {
            has_headers = true;
            if !current_section.trim().is_empty() {
                sections.push(current_section.trim().to_string());
            }
            current_section = line.to_string();
            current_section.push('\n');
        } else {
            current_section.push_str(line);
            current_section.push('\n');
        }
    }
    if !current_section.trim().is_empty() {
        sections.push(current_section.trim().to_string());
    }

    if !has_headers {
        return sliding_window_chunk(content, chunk_size, overlap);
    }

    let mut chunks = Vec::new();
    for section in sections {
        if section.chars().count() > chunk_size {
            chunks.extend(sliding_window_chunk(&section, chunk_size, overlap));
        } else {
            chunks.push(section);
        }
    }
    chunks
}

fn sliding_window_chunk(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut chunks = Vec::new();

    if chars.is_empty() {
        return chunks;
    }

    let mut start = 0;
    while start < chars.len() {
        let end = std::cmp::min(start + chunk_size, chars.len());
        let chunk: String = chars[start..end].iter().collect();
        let chunk_trimmed = chunk.trim().to_string();
        if !chunk_trimmed.is_empty() {
            chunks.push(chunk_trimmed);
        }

        if end == chars.len() {
            break;
        }

        let step = if chunk_size > overlap {
            chunk_size - overlap
        } else {
            1
        };
        start += step;
    }
    chunks
}

fn escape_regex(s: &str) -> String {
    let mut escaped = String::new();
    for c in s.chars() {
        match c {
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' => {
                escaped.push('\\');
                escaped.push(c);
            }
            _ => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IndexConfig;

    #[tokio::test]
    async fn test_semantic_search() {
        let test_dir =
            std::env::temp_dir().join(format!("searchxyz_test_{}", rand::random::<u64>()));
        let _ = std::fs::remove_dir_all(&test_dir);

        let config = IndexConfig {
            path: test_dir.clone(),
            writer_heap_bytes: 15_000_000,
        };

        let index = SearchIndex::open(&config).unwrap();

        let doc1 = ExtractedContent {
            url: "https://example.com/quantum".to_string(),
            title: "Quantum Computing Foundations".to_string(),
            description: "".to_string(),
            content_markdown: "Quantum mechanics describes the physical properties of nature at the scale of atoms and subatomic particles.".to_string(),
            links: vec![],
        };

        let doc2 = ExtractedContent {
            url: "https://example.com/baking".to_string(),
            title: "How to Bake Bread".to_string(),
            description: "".to_string(),
            content_markdown: "To bake delicious bread at home, you need flour, water, yeast, and salt. Kneading the dough is crucial.".to_string(),
            links: vec![],
        };

        index.add_document(&doc1, "test").await.unwrap();
        index.add_document(&doc2, "test").await.unwrap();

        // Force reload reader.
        index.reader.reload().unwrap();

        // 1. Keyword-based search
        let kw_results = index.search("bread", 5).unwrap();
        assert!(!kw_results.is_empty());
        assert_eq!(kw_results[0].title, "How to Bake Bread");

        // 2. Semantic search
        let sem_results = index.search_semantic("subatomic physics", 5).unwrap();
        assert!(!sem_results.is_empty());
        assert_eq!(sem_results[0].title, "Quantum Computing Foundations");

        let sem_results_recipe = index.search_semantic("culinary dough recipe", 5).unwrap();
        assert!(!sem_results_recipe.is_empty());
        assert_eq!(sem_results_recipe[0].title, "How to Bake Bread");

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[tokio::test]
    async fn test_list_documents() {
        let test_dir =
            std::env::temp_dir().join(format!("searchxyz_test_list_{}", rand::random::<u64>()));
        let _ = std::fs::remove_dir_all(&test_dir);

        let config = IndexConfig {
            path: test_dir.clone(),
            writer_heap_bytes: 15_000_000,
        };

        let index = SearchIndex::open(&config).unwrap();

        let doc1 = ExtractedContent {
            url: "https://example.com/a".to_string(),
            title: "Doc A".to_string(),
            description: "".to_string(),
            content_markdown: "Some content".to_string(),
            links: vec![],
        };

        let doc2 = ExtractedContent {
            url: "https://example.com/b".to_string(),
            title: "Doc B".to_string(),
            description: "".to_string(),
            content_markdown: "Some other content".to_string(),
            links: vec![],
        };

        index.add_document(&doc1, "sourcea").await.unwrap();
        index.add_document(&doc2, "sourceb").await.unwrap();

        index.reader.reload().unwrap();

        // Test listing all
        let (all_docs, count) = index.list_documents(None, 5, 0).unwrap();
        assert_eq!(count, 2);
        assert_eq!(all_docs.len(), 2);

        // Test filtering by source
        let (filtered_docs, filtered_count) = index.list_documents(Some("sourcea"), 5, 0).unwrap();
        assert_eq!(filtered_count, 1);
        assert_eq!(filtered_docs[0].title, "Doc A");

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[tokio::test]
    async fn test_export_documents() {
        let test_dir =
            std::env::temp_dir().join(format!("searchxyz_test_export_{}", rand::random::<u64>()));
        let _ = std::fs::remove_dir_all(&test_dir);

        let config = IndexConfig {
            path: test_dir.clone(),
            writer_heap_bytes: 15_000_000,
        };

        let index = SearchIndex::open(&config).unwrap();

        let doc1 = ExtractedContent {
            url: "https://example.com/rust".to_string(),
            title: "Rust Programming Language".to_string(),
            description: "".to_string(),
            content_markdown: "Rust is a multi-paradigm, general-purpose programming language designed for performance and safety.".to_string(),
            links: vec![],
        };

        index.add_document(&doc1, "test").await.unwrap();
        index.reader.reload().unwrap();

        // Test export all
        let docs = index.export_documents(None, 5).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "Rust Programming Language");
        assert_eq!(docs[0].content_markdown, "Rust is a multi-paradigm, general-purpose programming language designed for performance and safety.");

        // Test export with query
        let docs_query = index.export_documents(Some("performance"), 5).unwrap();
        assert_eq!(docs_query.len(), 1);

        let docs_empty = index.export_documents(Some("nonexistent"), 5).unwrap();
        assert_eq!(docs_empty.len(), 0);

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[tokio::test]
    async fn test_markdown_chunking_logic() {
        let content =
            "Paragraph 1\n\n# Header 1\nSection 1 text here.\n\n## Header 2\nSection 2 text here.";
        let chunks = chunk_content(content, 1500, 200);
        assert_eq!(chunks.len(), 3);
        assert!(chunks[0].contains("Paragraph 1"));
        assert!(chunks[1].contains("# Header 1"));
        assert!(chunks[2].contains("## Header 2"));
    }

    #[tokio::test]
    async fn test_prefix_deletion() {
        let test_dir = std::env::temp_dir().join(format!(
            "searchxyz_test_prefix_del_{}",
            rand::random::<u64>()
        ));
        let config = IndexConfig {
            path: test_dir.clone(),
            writer_heap_bytes: 15_000_000,
        };
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
        index
            .delete_document("https://example.com/prefix-del")
            .await
            .unwrap();
        index.reload().unwrap();

        let results = index.search("Prefix", 10).unwrap();
        assert_eq!(results.len(), 0);
        let _ = std::fs::remove_dir_all(&test_dir);
    }
}
