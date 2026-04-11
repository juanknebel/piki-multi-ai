pub mod client;
pub mod config;
pub mod http;
pub mod ollama;
pub mod parser;
pub mod protocol;
pub mod request;
pub mod response;

pub use client::ApiClient;
pub use config::{Auth, ClientConfig};
pub use http::HttpClient;
pub use ollama::{ChatStreamEvent, OllamaClient, OllamaMessage, OllamaModel};
pub use parser::{ParsedRequest, parse_hurl, parse_hurl_multi};
pub use protocol::Protocol;
pub use request::{ApiRequest, Method};
pub use response::ApiResponse;
