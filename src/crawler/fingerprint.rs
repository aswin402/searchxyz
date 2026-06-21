use rand::seq::IndexedRandom;

pub struct HeaderGenerator;

static USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/120.0",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2.1 Safari/605.1.15",
];

impl HeaderGenerator {
    pub fn random_headers() -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        let mut rng = rand::rng();

        let ua = USER_AGENTS.choose(&mut rng).unwrap_or(&USER_AGENTS[0]);

        headers.insert(reqwest::header::USER_AGENT, ua.parse().unwrap());
        headers.insert(
            reqwest::header::ACCEPT,
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8"
                .parse()
                .unwrap(),
        );
        headers.insert(
            reqwest::header::ACCEPT_LANGUAGE,
            "en-US,en;q=0.5".parse().unwrap(),
        );
        headers.insert(
            reqwest::header::ACCEPT_ENCODING,
            "gzip, deflate, br".parse().unwrap(),
        );

        headers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_headers() {
        let headers = HeaderGenerator::random_headers();
        assert!(headers.contains_key(reqwest::header::USER_AGENT));
        assert!(headers.contains_key(reqwest::header::ACCEPT));
        assert!(headers.contains_key(reqwest::header::ACCEPT_LANGUAGE));
        assert!(headers.contains_key(reqwest::header::ACCEPT_ENCODING));

        let ua = headers
            .get(reqwest::header::USER_AGENT)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(USER_AGENTS.contains(&ua));
    }
}
