# piki-api-client

Independent HTTP API client crate. **Must NOT depend on `piki-core` or `crates/tui`.**

## Modules

- `client.rs` — `ApiClient` trait abstracting the transport layer.
- `http.rs` — `HttpClient` implementing `ApiClient` via `reqwest`.
- `parser.rs` — Hurl-like syntax parser: converts `METHOD URL\nHeaders\n\nBody` text into `ParsedRequest`. Supports `parse_hurl()` (single) and `parse_hurl_multi()` (multiple).
- `request.rs` — `ApiRequest`, `Method` enum.
- `response.rs` — `ApiResponse` struct.
- `config.rs` — `ClientConfig`, `Auth` (bearer, basic, custom header).
- `protocol.rs` — `Protocol` enum (HTTP, prepared for future gRPC).
- `ollama.rs` — `OllamaClient` for Ollama HTTP API: `list_models()` (GET /api/tags), `chat_stream()` (POST /api/chat with streaming via `mpsc` channels), `chat()` (non-streaming). Types: `OllamaMessage`, `OllamaModel`, `ChatStreamEvent`.

## Conventions

- All public types re-exported from `lib.rs`.
- Error handling: `anyhow::Result`.
- Tests: use `wiremock` for HTTP mocking in integration tests.
- Keep this crate minimal and transport-focused — no UI or domain logic.
