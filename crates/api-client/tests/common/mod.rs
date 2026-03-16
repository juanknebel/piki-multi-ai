use piki_api_client::{ClientConfig, HttpClient};
use wiremock::MockServer;

pub async fn setup_mock_client() -> (MockServer, HttpClient) {
    let server = MockServer::start().await;
    let config = ClientConfig::new(server.uri());
    let client = HttpClient::new(config).expect("failed to create HttpClient");
    (server, client)
}
