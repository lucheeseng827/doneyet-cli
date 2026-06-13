use anyhow::Result;
use clap::{Args, Subcommand};

use crate::config::StoredConfig;

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: Option<ConfigAction>,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Print the current config (paths + values, token masked).
    Show,
    /// Set a single config key. `dyt config set api-url http://...`
    ///
    /// For secret keys (`admin-token`, `read-basic`), pass `-` as the value
    /// to be prompted on stdin (hidden) instead of putting the secret on the
    /// argv where it would leak into shell history and process listings.
    Set { key: ConfigKey, value: String },
    /// Clear a key.
    Unset { key: ConfigKey },
    /// Print where the config file lives.
    Path,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ConfigKey {
    ApiUrl,
    AdminToken,
    ReadBasic,
}

pub async fn run(args: ConfigArgs) -> Result<()> {
    let action = args.action.unwrap_or(ConfigAction::Show);
    match action {
        ConfigAction::Path => {
            println!("{}", StoredConfig::path()?.display());
        }
        ConfigAction::Show => {
            let path = StoredConfig::path()?;
            let cfg = StoredConfig::load()?;
            println!("config_path = {}", path.display());
            println!(
                "api_url     = {}",
                cfg.api_url.as_deref().unwrap_or("<unset, default http://localhost:3001>")
            );
            println!(
                "admin_token = {}",
                cfg.admin_token.as_deref().map(mask).unwrap_or_else(|| "<unset>".to_string())
            );
            println!(
                "read_basic  = {}",
                cfg.read_basic.as_deref().map(mask).unwrap_or_else(|| "<unset>".to_string())
            );
        }
        ConfigAction::Set { key, value } => {
            let mut cfg = StoredConfig::load()?;
            let effective = resolve_value(key, value)?;
            match key {
                ConfigKey::ApiUrl => cfg.api_url = Some(effective),
                ConfigKey::AdminToken => cfg.admin_token = Some(effective),
                ConfigKey::ReadBasic => cfg.read_basic = Some(effective),
            }
            let path = cfg.save()?;
            println!("saved -> {}", path.display());
        }
        ConfigAction::Unset { key } => {
            let mut cfg = StoredConfig::load()?;
            match key {
                ConfigKey::ApiUrl => cfg.api_url = None,
                ConfigKey::AdminToken => cfg.admin_token = None,
                ConfigKey::ReadBasic => cfg.read_basic = None,
            }
            let path = cfg.save()?;
            println!("saved -> {}", path.display());
        }
    }
    Ok(())
}

fn resolve_value(key: ConfigKey, raw: String) -> Result<String> {
    use std::io::{BufRead, IsTerminal};
    let is_secret = matches!(key, ConfigKey::AdminToken | ConfigKey::ReadBasic);
    if is_secret && raw == "-" {
        let prompt = match key {
            ConfigKey::AdminToken => "admin token: ",
            ConfigKey::ReadBasic => "read basic (user:pass): ",
            ConfigKey::ApiUrl => unreachable!(),
        };
        let entered = if std::io::stdin().is_terminal() {
            rpassword::prompt_password(prompt)?
        } else {
            // Piped input — read a single line from stdin so automation
            // (`echo $TOKEN | dyt config set admin-token -`) works.
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line)?;
            line
        };
        let entered = entered.trim().to_string();
        if entered.is_empty() {
            anyhow::bail!("empty value");
        }
        Ok(entered)
    } else if is_secret && !raw.is_empty() {
        eprintln!(
            "warning: secret value passed on the command line will appear in shell history. \
             Use `dyt config set <key> -` (or `dyt login`) to read from stdin instead."
        );
        Ok(raw)
    } else {
        Ok(raw)
    }
}

fn mask(s: &str) -> String {
    if s.len() <= 8 {
        return "*".repeat(s.len());
    }
    format!("{}…{}", &s[..4], &s[s.len() - 4..])
}
