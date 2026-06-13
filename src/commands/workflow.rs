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
pub struct WorkflowArgs {
    #[command(subcommand)]
    pub action: WorkflowAction,
}

#[derive(Debug, Subcommand)]
pub enum WorkflowAction {
    /// List all workflows.
    List,
    /// Get one workflow by slug.
    Get { slug: String },
    /// Create or update a workflow (POST /workflows).
    Upsert(UpsertArgs),
    /// Soft-delete a workflow.
    Delete { slug: String },
    /// Print the Backstage catalog YAML for a workflow.
    Catalog { slug: String },
}

#[derive(Debug, Args)]
pub struct UpsertArgs {
    /// Slug — stable url-safe identifier.
    #[arg(long)]
    pub slug: String,

    /// Human name.
    #[arg(long)]
    pub name: String,

    /// Optional description shown on the dashboard.
    #[arg(long)]
    pub description: Option<String>,

    /// Expected duration in seconds; used for overrun detection. Must be >= 0.
    #[arg(long, allow_hyphen_values = true)]
    pub expected_duration_s: Option<i64>,

    /// Grace window in seconds before missing heartbeats flip the run to "stalled". Must be >= 0.
    #[arg(long, allow_hyphen_values = true)]
    pub heartbeat_grace_s: Option<i64>,

    /// Owner team or user reference (Backstage-style: group:default/payments).
    #[arg(long)]
    pub owner: Option<String>,

    /// Read the full request body from this JSON file. Other flags merge in on top.
    #[arg(long, value_name = "FILE")]
    pub from_file: Option<std::path::PathBuf>,
}

pub async fn run(global: &GlobalArgs, args: WorkflowArgs) -> Result<()> {
    let cfg = ResolvedConfig::from_global(global)?;
    let client = Client::new(cfg)?;
    match args.action {
        WorkflowAction::List => list(&client, global).await,
        WorkflowAction::Get { slug } => get(&client, global, &slug).await,
        WorkflowAction::Upsert(a) => upsert(&client, global, a).await,
        WorkflowAction::Delete { slug } => delete(&client, &slug).await,
        WorkflowAction::Catalog { slug } => catalog(&client, &slug).await,
    }
}

async fn list(client: &Client, global: &GlobalArgs) -> Result<()> {
    let rows: Vec<Value> = client
        .request_json(Method::GET, "/workflows", Auth::ReadOptional, None::<&()>, None)
        .await?;
    output::print_table(global.output, &rows, &["slug", "name", "owner", "created_at"])
}

async fn get(client: &Client, global: &GlobalArgs, slug: &str) -> Result<()> {
    let v: Value = client
        .request_json(
            Method::GET,
            &format!("/workflows/{}", enc_path(slug)),
            Auth::ReadOptional,
            None::<&()>,
            None,
        )
        .await?;
    output::print_object_table(
        global.output,
        &v,
        &[
            ("slug", "slug"),
            ("name", "name"),
            ("description", "description"),
            ("owner", "owner"),
            ("expected_duration_s", "expected_duration_s"),
            ("heartbeat_grace_s", "heartbeat_grace_s"),
            ("created_at", "created_at"),
            ("deleted_at", "deleted_at"),
        ],
    )
}

async fn upsert(client: &Client, global: &GlobalArgs, a: UpsertArgs) -> Result<()> {
    if a.expected_duration_s.is_some_and(|v| v < 0) {
        anyhow::bail!("--expected-duration-s must be non-negative");
    }
    if a.heartbeat_grace_s.is_some_and(|v| v < 0) {
        anyhow::bail!("--heartbeat-grace-s must be non-negative");
    }

    let mut body = if let Some(path) = a.from_file.as_ref() {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str::<Value>(&raw)
            .with_context(|| format!("parsing {} as JSON", path.display()))?
    } else {
        json!({})
    };

    let map = body.as_object_mut().context("--from-file must be a JSON object")?;
    map.insert("slug".into(), json!(a.slug));
    map.insert("name".into(), json!(a.name));
    if let Some(d) = a.description {
        map.insert("description".into(), json!(d));
    }
    if let Some(d) = a.expected_duration_s {
        map.insert("expected_duration_s".into(), json!(d));
    }
    if let Some(d) = a.heartbeat_grace_s {
        map.insert("heartbeat_grace_s".into(), json!(d));
    }
    if let Some(o) = a.owner {
        map.insert("owner".into(), json!(o));
    }

    let v: Value = client
        .request_json(Method::POST, "/workflows", Auth::Admin, Some(&body), None)
        .await?;
    output::print(global.output, &v)
}

async fn delete(client: &Client, slug: &str) -> Result<()> {
    client
        .request_unit::<()>(
            Method::DELETE,
            &format!("/workflows/{}", enc_path(slug)),
            Auth::Admin,
            None,
            None,
        )
        .await?;
    println!("deleted {slug}");
    Ok(())
}

async fn catalog(client: &Client, slug: &str) -> Result<()> {
    let body = client
        .request_text(
            Method::GET,
            &format!("/workflows/{}/catalog-info", enc_path(slug)),
            Auth::ReadOptional,
        )
        .await?;
    print!("{body}");
    Ok(())
}
