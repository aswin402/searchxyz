use crate::crawler::Crawler;
use crate::error::SearchXyzError;

/// Extract the 11-character video ID from various YouTube URL formats.
pub fn extract_video_id(url: &str) -> Option<String> {
    if let Ok(parsed) = url::Url::parse(url) {
        let host = parsed.host_str().unwrap_or("");
        if host == "www.youtube.com" || host == "youtube.com" || host == "m.youtube.com" {
            for (k, v) in parsed.query_pairs() {
                if k == "v" {
                    return Some(v.into_owned());
                }
            }
        } else if host == "youtu.be" {
            let path = parsed.path().trim_start_matches('/');
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    None
}

/// Find the transcript XML timedtext track URL from the video HTML source.
pub fn find_timedtext_url(html: &str, lang: &str) -> Option<String> {
    let marker = "ytInitialPlayerResponse = ";
    let start_idx = html.find(marker)?;
    let sub = &html[start_idx + marker.len()..];

    // Find the end of the JSON block
    let end_idx = sub
        .find(";var ")
        .or_else(|| sub.find(";</script>"))
        .or_else(|| sub.find("</script>"))?;
    let mut json_str = sub[..end_idx].trim();
    if json_str.ends_with(';') {
        json_str = json_str[..json_str.len() - 1].trim();
    }

    let json: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let caption_tracks = json
        .get("captions")?
        .get("playerCaptionsTracklistRenderer")?
        .get("captionTracks")?
        .as_array()?;

    let mut selected_url = None;
    for track in caption_tracks {
        if let Some(code) = track.get("languageCode").and_then(|v| v.as_str()) {
            if code == lang {
                selected_url = track
                    .get("baseUrl")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                break;
            }
        }
    }

    if selected_url.is_none() && !caption_tracks.is_empty() {
        selected_url = caption_tracks[0]
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }

    selected_url
}

/// Decode basic HTML entities inside subtitle text.
pub fn decode_html_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// Parse the timed text XML format and merge lines into a continuous transcript.
pub fn parse_xml_transcript(xml: &str) -> String {
    let mut transcript = Vec::new();
    let mut cursor = xml;
    while let Some(start_idx) = cursor.find("<text") {
        let sub = &cursor[start_idx..];
        if let Some(end_tag_idx) = sub.find('>') {
            let body_sub = &sub[end_tag_idx + 1..];
            if let Some(close_idx) = body_sub.find("</text>") {
                let raw_text = &body_sub[..close_idx].trim();
                if !raw_text.is_empty() {
                    let decoded = decode_html_entities(raw_text);
                    // Strip nested HTML tags if any (e.g. <b> or <font>)
                    let cleaned = strip_tags(&decoded);
                    if !cleaned.is_empty() {
                        transcript.push(cleaned);
                    }
                }
                cursor = &body_sub[close_idx + 7..];
            } else {
                break;
            }
        } else {
            break;
        }
    }
    transcript.join(" ")
}

fn strip_tags(input: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for c in input.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            output.push(c);
        }
    }
    output
}

/// Main orchestrator: extracts video ID, fetches page, extracts caption URL, fetches and parses captions.
pub async fn fetch_youtube_transcript(
    crawler: &Crawler,
    url: &str,
) -> Result<String, SearchXyzError> {
    let video_id = extract_video_id(url).ok_or_else(|| SearchXyzError::CrawlFailed {
        url: url.to_string(),
        reason: "Invalid YouTube URL: could not parse Video ID".to_string(),
    })?;

    let watch_url = format!("https://www.youtube.com/watch?v={}", video_id);
    let result = crawler.fetch_url(&watch_url, false).await?;

    let timedtext_url =
        find_timedtext_url(&result.body, "en").ok_or_else(|| SearchXyzError::CrawlFailed {
            url: url.to_string(),
            reason: "No captions or transcript tracks found for this video".to_string(),
        })?;

    let xml_result = crawler.fetch_url(&timedtext_url, false).await?;
    let transcript = parse_xml_transcript(&xml_result.body);

    if transcript.is_empty() {
        return Err(SearchXyzError::CrawlFailed {
            url: url.to_string(),
            reason: "Transcript download resulted in empty content".to_string(),
        });
    }

    Ok(transcript)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_video_id() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            extract_video_id("https://youtube.com/watch?v=dQw4w9WgXcQ&feature=share"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            extract_video_id("https://youtu.be/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            extract_video_id("https://example.com/watch?v=dQw4w9WgXcQ"),
            None
        );
    }

    #[test]
    fn test_parse_xml_transcript() {
        let xml = r#"
            <?xml version="1.0" encoding="utf-8" ?>
            <transcript>
                <text start="0.0" dur="2.0">Hello &amp; welcome!</text>
                <text start="2.0" dur="1.5">This is <b>subtitles</b>.</text>
                <text start="3.5" dur="1.0"></text>
            </transcript>
        "#;
        assert_eq!(
            parse_xml_transcript(xml),
            "Hello & welcome! This is subtitles."
        );
    }

    #[tokio::test]
    async fn test_fetch_youtube_transcript_cached() {
        use crate::cache::{Cache, CacheEntry};
        use crate::config::{CrawlerConfig, HeadlessConfig, ProxyConfig};
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let cache = Arc::new(Mutex::new(Cache::new(100, 3600)));
        {
            let mut c = cache.lock().await;
            // Simulated watch page html
            c.put(
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
                CacheEntry::new(
                    r#"
                    <html>
                        <body>
                            <script>
                                var ytInitialPlayerResponse = {
                                    "captions": {
                                        "playerCaptionsTracklistRenderer": {
                                            "captionTracks": [
                                                {
                                                    "baseUrl": "https://www.youtube.com/api/timedtext?v=dQw4w9WgXcQ&lang=en",
                                                    "languageCode": "en"
                                                }
                                            ]
                                        }
                                    }
                                };
                            </script>
                        </body>
                    </html>
                    "#.to_string(),
                    "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
                ),
            );
            // Simulated XML captions response
            c.put(
                "https://www.youtube.com/api/timedtext?v=dQw4w9WgXcQ&lang=en".to_string(),
                CacheEntry::new(
                    r#"
                    <?xml version="1.0" encoding="utf-8" ?>
                    <transcript>
                        <text start="0.0">Never gonna give you up</text>
                    </transcript>
                    "#
                    .to_string(),
                    "https://www.youtube.com/api/timedtext?v=dQw4w9WgXcQ&lang=en".to_string(),
                ),
            );
        }

        let crawler = Crawler::new(
            CrawlerConfig::default(),
            HeadlessConfig::default(),
            ProxyConfig::default(),
            cache,
        );

        let transcript =
            fetch_youtube_transcript(&crawler, "https://www.youtube.com/watch?v=dQw4w9WgXcQ")
                .await
                .unwrap();
        assert_eq!(transcript, "Never gonna give you up");
    }
}
