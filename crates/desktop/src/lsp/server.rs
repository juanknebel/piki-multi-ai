use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, mpsc};

use super::registry::{LspRegistry, LspServerConfig};

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct LspServerKey {
    pub server_id: String,
    pub root_path: PathBuf,
}

#[derive(Debug, PartialEq)]
pub enum LspServerStatus {
    Active,
    Idle(Instant),
    ShuttingDown,
}

pub struct LspServerInstance {
    pub key: LspServerKey,
    pub status: LspServerStatus,
    pub child: Child,
    pub stdin_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub stdout_broadcast_tx: broadcast::Sender<Vec<u8>>,
    pub ws_path: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct LspServerStatusInfo {
    pub server_id: String,
    pub root_path: String,
    pub status: String,
    pub ws_path: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct LspConnectionInfo {
    pub ws_port: u16,
    pub ws_path: String,
    pub server_id: String,
}

pub struct LspManager {
    pub servers: HashMap<LspServerKey, LspServerInstance>,
    pub registry: LspRegistry,
    pub ws_port: u16,
}

impl LspManager {
    pub fn new(registry: LspRegistry) -> Self {
        Self {
            servers: HashMap::new(),
            registry,
            ws_port: 0,
        }
    }

    /// Start or reactivate an LSP server for a file.
    /// Returns connection info for the frontend.
    pub async fn ensure_server(
        &mut self,
        file_path: &str,
        root_path: &PathBuf,
    ) -> anyhow::Result<Option<LspConnectionInfo>> {
        // Find the appropriate server config based on file extension
        let ext = file_path
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();

        let config = match self.registry.find_server_for_extension(&ext) {
            Some(c) => c.clone(),
            None => return Ok(None),
        };

        let key = LspServerKey {
            server_id: config.id.clone(),
            root_path: root_path.clone(),
        };

        // If already running, reactivate
        if let Some(instance) = self.servers.get_mut(&key)
            && instance.status != LspServerStatus::ShuttingDown
        {
            instance.status = LspServerStatus::Active;
            return Ok(Some(LspConnectionInfo {
                ws_port: self.ws_port,
                ws_path: instance.ws_path.clone(),
                server_id: config.id,
            }));
        }

        // Evict if at capacity
        self.evict_if_needed().await;

        // Spawn new server
        let ws_path = format!(
            "/lsp/{}/{}",
            config.id,
            hash_path(root_path)
        );

        let instance = spawn_server(&config, root_path, &ws_path).await?;
        let info = LspConnectionInfo {
            ws_port: self.ws_port,
            ws_path: instance.ws_path.clone(),
            server_id: config.id.clone(),
        };

        self.servers.insert(key, instance);
        tracing::info!(
            server = %config.id,
            root = %root_path.display(),
            "LSP server started"
        );

        Ok(Some(info))
    }

    /// Mark all servers for a given root_path as idle.
    pub fn mark_idle_by_root(&mut self, root_path: &PathBuf) {
        let now = Instant::now();
        for instance in self.servers.values_mut() {
            if &instance.key.root_path == root_path
                && instance.status == LspServerStatus::Active
            {
                instance.status = LspServerStatus::Idle(now);
                tracing::debug!(
                    server = %instance.key.server_id,
                    "LSP server marked idle"
                );
            }
        }
    }

    /// Mark all servers for a given root_path as active.
    pub fn mark_active_by_root(&mut self, root_path: &PathBuf) {
        for instance in self.servers.values_mut() {
            if &instance.key.root_path == root_path
                && matches!(instance.status, LspServerStatus::Idle(_))
            {
                instance.status = LspServerStatus::Active;
                tracing::debug!(
                    server = %instance.key.server_id,
                    "LSP server reactivated"
                );
            }
        }
    }

    /// Shut down a specific server.
    pub async fn shutdown_server(&mut self, key: &LspServerKey) {
        if let Some(mut instance) = self.servers.remove(key) {
            instance.status = LspServerStatus::ShuttingDown;

            // Send LSP shutdown request
            let shutdown_req =
                r#"{"jsonrpc":"2.0","id":999999,"method":"shutdown","params":null}"#;
            let msg = format_lsp_message(shutdown_req);
            let _ = instance.stdin_tx.send(msg);

            // Brief delay for graceful shutdown
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Send exit notification
            let exit_notif = r#"{"jsonrpc":"2.0","method":"exit","params":null}"#;
            let msg = format_lsp_message(exit_notif);
            let _ = instance.stdin_tx.send(msg);

            // Give it a moment, then kill
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = instance.child.kill().await;

            tracing::info!(
                server = %key.server_id,
                root = %key.root_path.display(),
                "LSP server shut down"
            );
        }
    }

    /// Shut down servers whose idle time exceeds TTL.
    pub async fn reap_idle_servers(&mut self) {
        let ttl = Duration::from_secs(self.registry.idle_ttl_secs);
        let keys_to_reap: Vec<LspServerKey> = self
            .servers
            .iter()
            .filter_map(|(key, inst)| {
                if let LspServerStatus::Idle(since) = inst.status
                    && since.elapsed() > ttl
                {
                    return Some(key.clone());
                }
                None
            })
            .collect();

        for key in keys_to_reap {
            self.shutdown_server(&key).await;
        }
    }

    /// Evict the oldest idle server if at max_concurrent.
    async fn evict_if_needed(&mut self) {
        if self.servers.len() < self.registry.max_concurrent {
            return;
        }

        // Find oldest idle server
        let oldest_idle = self
            .servers
            .iter()
            .filter_map(|(key, inst)| {
                if let LspServerStatus::Idle(since) = inst.status {
                    Some((key.clone(), since))
                } else {
                    None
                }
            })
            .min_by_key(|(_, since)| *since)
            .map(|(key, _)| key);

        if let Some(key) = oldest_idle {
            self.shutdown_server(&key).await;
        }
    }

    /// Get status of all running servers.
    pub fn status_all(&self) -> Vec<LspServerStatusInfo> {
        self.servers
            .values()
            .map(|inst| LspServerStatusInfo {
                server_id: inst.key.server_id.clone(),
                root_path: inst.key.root_path.display().to_string(),
                status: match &inst.status {
                    LspServerStatus::Active => "active".into(),
                    LspServerStatus::Idle(_) => "idle".into(),
                    LspServerStatus::ShuttingDown => "shutting_down".into(),
                },
                ws_path: inst.ws_path.clone(),
            })
            .collect()
    }
}

/// Spawn a language server as a child process and set up I/O bridging.
async fn spawn_server(
    config: &LspServerConfig,
    root_path: &PathBuf,
    ws_path: &str,
) -> anyhow::Result<LspServerInstance> {
    let resolved = piki_core::shell_env::resolve_command(&config.command);

    let mut cmd = Command::new(&resolved);
    cmd.args(&config.args)
        .current_dir(root_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // Apply user's login shell environment
    let env = piki_core::shell_env::user_login_env();
    if !env.is_empty() {
        cmd.envs(env);
    }

    let mut child = cmd.spawn().map_err(|e| {
        anyhow::anyhow!(
            "Failed to spawn LSP server '{}' (resolved: '{}'): {}",
            config.command,
            resolved,
            e
        )
    })?;

    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    // Channel: frontend (via WS) → server stdin
    let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    // Broadcast: server stdout → frontend (via WS)
    let (stdout_broadcast_tx, _) = broadcast::channel::<Vec<u8>>(64);

    // Stdin writer task
    let mut stdin_writer = stdin;
    tokio::spawn(async move {
        while let Some(data) = stdin_rx.recv().await {
            if stdin_writer.write_all(&data).await.is_err() {
                break;
            }
            if stdin_writer.flush().await.is_err() {
                break;
            }
        }
    });

    // Stdout reader task — parses Content-Length framing
    let broadcast_tx = stdout_broadcast_tx.clone();
    let server_id_log = config.id.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_lsp_message(&mut reader).await {
                Ok(Some(msg)) => {
                    let _ = broadcast_tx.send(msg);
                }
                Ok(None) => {
                    tracing::debug!(server = %server_id_log, "LSP stdout EOF");
                    break;
                }
                Err(e) => {
                    tracing::warn!(server = %server_id_log, "LSP stdout error: {e}");
                    break;
                }
            }
        }
    });

