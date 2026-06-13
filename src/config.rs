use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::cli::GlobalArgs;

const DEFAULT_API_URL: &str = "http://localhost:3001";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct StoredConfig {
    pub api_url: Option<String>,
    pub admin_token: Option<String>,
    pub read_basic: Option<String>,
}

/// Effective config after merging stored values with CLI flags / env vars.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub api_url: String,
    pub admin_token: Option<String>,
    pub run_token: Option<String>,
    pub read_basic: Option<String>,
    pub insecure: bool,
}

impl ResolvedConfig {
    pub fn from_global(global: &GlobalArgs) -> Result<Self> {
        let stored = StoredConfig::load().context("loading stored dyt config")?;

        let api_url = global
            .api_url
            .clone()
            .or(stored.api_url)
            .unwrap_or_else(|| DEFAULT_API_URL.to_string());

        Ok(Self {
            api_url: api_url.trim_end_matches('/').to_string(),
            admin_token: global.admin_token.clone().or(stored.admin_token),
            run_token: global.run_token.clone(),
            read_basic: global.read_basic.clone().or(stored.read_basic),
            insecure: global.insecure,
        })
    }

    pub fn api_base(&self) -> String {
        format!("{}/api/v1", self.api_url)
    }
}

impl StoredConfig {
    pub fn path() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("", "", "doneyet")
            .context("could not resolve the user's config directory")?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&bytes).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self) -> Result<PathBuf> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("creating config dir {}", parent.display())
            })?;
        }
        let body = toml::to_string_pretty(self).context("serializing config")?;
        write_atomic_0600(&path, body.as_bytes())?;
        Ok(path)
    }
}

/// Create/truncate `path` with mode 0600 on Unix (where applicable) and
/// write `bytes`. On non-Unix the file is written with default perms.
pub fn write_atomic_0600(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("writing {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}
