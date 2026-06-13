use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Method;
use serde_json::{json, Value};

use crate::cli::GlobalArgs;
use crate::client::{Auth, Client};
use crate::config::{write_atomic_0600, ResolvedConfig};
use crate::output;
use crate::url::{enc_path, enc_query};

#[derive(Debug, Args)]
pub struct RunArgs {
    #[command(subcommand)]
    pub action: RunAction,
}

#[derive(Debug, Subcommand)]
pub enum RunAction {
    /// List runs (filter by status / workflow / since / limit).
    List(ListArgs),
    /// Get a single run by id. Requires the run's bearer token.
    Get { id: String },
    /// Start a new run. Returns {id, token}; admin token required.
    Start(StartArgs),
    /// Send a single heartbeat. Requires --run-token.
    Heartbeat(HeartbeatArgs),
    /// Loop heartbeats every --interval seconds until ^C.
    Tail(TailArgs),
    /// Finish a run (status = succeeded | failed). Requires --run-token.
    Finish(FinishArgs),
    /// Cancel a run (admin only).
    Cancel { id: String },
    /// Show the handoff chain (root → leaves) for a run id.
    Chain { id: String },
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub workflow: Option<String>,
    /// ISO-8601 timestamp, e.g. 2025-05-16T00:00:00Z
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long, default_value_t = 100)]
    pub limit: i64,
}

#[derive(Debug, Args)]
pub struct StartArgs {
    /// Workflow slug to start a run for.
    pub slug: String,
    /// Free-form correlation id (CI build #, k8s job uid, etc.).
    #[arg(long)]
    pub external_ref: Option<String>,
    /// Cluster tag — required when accepting a handoff.
    #[arg(long)]
    pub cluster: Option<String>,
    /// Inline JSON object passed verbatim as `metadata`.
    #[arg(long)]
    pub metadata: Option<String>,
    /// Read the metadata blob from a file.
    #[arg(long, value_name = "FILE", conflicts_with = "metadata")]
    pub metadata_file: Option<std::path::PathBuf>,
    /// Accept a handoff. Pair with --handoff-token to authenticate.
    #[arg(long, value_name = "PARENT_RUN_ID")]
    pub continue_from: Option<String>,
    /// One-time handoff token returned by `dyt handoff offer`.
    #[arg(long, env = "DONEYET_HANDOFF_TOKEN", hide_env_values = true)]
    pub handoff_token: Option<String>,
    /// After starting, write the run token to this file (mode 0600).
    #[arg(long, value_name = "FILE")]
    pub token_out: Option<std::path::PathBuf>,
}

#[derive(Debug, Args)]
pub struct HeartbeatArgs {
    pub id: String,
    /// Progress 0-100 (omit for unknown).
    #[arg(long)]
    pub progress: Option<i64>,
    /// Short human-readable status line.
    #[arg(long)]
    pub message: Option<String>,
}

#[derive(Debug, Args)]
pub struct TailArgs {
    pub id: String,
    /// Heartbeat interval, in seconds.
    #[arg(long, default_value_t = 10)]
    pub interval: u64,
    #[arg(long)]
    pub message: Option<String>,
    /// Stop after N heartbeats (default: forever).
    #[arg(long)]
    pub count: Option<u64>,
}

#[derive(Debug, Args)]
pub struct FinishArgs {
    pub id: String,
    #[arg(long, value_parser = ["succeeded", "failed"])]
    pub status: String,
    #[arg(long)]
    pub message: Option<String>,
}

pub async fn run(global: &GlobalArgs, args: RunArgs) -> Result<()> {
    let cfg = ResolvedConfig::from_global(global)?;
    let client = Client::new(cfg)?;
    match args.action {
        RunAction::List(a) => list(&client, global, a).await,
        RunAction::Get { id } => get(&client, global, &id).await,
        RunAction::Start(a) => start(&client, global, a).await,
        RunAction::Heartbeat(a) => heartbeat(&client, a).await,
        RunAction::Tail(a) => tail(&client, a).await,
        RunAction::Finish(a) => finish(&client, global, a).await,
        RunAction::Cancel { id } => cancel(&client, &id).await,
        RunAction::Chain { id } => chain(&client, global, &id).await,
    }
}

async fn list(client: &Client, global: &GlobalArgs, a: ListArgs) -> Result<()> {
    let mut path = format!("/runs?limit={}", a.limit);
    if let Some(s) = a.status {
        path.push_str(&format!("&status={}", enc_query(&s)));
    }
    if let Some(w) = a.workflow {
        path.push_str(&format!("&workflow={}", enc_query(&w)));
    }
    if let Some(s) = a.since {
        path.push_str(&format!("&since={}", enc_query(&s)));
    }
    let rows: Vec<Value> = client
        .request_json(Method::GET, &path, Auth::ReadOptional, None::<&()>, None)
        .await?;
    output::print_table(
        global.output,
        &rows,
        &[
            "id",
            "workflow_slug",
            "status",
            "progress_pct",
            "started_at",
            "finished_at",
        ],
    )
}

async fn get(client: &Client, global: &GlobalArgs, id: &str) -> Result<()> {
    let v: Value = client
        .request_json(
            Method::GET,
            &format!("/runs/{}", enc_path(id)),
            Auth::Run,
            None::<&()>,
            None,
        )
        .await?;
    output::print(global.output, &v)
}

