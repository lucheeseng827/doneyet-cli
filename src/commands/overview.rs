use anyhow::Result;
use clap::Args;
use reqwest::Method;
use serde_json::Value;

use crate::cli::GlobalArgs;
use crate::client::{Auth, Client};
use crate::config::ResolvedConfig;
use crate::output;

#[derive(Debug, Args)]
pub struct OverviewArgs {}

pub async fn run(global: &GlobalArgs, _args: OverviewArgs) -> Result<()> {
    let cfg = ResolvedConfig::from_global(global)?;
    let client = Client::new(cfg)?;
    let v: Value = client
        .request_json(Method::GET, "/overview", Auth::ReadOptional, None::<&()>, None)
        .await?;
    output::print(global.output, &v)
}
