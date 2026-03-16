use std::collections::HashMap;

use serde::Serialize;

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

/// A request to be executed by an `ApiClient`.
#[derive(Debug, Clone)]
pub struct ApiRequest {
    pub method: Method,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub query_params: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

impl ApiRequest {
    fn new(method: Method, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            headers: HashMap::new(),
            query_params: Vec::new(),
            body: None,
        }
    }

    pub fn get(path: impl Into<String>) -> Self {
        Self::new(Method::Get, path)
    }

    pub fn post(path: impl Into<String>) -> Self {
        Self::new(Method::Post, path)
    }

    pub fn put(path: impl Into<String>) -> Self {
        Self::new(Method::Put, path)
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self::new(Method::Delete, path)
    }

    pub fn patch(path: impl Into<String>) -> Self {
        Self::new(Method::Patch, path)
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    pub fn with_query(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query_params.push((key.into(), value.into()));
        self
    }

    pub fn with_json_body<T: Serialize>(mut self, body: &T) -> anyhow::Result<Self> {
        let json = serde_json::to_vec(body)?;
        self.headers
            .insert("Content-Type".to_string(), "application/json".to_string());
        self.body = Some(json);
        Ok(self)
    }

    pub fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }
}
