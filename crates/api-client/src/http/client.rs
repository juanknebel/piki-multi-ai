use std::collections::HashMap;

use reqwest::Client;
use tracing::debug;

use crate::client::ApiClient;
use crate::config::{Auth, ClientConfig};
use crate::request::{ApiRequest, Method};
use crate::response::ApiResponse;

/// HTTP transport implementation using reqwest.
pub struct HttpClient {
    client: Client,
    config: ClientConfig,
}

impl HttpClient {
    pub fn new(config: ClientConfig) -> anyhow::Result<Self> {
        let client = Client::builder().timeout(config.timeout).build()?;
        Ok(Self { client, config })
    }
}

#[async_trait::async_trait]
impl ApiClient for HttpClient {
    async fn execute(&self, request: ApiRequest) -> anyhow::Result<ApiResponse> {
        let url = format!("{}{}", self.config.base_url, request.path);
        debug!(method = ?request.method, url = %url, "executing request");

        let method = match request.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Delete => reqwest::Method::DELETE,
            Method::Patch => reqwest::Method::PATCH,
        };

        let mut builder = self.client.request(method, &url);

        // Apply default headers
        for (k, v) in &self.config.default_headers {
            builder = builder.header(k, v);
        }

        // Apply request-specific headers
        for (k, v) in &request.headers {
            builder = builder.header(k, v);
        }

        // Apply auth
        if let Some(auth) = &self.config.auth {
            builder = match auth {
                Auth::Bearer(token) => builder.bearer_auth(token),
                Auth::Basic { username, password } => builder.basic_auth(username, Some(password)),
                Auth::Header { name, value } => builder.header(name, value),
            };
        }

        // Apply query params
        if !request.query_params.is_empty() {
            builder = builder.query(&request.query_params);
        }

        // Apply body
        if let Some(body) = request.body {
            builder = builder.body(body);
        }

        let response = builder.send().await?;

        let status = response.status().as_u16();
        let headers: HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
            .collect();
        let body = response.bytes().await?.to_vec();

        Ok(ApiResponse {
            status,
            headers,
            body,
        })
    }
}
