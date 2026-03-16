mod common;

use piki_api_client::{ApiClient, ApiRequest, Auth, ClientConfig, HttpClient};
use serde::{Deserialize, Serialize};
use wiremock::matchers::{bearer_token, body_json, header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct TestPayload {
    name: String,
    value: i32,
}

#[tokio::test(flavor = "multi_thread")]
async fn get_basic() {
    let (server, client) = common::setup_mock_client().await;

    Mock::given(method("GET"))
        .and(path("/items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server)
        .await;

    let resp = client.execute(ApiRequest::get("/items")).await.unwrap();
    assert!(resp.is_success());
    assert_eq!(resp.status, 200);
    let body: serde_json::Value = resp.json().unwrap();
    assert_eq!(body["ok"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn post_with_json_body() {
    let (server, client) = common::setup_mock_client().await;
    let payload = TestPayload {
        name: "test".into(),
        value: 42,
    };

    Mock::given(method("POST"))
        .and(path("/items"))
        .and(body_json(&payload))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({"id": 1})))
        .mount(&server)
        .await;

    let req = ApiRequest::post("/items").with_json_body(&payload).unwrap();
    let resp = client.execute(req).await.unwrap();
    assert!(resp.is_success());
    assert_eq!(resp.status, 201);
}

#[tokio::test(flavor = "multi_thread")]
async fn put_request() {
    let (server, client) = common::setup_mock_client().await;
    let payload = TestPayload {
        name: "updated".into(),
        value: 99,
    };

    Mock::given(method("PUT"))
        .and(path("/items/1"))
        .and(body_json(&payload))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let req = ApiRequest::put("/items/1")
        .with_json_body(&payload)
        .unwrap();
    let resp = client.execute(req).await.unwrap();
    assert!(resp.is_success());
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_request() {
    let (server, client) = common::setup_mock_client().await;

    Mock::given(method("DELETE"))
        .and(path("/items/1"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let resp = client
        .execute(ApiRequest::delete("/items/1"))
        .await
        .unwrap();
    assert!(resp.is_success());
    assert_eq!(resp.status, 204);
}

#[tokio::test(flavor = "multi_thread")]
async fn patch_request() {
    let (server, client) = common::setup_mock_client().await;

    Mock::given(method("PATCH"))
        .and(path("/items/1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"patched": true})),
        )
        .mount(&server)
        .await;

    let req = ApiRequest::patch("/items/1")
        .with_json_body(&serde_json::json!({"value": 100}))
        .unwrap();
    let resp = client.execute(req).await.unwrap();
    assert!(resp.is_success());
}

#[tokio::test(flavor = "multi_thread")]
async fn query_params() {
    let (server, client) = common::setup_mock_client().await;

    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("q", "rust"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server)
        .await;

    let req = ApiRequest::get("/search")
        .with_query("q", "rust")
        .with_query("page", "2");
    let resp = client.execute(req).await.unwrap();
    assert!(resp.is_success());
}

#[tokio::test(flavor = "multi_thread")]
async fn custom_headers() {
    let (server, client) = common::setup_mock_client().await;

    Mock::given(method("GET"))
        .and(path("/protected"))
        .and(header("X-Custom", "my-value"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let req = ApiRequest::get("/protected").with_header("X-Custom", "my-value");
    let resp = client.execute(req).await.unwrap();
    assert!(resp.is_success());
}

#[tokio::test(flavor = "multi_thread")]
async fn bearer_auth() {
    let (server, _) = common::setup_mock_client().await;

    let config = ClientConfig::new(server.uri()).with_auth(Auth::Bearer("secret-token".into()));
    let client = HttpClient::new(config).unwrap();

    Mock::given(method("GET"))
        .and(path("/secure"))
        .and(bearer_token("secret-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"user": "admin"})),
        )
        .mount(&server)
        .await;

    let resp = client.execute(ApiRequest::get("/secure")).await.unwrap();
    assert!(resp.is_success());
    let body: serde_json::Value = resp.json().unwrap();
    assert_eq!(body["user"], "admin");
}

#[tokio::test(flavor = "multi_thread")]
async fn response_404() {
    let (server, client) = common::setup_mock_client().await;

    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({"error": "not found"})),
        )
        .mount(&server)
        .await;

    let resp = client.execute(ApiRequest::get("/missing")).await.unwrap();
    assert!(!resp.is_success());
    assert_eq!(resp.status, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn response_as_text() {
    let (server, client) = common::setup_mock_client().await;

    Mock::given(method("GET"))
        .and(path("/text"))
        .respond_with(ResponseTemplate::new(200).set_body_string("hello world"))
        .mount(&server)
        .await;

    let resp = client.execute(ApiRequest::get("/text")).await.unwrap();
    assert!(resp.is_success());
    assert_eq!(resp.text().unwrap(), "hello world");
}
