use std::sync::Arc;

use chrono::Utc;
use tantivy::{
    collector::TopDocs,
    directory::MmapDirectory,
    query::QueryParser,
    schema::*,
    Index, IndexReader, IndexWriter, ReloadPolicy,
    doc,
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
    // Schema field handles — kept for building docs & queries.
    f_url: Field,
    f_title: Field,
    f_content: Field,
    f_source: Field,
    f_indexed_at: Field,
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

impl SearchIndex {
    /// Open or create the index at the configured path.
    pub fn open(config: &IndexConfig) -> Result<Self, SearchXyzError> {
        // Ensure directory exists.
        std::fs::create_dir_all(&config.path)?;

        // ── Build schema ──
        let mut builder = Schema::builder();

        let f_url = builder.add_text_field("url", TEXT | STORED);
        let f_title = builder.add_text_field("title", TEXT | STORED);
        let f_content = builder.add_text_field("content", TEXT);
        let f_source = builder.add_text_field("source", TEXT | STORED);
        let f_indexed_at = builder.add_date_field(
            "indexed_at",
            INDEXED | STORED,
        );

        let schema = builder.build();

        // ── Open or create index ──
        let dir = MmapDirectory::open(&config.path)
            .map_err(|e| SearchXyzError::IndexError(format!(
                "Failed to open index directory: {e}"
            )))?;

        let index = Index::open_or_create(dir, schema.clone())
            .map_err(|e| SearchXyzError::IndexError(format!(
                "Failed to open/create index: {e}"
            )))?;

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
            .map_err(|e| SearchXyzError::IndexError(format!(
                "Failed to create writer: {e}"
            )))?;

        Ok(Self {
            index,
            reader,
            writer: Arc::new(Mutex::new(writer)),
            f_url,
            f_title,
            f_content,
            f_source,
            f_indexed_at,
        })
    }

    /// Index a piece of extracted content.
    pub async fn add_document(
        &self,
        content: &ExtractedContent,
        source: &str,
    ) -> Result<(), SearchXyzError> {
        let now = tantivy::DateTime::from_timestamp_secs(Utc::now().timestamp());

        let mut writer = self.writer.lock().await;
        
        // Remove existing document with same URL to avoid duplicates.
        let term = tantivy::Term::from_field_text(self.f_url, &content.url);
        writer.delete_term(term);

        writer.add_document(doc!(
            self.f_url     => content.url.clone(),
            self.f_title   => content.title.clone(),
            self.f_content => content.content_markdown.clone(),
            self.f_source  => source.to_string(),
            self.f_indexed_at => now,
        ))?;
        writer.commit()?;

        tracing::debug!(url = %content.url, "Indexed document");

        Ok(())
    }

    /// Full-text search across indexed content.
    pub fn search(
        &self,
        query_str: &str,
        max_results: usize,
    ) -> Result<Vec<IndexSearchResult>, SearchXyzError> {
        let searcher = self.reader.searcher();

        let query_parser = QueryParser::for_index(
            &self.index,
            vec![self.f_title, self.f_content],
        );

        let query = query_parser
            .parse_query(query_str)
            .map_err(|e| SearchXyzError::IndexError(format!(
                "Failed to parse query `{query_str}`: {e}"
            )))?;

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(max_results))
            .map_err(|e| SearchXyzError::IndexError(format!(
                "Search execution failed: {e}"
            )))?;

        // ── Build snippet generator for content field ──
        let snippet_generator =
            tantivy::SnippetGenerator::create(&searcher, &query, self.f_content)
                .map_err(|e| SearchXyzError::IndexError(format!(
                    "Snippet generator failed: {e}"
                )))?;

        let mut results = Vec::with_capacity(top_docs.len());

        for (score, doc_address) in top_docs {
            let doc: tantivy::TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| SearchXyzError::IndexError(format!(
                    "Failed to retrieve doc: {e}"
                )))?;

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

            let snippet = snippet_generator
                .snippet_from_doc(&doc)
                .to_html();

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

    /// Delete all documents matching a URL.
    pub async fn delete_by_url(
        &self,
        url: &str,
    ) -> Result<(), SearchXyzError> {
        let term = tantivy::Term::from_field_text(self.f_url, url);
        let mut writer = self.writer.lock().await;
        writer.delete_term(term);
        writer.commit()?;
        tracing::debug!(url, "Deleted from index");
        Ok(())
    }
}
