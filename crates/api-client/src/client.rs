use crate::request::ApiRequest;
use crate::response::ApiResponse;

/// Trait for executing API requests. Implement this for different transports (HTTP, gRPC, etc.).
#[async_trait::async_trait]
pub trait ApiClient: Send + Sync {
    async fn execute(&self, request: ApiRequest) -> anyhow::Result<ApiResponse>;
}
