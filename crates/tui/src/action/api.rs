use std::sync::Arc;

use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{self, App, ToastLevel};
use piki_core::workspace::WorkspaceManager;

/// Cap on one jq run, mirroring the desktop's `jq_filter` command.
const JQ_TIMEOUT_SECS: u64 = 10;

pub(super) async fn handle(
    app: &mut App,
    _manager: &WorkspaceManager,
    action: Action,
    _terminal: &mut DefaultTerminal,
) -> anyhow::Result<()> {
    match action {
        Action::SendApiRequest(text) => {
            // Parse the Hurl text (supports multiple requests)
            let parsed_requests = match piki_api_client::parse_hurl_multi(&text) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(error = %e, "API Explorer: failed to parse request");
                    app.set_toast(format!("Parse error: {}", e), ToastLevel::Error);
                    return Ok(());
                }
            };

            // Set loading state
            if let Some(ws) = app.workspaces.get_mut(app.active_workspace)
                && let Some(tab) = ws.current_tab_mut()
                && let Some(ref mut api) = tab.api_state
            {
                api.loading = true;
                api.responses.clear();
                let slot = Arc::clone(&api.pending_responses);
                let storage = Arc::clone(&app.storage);
                let source_repo = ws.source_repo.to_string_lossy().to_string();

                tokio::spawn(async move {
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
                            piki_api_client::Method::Delete => {
                                piki_api_client::ApiRequest::delete("")
                            }
                            piki_api_client::Method::Patch => {
                                piki_api_client::ApiRequest::patch("")
                            }
                        };
                        request.body = parsed.body.clone();
                        for (k, v) in &parsed.headers {
                            request.headers.insert(k.clone(), v.clone());
                        }

                        // Build request text for history
                        let request_text = {
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
                                results.push(app::ApiResponseDisplay {
                                    status: 0,
                                    elapsed_ms: 0,
                                    body: format!("Client error: {}", e),
                                    headers: String::new(),
                                });
                                continue;
                            }
                        };

                        let start = std::time::Instant::now();
                        let result =
                            <piki_api_client::HttpClient as piki_api_client::ApiClient>::execute(
                                &client, request,
                            )
                            .await;
                        let elapsed = start.elapsed().as_millis();

                        let display = match result {
                            Ok(resp) => {
                                let body_text = String::from_utf8_lossy(&resp.body).to_string();
                                let body = if let Ok(json) =
                                    serde_json::from_str::<serde_json::Value>(&body_text)
                                {
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
                                app::ApiResponseDisplay {
                                    status: resp.status,
                                    elapsed_ms: elapsed,
                                    body,
                                    headers,
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, url = %url, "API Explorer: request failed");
                                app::ApiResponseDisplay {
                                    status: 0,
                                    elapsed_ms: elapsed,
                                    body: format!("Error: {}", e),
                                    headers: String::new(),
                                }
                            }
                        };

                        // Persist to API history storage if available
                        if let Some(ref api_storage) = storage.api_history {
                            let entry = piki_core::storage::ApiHistoryEntry {
                                id: None,
                                source_repo: source_repo.clone(),
                                created_at: String::new(),
                                request_text,
                                method: method_str.to_string(),
                                url: url.clone(),
                                status: display.status,
                                elapsed_ms: display.elapsed_ms,
                                response_body: display.body.clone(),
                                response_headers: display.headers.clone(),
                            };
                            if let Err(e) = api_storage.save_api_entry(&entry) {
                                tracing::warn!(error = %e, "Failed to persist API history entry");
                            }
                        }

                        results.push(display);
                    }

                    let mut guard = slot.lock();
                    *guard = Some(results);
                });
            }
        }
        Action::RunJqFilter(filter) => {
            let Some(api) = app
                .workspaces
                .get_mut(app.active_workspace)
                .and_then(|ws| ws.current_tab_mut())
                .and_then(|tab| tab.api_state.as_mut())
            else {
                return Ok(());
            };
            // An empty filter is "show me the responses again".
            if filter.trim().is_empty() {
                api.jq_output = None;
                if let Some(ref mut jq) = api.jq {
                    jq.error = None;
                    jq.running = false;
                }
                return Ok(());
            }
            let bodies: Vec<String> = api.responses.iter().map(|r| r.body.clone()).collect();
            if bodies.is_empty() {
                return Ok(());
            }
            if let Some(ref mut jq) = api.jq {
                jq.running = true;
                jq.error = None;
            }
            let slot = Arc::clone(&api.pending_jq);
            tokio::spawn(async move {
                let mut out = Vec::with_capacity(bodies.len());
                let mut failure = None;
                for body in &bodies {
                    match run_jq(&filter, body).await {
                        Ok(text) => out.push(text),
                        Err(e) => {
                            failure = Some(e);
                            break;
                        }
                    }
                }
                let mut guard = slot.lock();
                *guard = Some(match failure {
                    Some(e) => Err(e),
                    None => Ok(out),
                });
            });
        }
        other => unreachable!("non-api action routed to action::api: {other:?}"),
    }
    Ok(())
}

/// Pipe `body` through `jq <filter>`, returning its stdout or a message fit
/// for the filter bar. Goes through `shell_env::command` so `jq` is found
/// with the user's login PATH, and is capped like the desktop's `jq_filter`
/// so a pathological filter can't hang the tab.
async fn run_jq(filter: &str, body: &str) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;

    let mut child = piki_core::shell_env::command("jq")
        .arg(filter)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "jq not found — install jq to filter responses".to_string()
            } else {
                format!("could not run jq: {e}")
            }
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        // A filter that reads nothing (`jq -n`-style) closes stdin early;
        // that is not an error, the exit status below is what counts.
        let _ = stdin.write_all(body.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(JQ_TIMEOUT_SECS),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| format!("jq timed out after {JQ_TIMEOUT_SECS}s"))?
    .map_err(|e| format!("jq failed: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(stderr
            .lines()
            .next()
            .unwrap_or("jq failed")
            .trim()
            .to_string())
    }
}
