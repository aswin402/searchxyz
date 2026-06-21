use async_trait::async_trait;
use reqwest::Client;
use scraper::{Html, Selector};

use super::{SearchBackend, SearchQuery, SearchResult};
use crate::crawler::fingerprint::HeaderGenerator;
use crate::error::SearchXyzError;

/// Native Google Scraper Backend — no API key required.
pub struct GoogleBackend {
    client: Client,
}

impl GoogleBackend {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    fn parse_results(html_body: &str, max_results: usize) -> Vec<SearchResult> {
        let document = Html::parse_document(html_body);

        // Google organic result containers
        let container_sel =
            Selector::parse("div.g, div.MjjYud, div[jscontroller][data-hveid][data-ved]").unwrap();
        let containers: Vec<_> = document.select(&container_sel).collect();

        let mut results = Vec::new();

        if !containers.is_empty() {
            let title_sel = Selector::parse("h3").unwrap();
            let link_sel = Selector::parse("a[href]").unwrap();

            // Google snippet selectors:
            // .VwiC3b, .yDAB2e, .BNeawe, div[style*="-webkit-line-clamp"]
            let snippet_selectors = vec![
                Selector::parse(".VwiC3b").unwrap(),
                Selector::parse(".yDAB2e").unwrap(),
                Selector::parse(".BNeawe").unwrap(),
                Selector::parse("div[style*='-webkit-line-clamp']").unwrap(),
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

                let title = title_el
                    .text()
                    .collect::<Vec<_>>()
                    .join("")
                    .trim()
                    .to_string();
                if title.is_empty() {
                    continue;
                }

                // Find the first href anchor in the container (often enclosing h3 or nearby)
                let url = match container.select(&link_sel).next() {
                    Some(el) => match el.value().attr("href") {
                        Some(href) => {
                            if href.starts_with("http") {
                                href.to_string()
                            } else if href.starts_with("/url?q=") {
                                if let Ok(parsed) =
                                    url::Url::parse(&format!("https://www.google.com{}", href))
                                {
                                    let mut target = None;
                                    for (k, v) in parsed.query_pairs() {
                                        if k == "q" {
                                            target = Some(v.into_owned());
                                            break;
                                        }
                                    }
                                    if let Some(t) = target {
                                        t
                                    } else {
                                        href.to_string()
                                    }
                                } else {
                                    href.to_string()
                                }
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
                        snippet = snippet_el
                            .text()
                            .collect::<Vec<_>>()
                            .join("")
                            .trim()
                            .to_string();
                        if !snippet.is_empty() {
                            break;
                        }
                    }
                }

                // If snippet is still empty, let's grab text from the container omitting the title.
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
                    source: "google".into(),
                });
            }
        } else {
            // Fallback: If no containers found, try finding h3s directly
            let title_sel = Selector::parse("h3").unwrap();
            let h3s: Vec<_> = document.select(&title_sel).collect();
            for h3_el in h3s {
                if results.len() >= max_results {
                    break;
                }

                let title = h3_el.text().collect::<Vec<_>>().join("").trim().to_string();
                if title.is_empty() {
                    continue;
                }

                // Walk up the tree to find an ancestor anchor link
                let mut current = h3_el.parent();
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
                        source: "google".into(),
                    });
                }
            }
        }

        results
    }
}

#[async_trait]
impl SearchBackend for GoogleBackend {
    fn name(&self) -> &str {
        "google"
    }

    fn is_available(&self) -> bool {
        true // no key needed
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>, SearchXyzError> {
        let resp = self
            .client
            .get("https://www.google.com/search")
            .query(&[("q", &query.query)])
            .headers(HeaderGenerator::random_headers())
            .send()
            .await
            .map_err(|e| SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: format!("Google HTTP request failed: {e}"),
            })?;

        if !resp.status().is_success() {
            return Err(SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: format!("Google returned HTTP {}", resp.status()),
            });
        }

        let html_body = resp
            .text()
            .await
            .map_err(|e| SearchXyzError::SearchFailed {
                query: query.query.clone(),
                reason: format!("Failed to read Google response body: {e}"),
            })?;

        let results = Self::parse_results(&html_body, query.max_results);

        if results.is_empty() {
            tracing::warn!(query = %query.query, "Google returned no parsable results");
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_google_parsing() {
        let html = r#"
            <html>
            <body>
                <div class="g">
                    <h3><a href="https://rust-lang.org">Rust Programming Language</a></h3>
                    <div class="VwiC3b">Safe, fast, productive language.</div>
                </div>
                <div class="MjjYud">
                    <h3><a href="https://github.com/rust-lang/rust">Rust Github Repo</a></h3>
                    <div class="yDAB2e">Source code repository for Rust.</div>
                </div>
            </body>
            </html>
        "#;
        let results = GoogleBackend::parse_results(html, 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Rust Programming Language");
        assert_eq!(results[0].url, "https://rust-lang.org");
        assert_eq!(results[0].snippet, "Safe, fast, productive language.");
        assert_eq!(results[0].source, "google");

        assert_eq!(results[1].title, "Rust Github Repo");
        assert_eq!(results[1].url, "https://github.com/rust-lang/rust");
        assert_eq!(results[1].snippet, "Source code repository for Rust.");
    }
}
