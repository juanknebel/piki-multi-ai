use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;

use piki_core::storage::{ApiHistoryEntry, AppStorage};

use crate::state::DesktopApp;

#[derive(Serialize, Clone)]
pub struct ApiResponseResult {
    pub status: u16,
    pub elapsed_ms: u64,
    pub body: String,
    pub headers: String,
    pub method: String,
    pub url: String,
}

#[derive(Serialize, Clone)]
pub struct ApiHistoryEntryDto {
    pub id: Option<i64>,
    pub created_at: String,
    pub request_text: String,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub elapsed_ms: u64,
    pub response_body: String,
    pub response_headers: String,
}

fn entry_to_dto(e: &ApiHistoryEntry) -> ApiHistoryEntryDto {
    ApiHistoryEntryDto {
        id: e.id,
        created_at: e.created_at.clone(),
        request_text: e.request_text.clone(),
        method: e.method.clone(),
        url: e.url.clone(),
        status: e.status,
        elapsed_ms: e.elapsed_ms as u64,
        response_body: e.response_body.clone(),
        response_headers: e.response_headers.clone(),
    }
}

#[tauri::command]
pub async fn send_api_request(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    request_text: String,
) -> Result<Vec<ApiResponseResult>, String> {
    let (source_repo, storage) = extract_repo_and_storage(&state, workspace_idx)?;

    let parsed_requests = piki_api_client::parse_hurl_multi(&request_text)
        .map_err(|e| format!("Parse error: {e}"))?;

    let mut results = Vec::with_capacity(parsed_requests.len());

    for parsed in parsed_requests {
        let url = if !parsed.url.contains("://") {
            format!("https://{}", parsed.url)
        } else {
            parsed.url.clone()
        };

        let method_str = match parsed.method {
            piki_api_client::Method::Get => "GET",
            piki_api_client::Method::Post => "POST",
            piki_api_client::Method::Put => "PUT",
            piki_api_client::Method::Delete => "DELETE",
            piki_api_client::Method::Patch => "PATCH",
        };

        let mut request = match parsed.method {
            piki_api_client::Method::Get => piki_api_client::ApiRequest::get(""),
            piki_api_client::Method::Post => piki_api_client::ApiRequest::post(""),
            piki_api_client::Method::Put => piki_api_client::ApiRequest::put(""),
            piki_api_client::Method::Delete => piki_api_client::ApiRequest::delete(""),
            piki_api_client::Method::Patch => piki_api_client::ApiRequest::patch(""),
        };
        request.body = parsed.body.clone();
        for (k, v) in &parsed.headers {
            request.headers.insert(k.clone(), v.clone());
        }

        // Build request text for history
        let hist_request_text = {
            let mut text = format!("{} {}", method_str, url);
            for (k, v) in &parsed.headers {
                text.push_str(&format!("\n{}: {}", k, v));
            }
            if let Some(ref body) = parsed.body {
                let body_str = String::from_utf8_lossy(body);
                text.push_str(&format!("\n\n{}", body_str));
            }
            text
        };

        let config = piki_api_client::ClientConfig::new(&url);
        let client = match piki_api_client::HttpClient::new(config) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "API Explorer: failed to create HTTP client");
                results.push(ApiResponseResult {
                    status: 0,
                    elapsed_ms: 0,
                    body: format!("Client error: {}", e),
                    headers: String::new(),
                    method: method_str.to_string(),
                    url: url.clone(),
                });
                continue;
            }
        };

        let start = std::time::Instant::now();
        let result =
            <piki_api_client::HttpClient as piki_api_client::ApiClient>::execute(&client, request)
                .await;
        let elapsed = start.elapsed().as_millis() as u64;

        let display = match result {
            Ok(resp) => {
                let body_text = String::from_utf8_lossy(&resp.body).to_string();
                let body =
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body_text) {
                        serde_json::to_string_pretty(&json).unwrap_or(body_text)
                    } else {
                        body_text
                    };
                let headers = resp
                    .headers
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect::<Vec<_>>()
                    .join("\n");
                tracing::info!(status = resp.status, elapsed_ms = elapsed, url = %url, "API Explorer: request completed");
                ApiResponseResult {
                    status: resp.status,
                    elapsed_ms: elapsed,
                    body,
                    headers,
                    method: method_str.to_string(),
                    url: url.clone(),
                }
            }
            Err(e) => {
                tracing::error!(error = %e, url = %url, "API Explorer: request failed");
                ApiResponseResult {
                    status: 0,
                    elapsed_ms: elapsed,
                    body: format!("Error: {}", e),
                    headers: String::new(),
                    method: method_str.to_string(),
                    url: url.clone(),
                }
            }
        };

        // Persist to API history
        if let Some(ref api_storage) = storage.api_history {
            let entry = ApiHistoryEntry {
                id: None,
                source_repo: source_repo.to_string_lossy().to_string(),
                created_at: String::new(),
                request_text: hist_request_text,
                method: method_str.to_string(),
                url: url.clone(),
                status: display.status,
                elapsed_ms: display.elapsed_ms as u128,
                response_body: display.body.clone(),
                response_headers: display.headers.clone(),
            };
            if let Err(e) = api_storage.save_api_entry(&entry) {
                tracing::warn!(error = %e, "Failed to persist API history entry");
            }
        }

        results.push(display);
    }

    Ok(results)
}

