use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{json, Value};

use crate::cli::GlobalArgs;
use crate::client::{Auth, Client};
use crate::config::ResolvedConfig;
use crate::output;
use crate::url::enc_path;

#[derive(Debug, Args)]
pub struct StepArgs {
    #[command(subcommand)]
    pub action: StepAction,
}

#[derive(Debug, Subcommand)]
pub enum StepAction {
    /// Create a step under a run. Requires --run-token.
    Create(CreateArgs),
    /// Update an existing step. Requires --run-token.
    Update(UpdateArgs),
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Run id.
    pub run_id: String,
    /// Step name.
    #[arg(long)]
    pub name: String,
    /// Order index (default: server-assigned next).
    #[arg(long)]
    pub sort_order: Option<i64>,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    pub run_id: String,
    pub step_id: String,
    #[arg(long, value_parser = ["queued", "running", "succeeded", "failed", "cancelled"])]
    pub status: Option<String>,
    #[arg(long)]
    pub message: Option<String>,
    /// Inline JSON object stored as the step checkpoint. Pass `null` (literal) to clear.
    #[arg(long)]
    pub checkpoint: Option<String>,
}

pub async fn run(global: &GlobalArgs, args: StepArgs) -> Result<()> {
    let cfg = ResolvedConfig::from_global(global)?;
    let client = Client::new(cfg)?;
    match args.action {
        StepAction::Create(a) => create(&client, global, a).await,
        StepAction::Update(a) => update(&client, global, a).await,
    }
}

async fn create(client: &Client, global: &GlobalArgs, a: CreateArgs) -> Result<()> {
    let mut body = json!({ "name": a.name });
    if let Some(s) = a.sort_order {
        body.as_object_mut().unwrap().insert("sort_order".into(), json!(s));
    }
    let v: Value = client
        .request_json(
            Method::POST,
            &format!("/runs/{}/steps", enc_path(&a.run_id)),
            Auth::Run,
            Some(&body),
            None,
        )
        .await?;
    output::print(global.output, &v)
}

async fn update(client: &Client, global: &GlobalArgs, a: UpdateArgs) -> Result<()> {
    let mut body = json!({});
    let map = body.as_object_mut().unwrap();
    if let Some(s) = a.status {
        map.insert("status".into(), json!(s));
    }
    if let Some(m) = a.message {
        map.insert("message".into(), json!(m));
    }
    if let Some(c) = a.checkpoint {
        let parsed: Value = serde_json::from_str(&c)
            .with_context(|| format!("--checkpoint must be JSON: {c}"))?;
        map.insert("checkpoint".into(), parsed);
    }
    if map.is_empty() {
        anyhow::bail!(
            "no fields provided for update — pass at least one of --status, --message, or --checkpoint"
        );
    }
    let v: Value = client
        .request_json(
            Method::PATCH,
            &format!(
                "/runs/{}/steps/{}",
                enc_path(&a.run_id),
                enc_path(&a.step_id)
            ),
            Auth::Run,
            Some(&body),
            None,
        )
        .await?;
    output::print(global.output, &v)
}
