use anyhow::Result;
use clap::Args;
use clap_complete::{generate, Shell};
use reqwest::Method;

use crate::cli::{Cli, GlobalArgs};
use crate::client::{Auth, Client};
use crate::config::ResolvedConfig;

#[derive(Debug, Args)]
pub struct HealthArgs {}

#[derive(Debug, Args)]
pub struct CompletionArgs {
    /// Shell to generate completions for.
    #[arg(value_enum)]
    pub shell: Shell,
}

pub async fn health(global: &GlobalArgs, _args: HealthArgs) -> Result<()> {
    let cfg = ResolvedConfig::from_global(global)?;
    let client = Client::new(cfg)?;
    let body = client.request_text(Method::GET, "/health", Auth::None).await?;
    println!("{}", body.trim());
    Ok(())
}

pub fn completion(args: CompletionArgs) -> Result<()> {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(args.shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}