    // Stderr reader task — log warnings
    let server_id_log2 = config.id.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
            tracing::debug!(server = %server_id_log2, stderr = %line.trim());
            line.clear();
        }
    });

    let key = LspServerKey {
        server_id: config.id.clone(),
        root_path: root_path.clone(),
    };

    Ok(LspServerInstance {
        key,
        status: LspServerStatus::Active,
        child,
        stdin_tx,
        stdout_broadcast_tx,
        ws_path: ws_path.to_string(),
    })
}

/// Read a single LSP message from a stream using Content-Length framing.
async fn read_lsp_message<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> anyhow::Result<Option<Vec<u8>>> {
    let mut content_length: Option<usize> = None;

    // Read headers
    loop {
        let mut header_line = String::new();
        let bytes_read = reader.read_line(&mut header_line).await?;
        if bytes_read == 0 {
            return Ok(None); // EOF
        }

        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break; // End of headers
        }

        if let Some(len_str) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(len_str.trim().parse()?);
        }
    }

    let length = content_length.ok_or_else(|| anyhow::anyhow!("Missing Content-Length header"))?;

    // Read body
    let mut body = vec![0u8; length];
    tokio::io::AsyncReadExt::read_exact(reader, &mut body).await?;

    Ok(Some(body))
}

/// Format a JSON-RPC message with Content-Length header for LSP.
fn format_lsp_message(json: &str) -> Vec<u8> {
    format!("Content-Length: {}\r\n\r\n{}", json.len(), json).into_bytes()
}

fn hash_path(path: &PathBuf) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}
