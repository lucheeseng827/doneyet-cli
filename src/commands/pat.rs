//! `dyt pat ...` — runtime PAT management against the doneyet admin API.
//!
//! All subcommands here require an existing valid PAT in `--admin-token`
//! (or `$DONEYET_ADMIN_TOKEN`). The first PAT is seeded out-of-band with
//! `doneyet-backend pat-bootstrap`.

use std::io::{IsTerminal, Write};

use anyhow::{anyhow, Result};
use clap::{Args, Subcommand};
use reqwest::Method;
use serde_json::{json, Value};

use crate::cli::{GlobalArgs, OutputFormat};
use crate::client::{Auth, Client};
use crate::config::ResolvedConfig;
use crate::output;
use crate::url::enc_path;

const DAY_S: i64 = 24 * 3600;

#[derive(Debug, Args)]
pub struct PatArgs {
    #[command(subcommand)]
    pub action: PatAction,
}

#[derive(Debug, Subcommand)]
pub enum PatAction {
    /// Mint a new PAT. Plaintext is shown only with --show, --save,
    /// `--output json`, or when stdout is not a TTY.
    Create(CreateArgs),
    /// List all PATs (active + revoked). Plaintext tokens are never returned.
    List,
    /// Revoke a PAT by id. Idempotent for already-revoked rows.
    Revoke(RevokeArgs),
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Human-readable label, unique among active PATs (e.g. "ci-runner").
    #[arg(long)]
    pub label: String,
    /// TTL in days. Required — PATs cannot be issued without an expiry.
    /// Server caps the value at 1825 days (5 years).
    #[arg(long)]
    pub ttl_days: i64,
    /// Optional free-form attribution stored on the row.
    #[arg(long)]
    pub created_by: Option<String>,
    /// Write the plaintext token to this file (0600 on Unix). The token
    /// is NOT echoed to stdout when --save is given.
    #[arg(long, value_name = "PATH")]
    pub save: Option<String>,
    /// Print the plaintext token on stdout even when stdout is a TTY.
    /// Without this flag (and without --save / --output json), the
    /// plaintext is withheld to avoid leaking into terminal scrollback.
    #[arg(long)]
    pub show: bool,
}

#[derive(Debug, Args)]
pub struct RevokeArgs {
    /// PAT id (UUID) as returned by `dyt pat list`.
    pub id: String,
}

pub async fn run(global: &GlobalArgs, args: PatArgs) -> Result<()> {
    let cfg = ResolvedConfig::from_global(global)?;
    let client = Client::new(cfg)?;
    match args.action {
        PatAction::Create(a) => create(&client, global, a).await,
        PatAction::List => list(&client, global).await,
        PatAction::Revoke(a) => revoke(&client, a).await,
    }
}

async fn create(client: &Client, global: &GlobalArgs, a: CreateArgs) -> Result<()> {
    if a.ttl_days <= 0 {
        return Err(anyhow!(
            "--ttl-days must be positive (PATs cannot be issued without an expiry)"
        ));
    }
    let ttl_s = a.ttl_days.saturating_mul(DAY_S);

    let mut body = json!({ "label": a.label, "ttl_s": ttl_s });
    if let Some(by) = &a.created_by {
        body["created_by"] = Value::String(by.clone());
    }
    let resp: Value = client
        .request_json(Method::POST, "/pats", Auth::Admin, Some(&body), None)
        .await?;

    let plaintext = resp
        .get("token")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("server response missing `token` field"))?
        .to_string();

    // --save always wins: the token is the file's payload, never stdout.
    if let Some(path) = &a.save {
        write_secret_file(path, &plaintext)?;
        print_metadata(&resp);
        eprintln!();
        eprintln!("token written to {path} (mode 0600 on Unix)");
        return Ok(());
    }

    // JSON output: emit verbatim. The caller asked for a machine-readable
    // dump, and presumably is piping it somewhere safe.
    if matches!(global.output, OutputFormat::Json) {
        output::print_json(&resp)?;
        return Ok(());
    }

    // Table output. Refuse to print plaintext to a TTY unless --show.
    print_metadata(&resp);
    eprintln!();
    if a.show || !std::io::stdout().is_terminal() {
        // Either operator explicitly asked, or stdout is redirected /
        // piped — let the bytes flow.
        println!("{plaintext}");
        if a.show {
            eprintln!(
                "warning: plaintext token was printed to your terminal. \
                 Clear your scrollback if anyone else can see this session."
            );
        }
    } else {
        eprintln!(
            "token (plaintext withheld — terminal detected). Rerun with one of:\n  \
             --show                 print the token to this terminal\n  \
             --save <path>          write the token to a file (0600)\n  \
             --output json          emit JSON (pipe to `jq -r .token`)\n\n\
             The plaintext is never recoverable — if you can't capture it now, \
             revoke this PAT and mint a fresh one."
        );
        std::process::exit(1);
    }
    Ok(())
}

fn print_metadata(resp: &Value) {
    println!(
        "id        : {}",
        resp.get("id").and_then(Value::as_str).unwrap_or("-")
    );
    println!(
        "label     : {}",
        resp.get("label").and_then(Value::as_str).unwrap_or("-")
    );
    println!(
        "prefix    : {}",
        resp.get("prefix").and_then(Value::as_str).unwrap_or("-")
    );
    println!(
        "created   : {}",
        resp.get("created_at").and_then(Value::as_str).unwrap_or("-")
    );
    println!(
        "expires   : {}",
        resp.get("expires_at").and_then(Value::as_str).unwrap_or("-")
    );
}

#[cfg(unix)]
fn write_secret_file(path: &str, plaintext: &str) -> Result<()> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;

    // Refuse to overwrite an existing file — operators frequently reuse
    // labels, and silently clobbering a previous token is worse than
    // erroring out.
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| anyhow!("opening {path} for write: {e}"))?;
    f.write_all(plaintext.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

#[cfg(not(unix))]
fn write_secret_file(path: &str, plaintext: &str) -> Result<()> {
    use std::fs::OpenOptions;
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| anyhow!("opening {path} for write: {e}"))?;
    f.write_all(plaintext.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

async fn list(client: &Client, global: &GlobalArgs) -> Result<()> {
    let rows: Vec<Value> = client
        .request_json(Method::GET, "/pats", Auth::Admin, None::<&()>, None)
        .await?;
    output::print_table(
        global.output,
        &rows,
        &[
            "id",
            "label",
            "prefix",
            "created_by",
            "created_at",
            "last_used_at",
            "expires_at",
            "expired",
            "revoked_at",
        ],
    )
}

async fn revoke(client: &Client, a: RevokeArgs) -> Result<()> {
    let path = format!("/pats/{}", enc_path(&a.id));
    client
        .request_unit(Method::DELETE, &path, Auth::Admin, None::<&()>, None)
        .await?;
    println!("revoked {}", a.id);
    Ok(())
}
