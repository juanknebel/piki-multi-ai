use piki_api_client::parser::{parse_hurl, parse_hurl_multi};
use piki_api_client::protocol::Protocol;
use piki_api_client::request::Method;

#[test]
fn test_get_no_headers_no_body() {
    let input = "GET https://api.example.com/users";
    let req = parse_hurl(input).unwrap();
    assert_eq!(req.method, Method::Get);
    assert_eq!(req.url, "https://api.example.com/users");
    assert!(req.headers.is_empty());
    assert!(req.body.is_none());
    assert_eq!(req.protocol, Protocol::Http);
}

#[test]
fn test_post_with_headers_and_json_body() {
    let input = "\
POST https://api.example.com/users
Content-Type: application/json
Authorization: Bearer token123

{\"name\": \"test\", \"value\": 42}";

    let req = parse_hurl(input).unwrap();
    assert_eq!(req.method, Method::Post);
    assert_eq!(req.url, "https://api.example.com/users");
    assert_eq!(req.headers.len(), 2);
    assert_eq!(
        req.headers[0],
        ("Content-Type".to_string(), "application/json".to_string())
    );
    assert_eq!(
        req.headers[1],
        ("Authorization".to_string(), "Bearer token123".to_string())
    );
    assert_eq!(
        String::from_utf8(req.body.unwrap()).unwrap(),
        "{\"name\": \"test\", \"value\": 42}"
    );
}

#[test]
fn test_comments_are_ignored() {
    let input = "\
# This is a comment
GET https://api.example.com/users
# Another comment
Authorization: Bearer token
";
    let req = parse_hurl(input).unwrap();
    assert_eq!(req.method, Method::Get);
    assert_eq!(req.headers.len(), 1);
    assert_eq!(req.headers[0].0, "Authorization");
}

#[test]
fn test_invalid_method() {
    let input = "INVALID https://api.example.com/users";
    let err = parse_hurl(input).unwrap_err();
    assert!(err.to_string().contains("Unknown HTTP method"));
}

#[test]
fn test_missing_url() {
    let input = "GET";
    let err = parse_hurl(input).unwrap_err();
    assert!(err.to_string().contains("expected METHOD URL"));
}

#[test]
fn test_grpc_not_supported() {
    let input = "GRPC grpc://api.example.com/Service";
    let err = parse_hurl(input).unwrap_err();
    assert!(err.to_string().contains("gRPC is not yet supported"));
}

#[test]
fn test_multiline_body() {
    let input = "\
POST https://api.example.com/data
Content-Type: text/plain

line 1
line 2
line 3";

    let req = parse_hurl(input).unwrap();
    let body = String::from_utf8(req.body.unwrap()).unwrap();
    assert_eq!(body, "line 1\nline 2\nline 3");
}

#[test]
fn test_empty_input() {
    let input = "";
    let err = parse_hurl(input).unwrap_err();
    assert!(err.to_string().contains("Empty request"));
}

#[test]
fn test_all_methods() {
    for (method_str, expected) in [
        ("GET", Method::Get),
        ("POST", Method::Post),
        ("PUT", Method::Put),
        ("DELETE", Method::Delete),
        ("PATCH", Method::Patch),
    ] {
        let input = format!("{} https://example.com", method_str);
        let req = parse_hurl(&input).unwrap();
        assert_eq!(req.method, expected);
    }
}

#[test]
fn test_case_insensitive_method() {
    let input = "get https://example.com";
    let req = parse_hurl(input).unwrap();
    assert_eq!(req.method, Method::Get);
}

#[test]
fn test_leading_comments_and_blank_lines() {
    let input = "\
# Setup
# More comments

GET https://example.com/health
";
    let req = parse_hurl(input).unwrap();
    assert_eq!(req.method, Method::Get);
    assert_eq!(req.url, "https://example.com/health");
}

// ── Multi-request tests ──

#[test]
fn test_multi_two_gets() {
    let input = "\
GET https://example.com/users

GET https://example.com/posts";

    let reqs = parse_hurl_multi(input).unwrap();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].method, Method::Get);
    assert_eq!(reqs[0].url, "https://example.com/users");
    assert_eq!(reqs[1].method, Method::Get);
    assert_eq!(reqs[1].url, "https://example.com/posts");
}

#[test]
fn test_multi_get_then_post_with_body() {
    let input = "\
GET https://example.com/users/1

POST https://example.com/users
Content-Type: application/json

{\"name\": \"new\"}";

    let reqs = parse_hurl_multi(input).unwrap();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].method, Method::Get);
    assert!(reqs[0].body.is_none());
    assert_eq!(reqs[1].method, Method::Post);
    assert_eq!(reqs[1].headers.len(), 1);
    assert!(reqs[1].body.is_some());
}

#[test]
fn test_multi_three_requests_with_comments() {
    let input = "\
# Get all users
GET https://example.com/users

# Create a user
POST https://example.com/users
Content-Type: application/json

{\"name\": \"test\"}

# Delete user 1
DELETE https://example.com/users/1";

    let reqs = parse_hurl_multi(input).unwrap();
    assert_eq!(reqs.len(), 3);
    assert_eq!(reqs[0].method, Method::Get);
    assert_eq!(reqs[1].method, Method::Post);
    assert_eq!(reqs[2].method, Method::Delete);
}

#[test]
fn test_multi_single_request_still_works() {
    let input = "GET https://example.com/health";
    let reqs = parse_hurl_multi(input).unwrap();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].method, Method::Get);
}

#[test]
fn test_multi_error_in_second_request() {
    // GRPC is a recognized method keyword but not yet supported
    let input = "\
GET https://example.com/ok

GRPC grpc://example.com/Service";

    let err = parse_hurl_multi(input).unwrap_err();
    assert!(err.to_string().contains("gRPC is not yet supported"));
}

#[test]
fn test_multi_post_body_not_confused_with_next_request() {
    let input = "\
POST https://example.com/data
Content-Type: text/plain

This is body text
that spans multiple lines

GET https://example.com/next";

    let reqs = parse_hurl_multi(input).unwrap();
    assert_eq!(reqs.len(), 2);
    let body = String::from_utf8(reqs[0].body.clone().unwrap()).unwrap();
    assert_eq!(body, "This is body text\nthat spans multiple lines");
    assert_eq!(reqs[1].method, Method::Get);
    assert_eq!(reqs[1].url, "https://example.com/next");
}