async fn start(client: &Client, global: &GlobalArgs, a: StartArgs) -> Result<()> {
    let metadata: Option<Value> = match (a.metadata.as_deref(), a.metadata_file.as_ref()) {
        (Some(s), _) => Some(
            serde_json::from_str(s)
                .with_context(|| format!("--metadata must be JSON: {s}"))?,
        ),
        (None, Some(p)) => {
            let raw = std::fs::read_to_string(p)
                .with_context(|| format!("reading {}", p.display()))?;
            Some(serde_json::from_str(&raw)
                .with_context(|| format!("parsing {} as JSON", p.display()))?)
        }
        _ => None,
    };

    let mut body = json!({});
    let map = body.as_object_mut().unwrap();
    if let Some(r) = a.external_ref {
        map.insert("external_ref".into(), json!(r));
    }
    if let Some(c) = a.cluster {
        map.insert("cluster".into(), json!(c));
    }
    if let Some(m) = metadata {
        map.insert("metadata".into(), m);
    }

    let mut path = format!("/workflows/{}/runs", enc_path(&a.slug));
    let mut extra = HeaderMap::new();
    let auth;
    if let Some(parent) = a.continue_from {
        path.push_str(&format!("?continue_from={}", enc_query(&parent)));
        let tok = a
            .handoff_token
            .as_deref()
            .context("--continue-from requires --handoff-token")?;
        extra.insert(
            "X-Handoff-Token",
            HeaderValue::from_str(tok).context("bad X-Handoff-Token value")?,
        );
        // The middleware accepts admin OR handoff-token; using None avoids
        // requiring an admin token for the handoff case.
        auth = Auth::None;
    } else {
        auth = Auth::Admin;
    }

    let resp: Value = client
        .request_json(Method::POST, &path, auth, Some(&body), Some(extra))
        .await?;

    if let (Some(token), Some(path)) =
        (resp.get("token").and_then(|v| v.as_str()), a.token_out.as_ref())
    {
        write_secret(path, token)?;
        eprintln!("run token written to {}", path.display());
    }

    output::print(global.output, &resp)
}

async fn heartbeat(client: &Client, a: HeartbeatArgs) -> Result<()> {
    let mut body = json!({});
    let map = body.as_object_mut().unwrap();
    if let Some(p) = a.progress {
        map.insert("progress_pct".into(), json!(p));
    }
    if let Some(m) = a.message {
        map.insert("message".into(), json!(m));
    }
    client
        .request_unit(
            Method::POST,
            &format!("/runs/{}/heartbeat", enc_path(&a.id)),
            Auth::Run,
            Some(&body),
            None,
        )
        .await?;
    println!("ok");
    Ok(())
}

async fn tail(client: &Client, a: TailArgs) -> Result<()> {
    if a.count == Some(0) {
        return Ok(());
    }
    let path = format!("/runs/{}/heartbeat", enc_path(&a.id));
    let mut sent = 0u64;
    let mut ticker = tokio::time::interval(Duration::from_secs(a.interval.max(1)));
    // Block on either the next tick or Ctrl+C. The first tick fires
    // immediately so the first heartbeat goes out without a delay; subsequent
    // ticks wait the full interval. Either branch wakes the loop.
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            _ = tokio::signal::ctrl_c() => {
                eprintln!("stopping");
                break;
            }
        }
        let mut body = json!({});
        if let Some(m) = a.message.as_deref() {
            body.as_object_mut().unwrap().insert("message".into(), json!(m));
        }
        match client
            .request_unit(Method::POST, &path, Auth::Run, Some(&body), None)
            .await
        {
            Ok(()) => {
                sent += 1;
                eprintln!("heartbeat #{sent} ok");
            }
            Err(e) => eprintln!("heartbeat error: {e}"),
        }
        if let Some(max) = a.count {
            if sent >= max {
                break;
            }
        }
    }
    Ok(())
}

async fn finish(client: &Client, global: &GlobalArgs, a: FinishArgs) -> Result<()> {
    let mut body = json!({ "status": a.status });
    if let Some(m) = a.message {
        body.as_object_mut().unwrap().insert("message".into(), json!(m));
    }
    let resp: Value = client
        .request_json(
            Method::POST,
            &format!("/runs/{}/finish", enc_path(&a.id)),
            Auth::Run,
            Some(&body),
            None,
        )
        .await?;
    output::print(global.output, &resp)
}

async fn cancel(client: &Client, id: &str) -> Result<()> {
    client
        .request_unit::<()>(
            Method::POST,
            &format!("/runs/{}/cancel", enc_path(id)),
            Auth::Admin,
            None,
            None,
        )
        .await?;
    println!("cancelled {id}");
    Ok(())
}

async fn chain(client: &Client, global: &GlobalArgs, id: &str) -> Result<()> {
    let rows: Vec<Value> = client
        .request_json(
            Method::GET,
            &format!("/runs/{}/chain", enc_path(id)),
            Auth::ReadOptional,
            None::<&()>,
            None,
        )
        .await?;
    output::print_table(
        global.output,
        &rows,
        &["id", "workflow_slug", "status", "cluster", "started_at", "finished_at"],
    )
}

fn write_secret(path: &std::path::Path, value: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
    }
    write_atomic_0600(path, value.as_bytes())
}
