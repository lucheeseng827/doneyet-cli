use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{json, Value};

use crate::cli::GlobalArgs;
use crate::client::{Auth, Client};
use crate::config::ResolvedConfig;
use crate::output;
use crate::url::{enc_path, enc_query};

#[derive(Debug, Args)]
pub struct NotifyArgs {
    #[command(subcommand)]
    pub action: NotifyAction,
}

#[derive(Debug, Subcommand)]
pub enum NotifyAction {
    /// Manage notification channels.
    #[command(subcommand_required = true, arg_required_else_help = true)]
    Channel(ChannelArgs),
    /// List notifications (filter by channel / workflow / run / status / event).
    List(ListArgs),
    /// Get one notification by id.
    Get { id: String },
    /// Create a notification.
    Create(CreateArgs),
    /// Update a notification (status / retry_count / mark_delivered / last_error).
    Update(UpdateArgs),
}

#[derive(Debug, Args)]
pub struct ChannelArgs {
    #[command(subcommand)]
    pub action: ChannelAction,
}

#[derive(Debug, Subcommand)]
pub enum ChannelAction {
    List,
    Create(ChannelCreateArgs),
    Delete { id: String },
}

#[derive(Debug, Args)]
pub struct ChannelCreateArgs {
    #[arg(long, value_parser = ["slack", "pagerduty", "opsgenie", "email", "webhook"])]
    pub kind: String,
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub target: String,
    /// Inline JSON object for transport-specific options.
    #[arg(long)]
    pub config: Option<String>,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long)]
    pub channel_id: Option<String>,
    #[arg(long)]
    pub workflow_slug: Option<String>,
    #[arg(long)]
    pub run_id: Option<String>,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub event_type: Option<String>,
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    #[arg(long)]
    pub channel_id: Option<String>,
    #[arg(long)]
    pub channel_name: Option<String>,
    #[arg(long)]
    pub workflow_slug: Option<String>,
    #[arg(long)]
    pub run_id: Option<String>,
    #[arg(long, value_parser = ["run_stalled", "run_failed", "run_overrun", "sla_breach", "manual"])]
    pub event_type: String,
    #[arg(long, value_parser = ["info", "warning", "critical"])]
    pub severity: String,
    #[arg(long)]
    pub title: String,
    #[arg(long)]
    pub body: Option<String>,
    /// Inline JSON object for `payload`.
    #[arg(long)]
    pub payload: Option<String>,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    pub id: String,
    #[arg(long, value_parser = ["pending", "delivered", "failed", "dead"])]
    pub status: Option<String>,
    #[arg(long)]
    pub retry_count: Option<i64>,
    #[arg(long)]
    pub last_error: Option<String>,
    #[arg(long)]
    pub mark_delivered: bool,
}

pub async fn run(global: &GlobalArgs, args: NotifyArgs) -> Result<()> {
    let cfg = ResolvedConfig::from_global(global)?;
    let client = Client::new(cfg)?;
    match args.action {
        NotifyAction::Channel(c) => channel(&client, global, c).await,
        NotifyAction::List(a) => list(&client, global, a).await,
        NotifyAction::Get { id } => get(&client, global, &id).await,
        NotifyAction::Create(a) => create(&client, global, a).await,
        NotifyAction::Update(a) => update(&client, global, a).await,
    }
}

async fn channel(client: &Client, global: &GlobalArgs, args: ChannelArgs) -> Result<()> {
    match args.action {
        ChannelAction::List => {
            let rows: Vec<Value> = client
                .request_json(
                    Method::GET,
                    "/notifications/channels",
                    Auth::ReadOptional,
                    None::<&()>,
                    None,
                )
                .await?;
            output::print_table(
                global.output,
                &rows,
                &["id", "kind", "name", "target", "created_at"],
            )
        }
        ChannelAction::Create(a) => {
            let mut body = json!({
                "kind": a.kind,
                "name": a.name,
                "target": a.target,
            });
            if let Some(c) = a.config {
                let parsed: Value = serde_json::from_str(&c)
                    .with_context(|| format!("--config must be JSON: {c}"))?;
                body.as_object_mut().unwrap().insert("config".into(), parsed);
            }
            let v: Value = client
                .request_json(
                    Method::POST,
                    "/notifications/channels",
                    Auth::Admin,
                    Some(&body),
                    None,
                )
                .await?;
            output::print(global.output, &v)
        }
        ChannelAction::Delete { id } => {
            client
                .request_unit::<()>(
                    Method::DELETE,
                    &format!("/notifications/channels/{}", enc_path(&id)),
                    Auth::Admin,
                    None,
                    None,
                )
                .await?;
            println!("deleted {id}");
            Ok(())
        }
    }
}

