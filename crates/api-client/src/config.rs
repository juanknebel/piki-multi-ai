use std::collections::HashMap;
use std::time::Duration;

/// Authentication method for API requests.
#[derive(Debug, Clone)]
pub enum Auth {
    Bearer(String),
    Basic { username: String, password: String },
    Header { name: String, value: String },
}

/// Configuration for an API client instance.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub timeout: Duration,
    pub default_headers: HashMap<String, String>,
    pub auth: Option<Auth>,
}

impl ClientConfig {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            timeout: Duration::from_secs(30),
            default_headers: HashMap::new(),
            auth: None,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.default_headers.insert(name.into(), value.into());
        self
    }
}
