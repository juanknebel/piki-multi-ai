use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerConfig {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub extensions: Vec<String>,
    #[serde(default)]
    pub init_options: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspRegistry {
    pub servers: Vec<LspServerConfig>,
    #[serde(default = "default_idle_ttl")]
    pub idle_ttl_secs: u64,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
}

fn default_idle_ttl() -> u64 {
    300
}
fn default_max_concurrent() -> usize {
    3
}

impl Default for LspRegistry {
    fn default() -> Self {
        Self {
            servers: vec![
                LspServerConfig {
                    id: "rust-analyzer".into(),
                    command: "rust-analyzer".into(),
                    args: vec![],
                    extensions: vec!["rs".into()],
                    init_options: None,
                },
                LspServerConfig {
                    id: "typescript-language-server".into(),
                    command: "typescript-language-server".into(),
                    args: vec!["--stdio".into()],
                    extensions: vec![
                        "ts".into(),
                        "tsx".into(),
                        "js".into(),
                        "jsx".into(),
                    ],
                    init_options: None,
                },
                LspServerConfig {
                    id: "pyright-langserver".into(),
                    command: "pyright-langserver".into(),
                    args: vec!["--stdio".into()],
                    extensions: vec!["py".into()],
                    init_options: None,
                },
            ],
            idle_ttl_secs: 300,
            max_concurrent: 3,
        }
    }
}

impl LspRegistry {
    pub fn load_or_default(path: &Path) -> Self {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(content) => match toml::from_str::<LspRegistry>(&content) {
                    Ok(registry) => return registry,
                    Err(e) => {
                        tracing::warn!("Failed to parse LSP config {}: {}", path.display(), e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read LSP config {}: {}", path.display(), e);
                }
            }
        }

        let registry = Self::default();
        // Write default config for user reference
        if let Ok(content) = toml::to_string_pretty(&registry) {
            let _ = std::fs::write(path, content);
        }
        registry
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn find_server_for_extension(&self, ext: &str) -> Option<&LspServerConfig> {
        self.servers
            .iter()
            .find(|s| s.extensions.iter().any(|e| e == ext))
    }
}
