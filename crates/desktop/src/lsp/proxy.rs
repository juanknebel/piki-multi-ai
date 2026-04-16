use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

use super::server::LspManager;

/// Start the WebSocket proxy server on a random local port.
/// Returns the bound port number.
pub async fn start_ws_server(
    lsp_manager: Arc<Mutex<LspManager>>,
) -> anyhow::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    tracing::info!(port, "LSP WebSocket server listening");

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    tracing::debug!(%addr, "LSP WebSocket connection");
                    let manager = Arc::clone(&lsp_manager);
                    tokio::spawn(handle_connection(stream, manager));
                }
                Err(e) => {
                    tracing::error!("LSP WebSocket accept error: {e}");
                }
            }
        }
    });

    Ok(port)
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    lsp_manager: Arc<Mutex<LspManager>>,
) {
    // Perform the WebSocket handshake and extract the request path
    let mut ws_path = String::new();

    let ws_stream = match tokio_tungstenite::accept_hdr_async(
        stream,
        |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
         response: tokio_tungstenite::tungstenite::handshake::server::Response| {
            ws_path = req.uri().path().to_string();
            Ok(response)
        },
    )
    .await
    {
        Ok(ws) => ws,
        Err(e) => {
            tracing::warn!("WebSocket handshake failed: {e}");
            return;
        }
    };

    tracing::debug!(path = %ws_path, "WebSocket connected");

    // Find the LSP server instance for this path
    let (stdin_tx, mut stdout_rx) = {
        let manager = lsp_manager.lock().await;
        let instance = match manager
            .servers
            .values()
            .find(|inst| inst.ws_path == ws_path)
        {
            Some(inst) => inst,
            None => {
                tracing::warn!(path = %ws_path, "No LSP server found for WebSocket path");
                return;
            }
        };
        (
            instance.stdin_tx.clone(),
            instance.stdout_broadcast_tx.subscribe(),
        )
    };

    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // Task: server stdout → WebSocket
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = stdout_rx.recv().await {
            // Send raw JSON body (no Content-Length) over WebSocket
            if ws_sink
                .send(Message::Text(String::from_utf8_lossy(&msg).into_owned().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // Task: WebSocket → server stdin
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_stream.next().await {
            match msg {
                Message::Text(text) => {
                    // Wrap in Content-Length framing for the LSP server
                    let framed = format!(
                        "Content-Length: {}\r\n\r\n{}",
                        text.len(),
                        text
                    );
                    if stdin_tx.send(framed.into_bytes()).is_err() {
                        break;
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }

    tracing::debug!(path = %ws_path, "WebSocket disconnected");
}
