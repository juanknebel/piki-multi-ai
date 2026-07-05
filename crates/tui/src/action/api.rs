use std::sync::Arc;

use ratatui::DefaultTerminal;

use super::Action;
use crate::app::{self, App, ToastLevel};
use piki_core::workspace::WorkspaceManager;

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
        other => unreachable!("non-api action routed to action::api: {other:?}"),
    }
    Ok(())
}
