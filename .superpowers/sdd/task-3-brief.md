### Task 3: Embedding Model Customization (Local/Cloud)

**Files:**
- Modify: `src/config/mod.rs`
- Modify: `src/index/mod.rs`
- Test: Add embedding generator tests to `src/index/mod.rs` (testing local embedding as well as mocked cloud endpoints via a local test server).

**Interfaces:**
- Consumes: `EmbeddingConfig` config schemas.
- Produces: `EmbeddingGenerator` enum inside `src/index/mod.rs` which dynamically generates embeddings based on the provider configuration.

- [ ] **Step 1: Write the failing tests**
  Add unit tests in `src/index/mod.rs`'s `tests` module to verify configuration loading and mock embedding generation for `openai`, `gemini`, and `cohere`.
  - The tests should start a local dummy HTTP server (using `tokio::net::TcpListener` or similar), configure `api_url` pointing to it, and mock responses.
  - Verify that query vectors match the dimensions dynamically returned by the mock server or provider.

- [ ] **Step 2: Run test to verify it fails**
  Run: `OPENSSL_VENDORED=1 cargo test`
  Expected: FAIL (types/functions undefined)

- [ ] **Step 3: Write minimal implementation**
  1. Define `EmbeddingConfig` and add it to `IndexConfig` inside `src/config/mod.rs`:
     ```rust
     #[derive(Debug, Deserialize, Clone)]
     #[serde(default)]
     pub struct EmbeddingConfig {
         pub provider: String, // "local", "openai", "gemini", "cohere"
         pub model: String,
         pub api_key: Option<String>,
         pub api_url: Option<String>,
     }
     ```
     Provide environment variables overrides for the API keys (e.g. `SEARCHXYZ_OPENAI_API_KEY`, etc.) inside `apply_env_overrides`.
  2. Implement `EmbeddingGenerator` enum inside `src/index/mod.rs`:
     ```rust
     pub enum EmbeddingGenerator {
         Local(std::sync::Mutex<TextEmbedding>),
         OpenAi { client: reqwest::Client, model: String, api_key: String, api_url: String },
         Gemini { client: reqwest::Client, model: String, api_key: String, api_url: String },
         Cohere { client: reqwest::Client, model: String, api_key: String, api_url: String },
     }
     ```
  3. Implement `EmbeddingGenerator::embed(&self, texts: Vec<&str>, is_query: bool) -> Result<Vec<Vec<f32>>, SearchXyzError>`:
     - For `Local`: Format text with `passage: ` or `query: ` prefixes as before.
     - For `OpenAi`: Send a POST request to `api_url` (defaulting to `https://api.openai.com/v1/embeddings`), with Bearer Token auth. Request structure: `{"input": texts, "model": model}`. Response format: extract `data[i].embedding`.
     - For `Gemini`: Send a POST request to Gemini's API. Note that Gemini can embed multiple contents using the `batchEmbedContents` endpoint or single using `embedContent`. Let's support batching:
       - Endpoint for batch: `https://generativelanguage.googleapis.com/v1beta/models/<model>:batchEmbedContents?key=<api_key>` (or overriding base URL if `api_url` is specified).
       - Request body:
         ```json
         {
           "requests": [
             { "model": "models/<model>", "content": { "parts": [{ "text": text1 }] } },
             ...
           ]
         }
         ```
       - Response format: extract `embeddings[i].values`.
     - For `Cohere`: Send a POST request to Cohere's API: `https://api.cohere.com/v1/embed`.
       - Request body: `{"texts": texts, "model": model, "input_type": if is_query { "search_query" } else { "search_document" }}`.
       - Response format: extract `embeddings[i]`.
  4. Replace `embedding_model: std::sync::Mutex<TextEmbedding>` in `SearchIndex` with `embedding_generator: EmbeddingGenerator`.
  5. Adjust vector search logic in `search_semantic` to read embeddings query dimension dynamically (since local models produce 384 dimensions, and OpenAI can produce 1536, etc.). The L2 distance comparison and query index must match the size of the vectors generated.

- [ ] **Step 4: Run test to verify it passes**
  Run: `OPENSSL_VENDORED=1 cargo test`
  Expected: PASS

- [ ] **Step 5: Commit**
  Run:
  ```bash
  git add src/config/mod.rs src/index/mod.rs
  git commit -m "feat: implement local/cloud custom embedding models configuration and HTTP api dispatchers"
  ```
