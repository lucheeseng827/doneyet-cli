use anyhow::Result;
use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{json, Value};

use crate::cli::GlobalArgs;
use crate::client::{Auth, Client};
use crate::config::ResolvedConfig;
use crate::output;
use crate::url::{enc_path, enc_query};

#[derive(Debug, Args)]
pub struct HandoffArgs {
    #[command(subcommand)]
    pub action: HandoffAction,
}

#[derive(Debug, Subcommand)]
pub enum HandoffAction {
    /// List handoffs (filter by status / from_run_id / limit).
    List(ListArgs),
    /// Offer a handoff for a run. Admin-only.
    Offer(OfferArgs),
    /// Expire stale offered handoffs (admin sweep).
    Expire,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub from_run_id: Option<String>,
    #[arg(long, default_value_t = 100)]
    pub limit: i64,
}

#[derive(Debug, Args)]
pub struct OfferArgs {
    /// Source run id (the one being handed off).
    pub run_id: String,
    /// Cluster tag to advertise as the handoff target.
    #[arg(long)]
    pub to_cluster: Option<String>,
    /// Override the default handoff token TTL.
    #[arg(long)]
    pub ttl_s: Option<i64>,
}

pub async fn run(global: &GlobalArgs, args: HandoffArgs) -> Result<()> {
    let cfg = ResolvedConfig::from_global(global)?;
    let client = Client::new(cfg)?;
    match args.action {
        HandoffAction::List(a) => list(&client, global, a).await,
        HandoffAction::Offer(a) => offer(&client, global, a).await,
        HandoffAction::Expire => expire(&client, global).await,
    }
}

async fn list(client: &Client, global: &GlobalArgs, a: ListArgs) -> Result<()> {
    let mut path = format!("/handoffs?limit={}", a.limit);
    if let Some(s) = a.status {
        path.push_str(&format!("&status={}", enc_query(&s)));
    }
    if let Some(r) = a.from_run_id {
        path.push_str(&format!("&from_run_id={}", enc_query(&r)));
    }
    let rows: Vec<Value> = client
        .request_json(Method::GET, &path, Auth::ReadOptional, None::<&()>, None)
        .await?;
    output::print_table(
        global.output,
        &rows,
        &["id", "from_run_id", "to_run_id", "to_cluster", "status", "expires_at"],
    )
}

async fn offer(client: &Client, global: &GlobalArgs, a: OfferArgs) -> Result<()> {
    let mut body = json!({});
    let map = body.as_object_mut().unwrap();
    if let Some(c) = a.to_cluster {
        map.insert("to_cluster".into(), json!(c));
    }
    if let Some(t) = a.ttl_s {
        map.insert("ttl_s".into(), json!(t));
    }
    let v: Value = client
        .request_json(
            Method::POST,
            &format!("/runs/{}/handoff", enc_path(&a.run_id)),
            Auth::Admin,
            Some(&body),
            None,
        )
        .await?;
    output::print(global.output, &v)
}

async fn expire(client: &Client, global: &GlobalArgs) -> Result<()> {
    let v: Value = client
        .request_json(
            Method::POST,
            "/handoffs/expire",
            Auth::Admin,
            None::<&()>,
            None,
        )
        .await?;
    output::print(global.output, &v)
}
