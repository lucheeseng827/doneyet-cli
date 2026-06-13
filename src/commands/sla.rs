use anyhow::Result;
use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::Value;

use crate::cli::GlobalArgs;
use crate::client::{Auth, Client};
use crate::config::ResolvedConfig;
use crate::output;
use crate::url::enc_query;

#[derive(Debug, Args)]
pub struct SlaArgs {
    #[command(subcommand)]
    pub action: SlaAction,
}

#[derive(Debug, Subcommand)]
pub enum SlaAction {
    /// Runs whose expected finish is within --within (e.g. 30m, 2h).
    AtRisk {
        #[arg(long, default_value = "30m")]
        within: String,
    },
}

pub async fn run(global: &GlobalArgs, args: SlaArgs) -> Result<()> {
    let cfg = ResolvedConfig::from_global(global)?;
    let client = Client::new(cfg)?;
    match args.action {
        SlaAction::AtRisk { within } => at_risk(&client, global, &within).await,
    }
}

async fn at_risk(client: &Client, global: &GlobalArgs, within: &str) -> Result<()> {
    let rows: Vec<Value> = client
        .request_json(
            Method::GET,
            &format!("/sla/at-risk?within={}", enc_query(within)),
            Auth::ReadOptional,
            None::<&()>,
            None,
        )
        .await?;
    output::print_table(
        global.output,
        &rows,
        &["run_id", "workflow_slug", "expected_finish", "seconds_to_sla"],
    )
}
