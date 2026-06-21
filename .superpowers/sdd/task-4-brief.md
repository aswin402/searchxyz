### Task 4: Bearer Token SSE HTTP Authentication Middleware

**Files:**
- Modify: `src/config/mod.rs`
- Modify: `src/main.rs`
- Test: Add integration tests in `src/main.rs` to verify authorization blocks/allows client requests.

**Interfaces:**
- Consumes: `Config::server::auth_token` configuration.
- Produces: Axum authentication middleware checking the `Authorization` header for bearer token.

- [ ] **Step 1: Write the failing tests**
  Add a unit/integration test in `src/main.rs` or `src/config/mod.rs` verifying that if `auth_token` is enabled:
  - Unauthorized requests to the Axum router (mismatched/missing bearer token) return `401 Unauthorized`.
  - Authorized requests (with `Bearer <expected_token>`) are allowed through.
  (Since HTTP server is optional, we can test the router/middleware directly by sending mock requests via `axum::Router::oneshot` without starting a real TCP listener, or via standard integration test).

- [ ] **Step 2: Run test to verify it fails**
  Run: `OPENSSL_VENDORED=1 cargo test`
  Expected: FAIL

- [ ] **Step 3: Write minimal implementation**
  1. Add `auth_token: Option<String>` to `ServerConfig` in `src/config/mod.rs`, set default to `None` in `Default for ServerConfig`, and read `SEARCHXYZ_API_KEY` inside `apply_env_overrides`.
  2. Implement `auth_middleware` in `src/main.rs`:
     ```rust
     async fn auth_middleware(
         axum::extract::State(expected_token): axum::extract::State<String>,
         req: axum::extract::Request,
         next: axum::middleware::Next,
     ) -> Result<axum::response::Response, axum::http::StatusCode> {
         let auth_header = req.headers()
             .get(axum::http::header::AUTHORIZATION)
             .and_then(|h| h.to_str().ok());
         
         if let Some(auth) = auth_header {
             if auth.starts_with("Bearer ") && &auth[7..] == expected_token {
                 return Ok(next.run(req).await);
             }
         }
         Err(axum::http::StatusCode::UNAUTHORIZED)
     }
     ```
  3. Register the middleware on the `app` router in `src/main.rs` using `axum::middleware::from_fn_with_state` if `config.server.auth_token` is configured.
     Note: Make sure to import `axum::extract::State`.

- [ ] **Step 4: Run test to verify it passes**
  Run: `OPENSSL_VENDORED=1 cargo test`
  Expected: PASS

- [ ] **Step 5: Commit**
  Run:
  ```bash
  git add src/config/mod.rs src/main.rs
  git commit -m "feat: add Bearer Token Axum HTTP authentication layer for SSE remote server"
  ```