#[tauri::command]
pub async fn load_api_history(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    limit: usize,
) -> Result<Vec<ApiHistoryEntryDto>, String> {
    let (source_repo, storage) = extract_repo_and_storage(&state, workspace_idx)?;

    let entries = storage
        .api_history
        .as_ref()
        .map(|h| h.load_recent_api_history(&source_repo, limit))
        .transpose()
        .map_err(|e| format!("Failed to load history: {e}"))?
        .unwrap_or_default();

    Ok(entries.iter().map(entry_to_dto).collect())
}

#[tauri::command]
pub async fn search_api_history(
    state: State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
    query: String,
    limit: usize,
) -> Result<Vec<ApiHistoryEntryDto>, String> {
    let (source_repo, storage) = extract_repo_and_storage(&state, workspace_idx)?;

    let entries = storage
        .api_history
        .as_ref()
        .map(|h| h.search_api_history(&source_repo, &query, limit))
        .transpose()
        .map_err(|e| format!("Failed to search history: {e}"))?
        .unwrap_or_default();

    Ok(entries.iter().map(entry_to_dto).collect())
}

#[tauri::command]
pub async fn delete_api_history_entry(
    state: State<'_, Mutex<DesktopApp>>,
    entry_id: i64,
) -> Result<(), String> {
    let storage = {
        let app = state.lock();
        Arc::clone(&app.storage)
    };

    storage
        .api_history
        .as_ref()
        .ok_or_else(|| "API history storage not available".to_string())?
        .delete_api_entry(entry_id)
        .map_err(|e| format!("Failed to delete entry: {e}"))
}

#[tauri::command]
pub async fn jq_filter(input: String, filter: String) -> Result<String, String> {
    let mut child = tokio::process::Command::new("jq")
        .arg(&filter)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run jq: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to jq: {e}"))?;
    }

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| "jq timed out after 10s".to_string())?
    .map_err(|e| format!("jq failed: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(stderr.trim().to_string())
    }
}

fn extract_repo_and_storage(
    state: &State<'_, Mutex<DesktopApp>>,
    workspace_idx: usize,
) -> Result<(PathBuf, Arc<AppStorage>), String> {
    let app = state.lock();
    if workspace_idx >= app.workspaces.len() {
        return Err("Workspace index out of range".to_string());
    }
    let repo = app.workspaces[workspace_idx].info.source_repo.clone();
    let storage = Arc::clone(&app.storage);
    Ok((repo, storage))
}
