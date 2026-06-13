use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "dyt",
    version,
    about = "doneyet CLI — interact with the doneyet API without writing curl",
    long_about = "dyt is the official command-line client for the doneyet workflow tracker.\n\
                  It wraps the REST API so producers and operators never need to assemble \n\
                  Authorization headers or JSON bodies by hand."
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Args, Clone)]
pub struct GlobalArgs {
    /// Base URL of the doneyet API, e.g. http://localhost:3001
    /// (overrides the value stored in the config file).
    #[arg(long, global = true, env = "DONEYET_API_URL")]
    pub api_url: Option<String>,

    /// Admin bearer token for write-side calls.
    #[arg(long, global = true, env = "DONEYET_ADMIN_TOKEN", hide_env_values = true)]
    pub admin_token: Option<String>,

    /// Per-run bearer token (for heartbeat / step / finish calls).
    #[arg(long, global = true, env = "DONEYET_RUN_TOKEN", hide_env_values = true)]
    pub run_token: Option<String>,

    /// Username:password for Basic-auth read endpoints when the server has
    /// DONEYET_READ_BASIC configured.
    #[arg(long, global = true, env = "DONEYET_READ_BASIC", hide_env_values = true)]
    pub read_basic: Option<String>,

    /// Output format. `table` is human-friendly, `json` is pipeable.
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Table)]
    pub output: OutputFormat,

    /// Skip TLS verification (development clusters with self-signed certs).
    #[arg(long, global = true)]
    pub insecure: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show or update the stored CLI configuration (api-url, default token).
    Config(crate::commands::config::ConfigArgs),

    /// Save an admin token to the config file (prompts if not provided).
    Login(crate::commands::auth::LoginArgs),

    /// Forget the stored admin token.
    Logout,

    /// Print the resolved config and ping the server.
    Whoami(crate::commands::auth::WhoamiArgs),

    /// Ping the /health endpoint.
    Health(crate::commands::system::HealthArgs),

    /// Manage workflows.
    #[command(subcommand_required = true, arg_required_else_help = true)]
    Workflow(crate::commands::workflow::WorkflowArgs),

    /// Start / observe / control runs.
    #[command(subcommand_required = true, arg_required_else_help = true)]
    Run(crate::commands::run::RunArgs),

    /// Create / update steps inside a run.
    #[command(subcommand_required = true, arg_required_else_help = true)]
    Step(crate::commands::step::StepArgs),

    /// Inspect or offer cross-cluster handoffs.
    #[command(subcommand_required = true, arg_required_else_help = true)]
    Handoff(crate::commands::handoff::HandoffArgs),

    /// Dashboard counts + recent runs.
    Overview(crate::commands::overview::OverviewArgs),

    /// Notifications and notification channels.
    #[command(subcommand_required = true, arg_required_else_help = true)]
    Notify(crate::commands::notify::NotifyArgs),

    /// SLA-at-risk queries.
    #[command(subcommand_required = true, arg_required_else_help = true)]
    Sla(crate::commands::sla::SlaArgs),

    /// Manage personal access tokens (create / list / revoke).
    #[command(subcommand_required = true, arg_required_else_help = true)]
    Pat(crate::commands::pat::PatArgs),

    /// Print shell completions to stdout.
    Completion(crate::commands::system::CompletionArgs),
}
