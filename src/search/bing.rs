use async_trait::async_trait;
use reqwest::Client;
use scraper::{Html, Selector};

use crate::error::SearchXyzError;
use crate::crawler::fingerprint::HeaderGenerator;
use super::{SearchBackend, SearchQuery, SearchResult};

/// Native Bing Scraper Backend — no API key required.
pub struct BingBackend {
    client: Client,
}

impl BingBackend {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    fn parse_results(html_body: &str, max_results: usize) -> Vec<SearchResult> {
        let document = Html::parse_document(html_body);

        // Bing organic result containers are typically li.b_algo
        let container_sel = Selector::parse("li.b_algo").unwrap();
        let containers: Vec<_> = document.select(&container_sel).collect();

        let mut results = Vec::new();

        if !containers.is_empty() {
            let title_sel = Selector::parse("h2").unwrap();
            let link_sel = Selector::parse("a[href]").unwrap();
            
            // Bing snippet selectors
            let snippet_selectors = vec![
                Selector::parse(".b_caption p").unwrap(),
                Selector::parse(".b_caption").unwrap(),
                Selector::parse(".b_snippet").unwrap(),
                Selector::parse("p").unwrap(),
            ];

            for container in containers {
                if results.len() >= max_results {
                    break;
                }

                // Check if this container has a title
                let title_el = match container.select(&title_sel).next() {
                    Some(el) => el,
                    None => continue,
                };

                let title = title_el.text().collect::<Vec<_>>().join("").trim().to_string();
                if title.is_empty() {
                    continue;
                }

                // Find the first href anchor in the container (often enclosing h2 or nearby)
                let url = match container.select(&link_sel).next() {
                    Some(el) => match el.value().attr("href") {
                        Some(href) => {
                            if href.starts_with("http") {
                                href.to_string()
                            } else {
                                continue;
                            }
                        }
                        None => continue,
                    },
                    None => continue,
                };

                // Extract snippet
                let mut snippet = String::new();
                for snippet_sel in &snippet_selectors {
                    if let Some(snippet_el) = container.select(snippet_sel).next() {
                        snippet = snippet_el.text().collect::<Vec<_>>().join("").trim().to_string();
                        if !snippet.is_empty() {
                            break;
                        }
                    }
                }

                // Fallback snippet
                if snippet.is_empty() {
                    snippet = container.text().collect::<Vec<_>>().join(" ");
                    snippet = snippet.replace(&title, "").trim().to_string();
                    if snippet.len() > 200 {
                        snippet.truncate(200);
                        snippet.push_str("...");
                    }
                }

                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                    source: "bing".into(),
                });
            }
        } else {
            // Fallback: search for h2s directly
            let title_sel = Selector::parse("h2").unwrap();
            let h2s: Vec<_> = document.select(&title_sel).collect();
            for h2_el in h2s {
                if results.len() >= max_results {
                    break;
                }
                
                let title = h2_el.text().collect::<Vec<_>>().join("").trim().to_string();
                if title.is_empty() {
                    continue;
                }

                let mut current = h2_el.parent();
                let mut url = None;
                for _ in 0..5 {
                    if let Some(node) = current {
                        if let Some(el) = scraper::ElementRef::wrap(node) {
                            if el.value().name() == "a" {
                                if let Some(href) = el.value().attr("href") {
                                    if href.starts_with("http") {
                                        url = Some(href.to_string());
                                        break;
                                    }
                                }
                            }
                        }
                        current = node.parent();
                    } else {
                        break;
                    }
                }

                if let Some(url) = url {
                    results.push(SearchResult {
                        title,
                        url,
                        snippet: String::new(),
                        source: "bing".into(),
                    });
                }
            }
        }

        results
    }
}

#[async_trait]
impl SearchBackend for BingBackend {
    fn name(&self) -> &str {
        "bing"
    }

    fn is_available(&self) -> bool {
        true // no key needed
    }

    async fn search(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<SearchResult>, SearchXyzError> {
        let resp = self
            .client
            .get("https://www.bing.com/search")
            .query(&[("q", &query.query)])
            .headers(HeaderGenerator::random_headers())
            .send()
            .await
            .map_err(|e| SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: format!("Bing HTTP request failed: {e}"),
            })?;

        if !resp.status().is_success() {
            return Err(SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: format!("Bing returned HTTP {}", resp.status()),
            });
        }

        let html_body = resp.text().await.map_err(|e| {
            SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: format!("Failed to read Bing response body: {e}"),
            }
        })?;

        let results = Self::parse_results(&html_body, query.max_results);

        if results.is_empty() {
            tracing::warn!(query = %query.query, "Bing returned no parsable results");
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bing_parsing() {
        let html = r#"
            <html>
            <body>
                <ol id="b_results">
                    <li class="b_algo">
                        <h2><a href="https://rust-lang.org">Rust Programming Language</a></h2>
                        <div class="b_caption">
                            <p>Safe, fast, productive language.</p>
                        </div>
                    </li>
                    <li class="b_algo">
                        <h2><a href="https://github.com/rust-lang/rust">Rust Github Repo</a></h2>
                        <div class="b_snippet">Source code repository for Rust.</div>
                    </li>
                </ol>
            </body>
            </html>
        "#;
        let results = BingBackend::parse_results(html, 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Rust Programming Language");
        assert_eq!(results[0].url, "https://rust-lang.org");
        assert_eq!(results[0].snippet, "Safe, fast, productive language.");
        assert_eq!(results[0].source, "bing");

        assert_eq!(results[1].title, "Rust Github Repo");
        assert_eq!(results[1].url, "https://github.com/rust-lang/rust");
        assert_eq!(results[1].snippet, "Source code repository for Rust.");
    }
}
