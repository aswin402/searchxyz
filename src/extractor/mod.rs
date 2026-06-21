use scraper::{Html, Selector};

use crate::config::ExtractorConfig;
use crate::error::SearchXyzError;

/// Extracted content from a crawled page.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtractedContent {
    pub url: String,
    pub title: String,
    pub description: String,
    pub content_markdown: String,
}

/// Pipeline that converts raw HTML into clean markdown text.
pub struct ExtractionPipeline {
    config: ExtractorConfig,
    // Pre-compiled selectors for performance.
    strip_selectors: Vec<Selector>,
    content_selectors: Vec<Selector>,
}

impl ExtractionPipeline {
    pub fn new(config: ExtractorConfig) -> Self {
        let strip_selectors = config
            .strip_selectors
            .iter()
            .filter_map(|s| Selector::parse(s).ok())
            .collect();

        let content_selectors = config
            .content_selectors
            .iter()
            .filter_map(|s| Selector::parse(s).ok())
            .collect();

        Self {
            config,
            strip_selectors,
            content_selectors,
        }
    }

    /// Extract readable content from raw HTML.
    pub fn extract(
        &self,
        url: &str,
        html: &str,
    ) -> Result<ExtractedContent, SearchXyzError> {
        let document = Html::parse_document(html);

        // ── 1. Extract metadata ──
        let title = self.extract_title(&document);
        let description = self.extract_meta_description(&document);

        // ── 2. Find the main content element ──
        // Try priority selectors in order; fall back to <body>.
        let content_html = self
            .find_main_content(&document)
            .unwrap_or_else(|| self.extract_body_text(&document));

        // ── 3. Strip noisy elements from extracted HTML ──
        let cleaned = self.strip_noise(&content_html);

        // ── 4. Convert to markdown-like plain text ──
        let markdown = self.html_to_markdown(&cleaned);

        // ── 5. Validate length ──
        if markdown.trim().len() < self.config.min_content_length {
            return Err(SearchXyzError::EmptyContent {
                url: url.into(),
                min_length: self.config.min_content_length,
            });
        }

        Ok(ExtractedContent {
            url: url.into(),
            title,
            description,
            content_markdown: markdown,
        })
    }

    // ── Private helpers ──────────────────────────────────────

    fn extract_title(&self, doc: &Html) -> String {
        let sel = Selector::parse("title").unwrap();
        doc.select(&sel)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default()
    }

    fn extract_meta_description(&self, doc: &Html) -> String {
        let sel = Selector::parse(r#"meta[name="description"]"#).unwrap();
        doc.select(&sel)
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }

    /// Walk priority selectors and return the first match's inner HTML.
    fn find_main_content(&self, doc: &Html) -> Option<String> {
        for sel in &self.content_selectors {
            if let Some(el) = doc.select(sel).next() {
                return Some(el.inner_html());
            }
        }
        None
    }

    /// Fallback: grab all text inside <body>.
    fn extract_body_text(&self, doc: &Html) -> String {
        let sel = Selector::parse("body").unwrap();
        doc.select(&sel)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_default()
    }

    /// Remove noisy elements (script, style, nav, etc.) from an
    /// HTML fragment string.
    fn strip_noise(&self, html: &str) -> String {
        let fragment = Html::parse_fragment(html);
        let mut output = String::new();

        // Collect node IDs to skip (elements matching strip selectors).
        let skip_ids: std::collections::HashSet<_> = self
            .strip_selectors
            .iter()
            .flat_map(|sel| fragment.select(sel))
            .map(|el| el.id())
            .collect();

        // Walk the tree and emit text for non-skipped nodes.
        fn collect_text(
            node: ego_tree::NodeRef<scraper::Node>,
            skip: &std::collections::HashSet<ego_tree::NodeId>,
            out: &mut String,
        ) {
            if skip.contains(&node.id()) {
                return;
            }
            match node.value() {
                scraper::Node::Text(t) => {
                    let text = t.text.trim();
                    if !text.is_empty() {
                        out.push_str(text);
                        out.push(' ');
                    }
                }
                scraper::Node::Element(el) => {
                    // Add line breaks for block elements.
                    let tag = el.name();
                    let is_block = matches!(
                        tag,
                        "p" | "div" | "br" | "h1" | "h2" | "h3"
                            | "h4" | "h5" | "h6" | "li" | "tr"
                            | "blockquote" | "pre" | "hr"
                    );
                    if is_block {
                        out.push('\n');
                    }
                    // Add markdown heading prefix.
                    match tag {
                        "h1" => out.push_str("# "),
                        "h2" => out.push_str("## "),
                        "h3" => out.push_str("### "),
                        "h4" => out.push_str("#### "),
                        _ => {}
                    }
                    for child in node.children() {
                        collect_text(child, skip, out);
                    }
                    if is_block {
                        out.push('\n');
                    }
                }
                _ => {
                    for child in node.children() {
                        collect_text(child, skip, out);
                    }
                }
            }
        }

        if let Some(root) = fragment.tree.root().children().next() {
            collect_text(root, &skip_ids, &mut output);
        }

        output
    }

    /// Normalise whitespace for the final markdown output.
    fn html_to_markdown(&self, text: &str) -> String {
        // Collapse runs of whitespace; normalise line breaks.
        let mut result = String::with_capacity(text.len());
        let mut prev_blank = false;

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !prev_blank {
                    result.push('\n');
                    prev_blank = true;
                }
            } else {
                result.push_str(trimmed);
                result.push('\n');
                prev_blank = false;
            }
        }

        result.trim().to_string()
    }
}
