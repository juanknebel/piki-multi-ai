use std::collections::HashMap;

use serde::de::DeserializeOwned;

/// Response returned by an `ApiClient`.
#[derive(Debug, Clone)]
pub struct ApiResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl ApiResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn json<T: DeserializeOwned>(&self) -> anyhow::Result<T> {
        Ok(serde_json::from_slice(&self.body)?)
    }

    pub fn text(&self) -> anyhow::Result<String> {
        Ok(String::from_utf8(self.body.clone())?)
    }
}
