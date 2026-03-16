use crate::protocol::Protocol;
use crate::request::Method;

const METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "GRPC"];

/// A parsed request from Hurl-like syntax.
#[derive(Debug, Clone)]
pub struct ParsedRequest {
    pub protocol: Protocol,
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// Check if a line looks like a METHOD URL request line.
fn is_method_line(line: &str) -> bool {
    let trimmed = line.trim();
    if let Some((word, rest)) = trimmed.split_once(char::is_whitespace) {
        METHODS.contains(&word.to_uppercase().as_str()) && !rest.trim().is_empty()
    } else {
        false
    }
}

/// Parse Hurl-like syntax that may contain multiple requests.
///
/// Requests are separated naturally: each new `METHOD URL` line after the
/// first one starts a new request entry (same as real Hurl files).
///
/// Returns one or more parsed requests.
pub fn parse_hurl_multi(input: &str) -> anyhow::Result<Vec<ParsedRequest>> {
    // Filter comments, keep line content
    let lines: Vec<&str> = input
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect();

    // Split into request blocks: each block starts with a METHOD line
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut seen_method = false;

    for line in &lines {
        if is_method_line(line) && seen_method {
            // Trim trailing empty lines from previous block
            while current.last().is_some_and(|l| l.trim().is_empty()) {
                current.pop();
            }
            if !current.is_empty() {
                blocks.push(current);
                current = Vec::new();
            }
        }

        if is_method_line(line) {
            seen_method = true;
        }

        current.push(line);
    }

    // Push last block
    while current.last().is_some_and(|l| l.trim().is_empty()) {
        current.pop();
    }
    if !current.is_empty() {
        blocks.push(current);
    }

    if blocks.is_empty() {
        anyhow::bail!("Empty request: expected METHOD URL");
    }

    let mut requests = Vec::with_capacity(blocks.len());
    for (i, block) in blocks.iter().enumerate() {
        let text = block.join("\n");
        match parse_single_block(&text) {
            Ok(req) => requests.push(req),
            Err(e) => {
                if blocks.len() == 1 {
                    return Err(e);
                }
                anyhow::bail!("Request #{}: {}", i + 1, e);
            }
        }
    }

    Ok(requests)
}

/// Parse a single request (convenience wrapper for single-request input).
pub fn parse_hurl(input: &str) -> anyhow::Result<ParsedRequest> {
    let mut requests = parse_hurl_multi(input)?;
    if requests.is_empty() {
        anyhow::bail!("Empty request: expected METHOD URL");
    }
    Ok(requests.remove(0))
}

/// Parse a single request block (no multi-request splitting).
fn parse_single_block(input: &str) -> anyhow::Result<ParsedRequest> {
    let lines: Vec<&str> = input
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect();

    let first_line = lines
        .iter()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Empty request: expected METHOD URL"))?
        .trim();

    let (method_str, url) = first_line.split_once(char::is_whitespace).ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid request line: expected METHOD URL, got: {}",
            first_line
        )
    })?;

    let method = match method_str.to_uppercase().as_str() {
        "GET" => Method::Get,
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "DELETE" => Method::Delete,
        "PATCH" => Method::Patch,
        "GRPC" => anyhow::bail!("gRPC is not yet supported"),
        other => anyhow::bail!("Unknown HTTP method: {}", other),
    };

    let url = url.trim().to_string();
    if url.is_empty() {
        anyhow::bail!("Missing URL after method");
    }

    let mut after_method = false;
    let mut in_headers = true;
    let mut headers = Vec::new();
    let mut body_lines: Vec<&str> = Vec::new();

    for line in &lines {
        let trimmed = line.trim();
        if !after_method {
            if !trimmed.is_empty() {
                after_method = true;
            }
            continue;
        }

        if in_headers {
            if trimmed.is_empty() {
                in_headers = false;
                continue;
            }
            if let Some((key, value)) = trimmed.split_once(':') {
                headers.push((key.trim().to_string(), value.trim().to_string()));
            }
        } else {
            body_lines.push(line);
        }
    }

    while body_lines.last().is_some_and(|l| l.trim().is_empty()) {
        body_lines.pop();
    }

    let body = if body_lines.is_empty() {
        None
    } else {
        Some(body_lines.join("\n").into_bytes())
    };

    Ok(ParsedRequest {
        protocol: Protocol::Http,
        method,
        url,
        headers,
        body,
    })
}