async fn list(client: &Client, global: &GlobalArgs, a: ListArgs) -> Result<()> {
    let mut path = String::from("/notifications");
    let mut q = Vec::new();
    if let Some(v) = a.channel_id {
        q.push(format!("channel_id={}", enc_query(&v)));
    }
    if let Some(v) = a.workflow_slug {
        q.push(format!("workflow_slug={}", enc_query(&v)));
    }
    if let Some(v) = a.run_id {
        q.push(format!("run_id={}", enc_query(&v)));
    }
    if let Some(v) = a.status {
        q.push(format!("status={}", enc_query(&v)));
    }
    if let Some(v) = a.event_type {
        q.push(format!("event_type={}", enc_query(&v)));
    }
    if !q.is_empty() {
        path.push('?');
        path.push_str(&q.join("&"));
    }
    let rows: Vec<Value> = client
        .request_json(Method::GET, &path, Auth::ReadOptional, None::<&()>, None)
        .await?;
    output::print_table(
        global.output,
        &rows,
        &[
            "id",
            "event_type",
            "severity",
            "title",
            "status",
            "retry_count",
            "created_at",
        ],
    )
}

async fn get(client: &Client, global: &GlobalArgs, id: &str) -> Result<()> {
    let v: Value = client
        .request_json(
            Method::GET,
            &format!("/notifications/{}", enc_path(id)),
            Auth::ReadOptional,
            None::<&()>,
            None,
        )
        .await?;
    output::print(global.output, &v)
}

async fn create(client: &Client, global: &GlobalArgs, a: CreateArgs) -> Result<()> {
    let mut body = json!({
        "event_type": a.event_type,
        "severity": a.severity,
        "title": a.title,
    });
    let map = body.as_object_mut().unwrap();
    if let Some(v) = a.channel_id {
        map.insert("channel_id".into(), json!(v));
    }
    if let Some(v) = a.channel_name {
        map.insert("channel_name".into(), json!(v));
    }
    if let Some(v) = a.workflow_slug {
        map.insert("workflow_slug".into(), json!(v));
    }
    if let Some(v) = a.run_id {
        map.insert("run_id".into(), json!(v));
    }
    if let Some(v) = a.body {
        map.insert("body".into(), json!(v));
    }
    if let Some(p) = a.payload {
        let parsed: Value = serde_json::from_str(&p)
            .with_context(|| format!("--payload must be JSON: {p}"))?;
        map.insert("payload".into(), parsed);
    }
    let v: Value = client
        .request_json(Method::POST, "/notifications", Auth::Admin, Some(&body), None)
        .await?;
    output::print(global.output, &v)
}

async fn update(client: &Client, global: &GlobalArgs, a: UpdateArgs) -> Result<()> {
    let mut body = json!({});
    let map = body.as_object_mut().unwrap();
    if let Some(s) = a.status {
        map.insert("status".into(), json!(s));
    }
    if let Some(r) = a.retry_count {
        map.insert("retry_count".into(), json!(r));
    }
    if let Some(e) = a.last_error {
        map.insert("last_error".into(), json!(e));
    }
    if a.mark_delivered {
        map.insert("mark_delivered".into(), json!(true));
    }
    if map.is_empty() {
        anyhow::bail!(
            "no mutable fields provided — pass at least one of --status, --retry-count, --last-error, or --mark-delivered"
        );
    }
    let v: Value = client
        .request_json(
            Method::PATCH,
            &format!("/notifications/{}", enc_path(&a.id)),
            Auth::Admin,
            Some(&body),
            None,
        )
        .await?;
    output::print(global.output, &v)
}
