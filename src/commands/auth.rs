use anyhow::{Context, Result};
use clap::Args;
use reqwest::Method;
use serde_json::Value;

use crate::cli::GlobalArgs;
use crate::client::{Auth, Client};
use crate::config::{ResolvedConfig, StoredConfig};

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// Admin token. If omitted, dyt prompts on stdin (hidden input).
    #[arg(long, env = "DONEYET_ADMIN_TOKEN", hide_env_values = true)]
    pub admin_token: Option<String>,

    /// API base URL to save with the token.
    #[arg(long)]
    pub api_url: Option<String>,
}

#[derive(Debug, Args)]
pub struct WhoamiArgs {}

pub async fn login(args: LoginArgs) -> Result<()> {
    let mut cfg = StoredConfig::load()
        .context("loading existing config (refusing to overwrite a broken file)")?;
    if let Some(u) = args.api_url {
        cfg.api_url = Some(u);
    }
    let token = match args.admin_token {
        Some(t) => t,
        None => rpassword::prompt_password("doneyet admin token: ")?,
    };
    if token.trim().is_empty() {
        anyhow::bail!("empty token");
    }
    cfg.admin_token = Some(token.trim().to_string());
    let path = cfg.save()?;
    println!("token saved -> {}", path.display());
    Ok(())
}

pub async fn logout() -> Result<()> {
    let mut cfg = StoredConfig::load()
        .context("loading existing config (refusing to overwrite a broken file)")?;
    cfg.admin_token = None;
    let path = cfg.save()?;
    println!("token cleared from {}", path.display());
    Ok(())
}

pub async fn whoami(global: &GlobalArgs, _args: WhoamiArgs) -> Result<()> {
    let cfg = ResolvedConfig::from_global(global)?;
    println!("api_url     = {}", cfg.api_url);
    println!(
        "admin_token = {}",
        cfg.admin_token.as_deref().map(mask).unwrap_or_else(|| "<unset>".into())
    );

    let client = Client::new(cfg.clone())?;
    match client.request_text(Method::GET, "/health", Auth::None).await {
        Ok(body) => println!("/health     = {}", body.trim()),
        Err(e) => println!("/health     = ERROR ({e})"),
    }

    if cfg.admin_token.is_some() {
        // /auth/me returns SSO session info; admin token will be rejected,
        // but a 401 from a reachable server is still a useful signal.
        match client
            .request_json::<(), Value>(Method::GET, "/auth/me", Auth::None, None, None)
            .await
        {
            Ok(v) => println!("/auth/me    = {}", v),
            Err(e) => println!("/auth/me    = {e}"),
        }
    }
    Ok(())
}

fn mask(s: &str) -> String {
    if s.len() <= 8 {
        return "*".repeat(s.len());
    }
    format!("{}…{}", &s[..4], &s[s.len() - 4..])
}
