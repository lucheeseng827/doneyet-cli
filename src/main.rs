use anyhow::Result;
use clap::Parser;

mod cli;
mod client;
mod commands;
mod config;
mod output;
mod url;

use cli::{Cli, Command};

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Config(args) => commands::config::run(args).await,
        Command::Login(args) => commands::auth::login(args).await,
        Command::Logout => commands::auth::logout().await,
        Command::Whoami(args) => commands::auth::whoami(&cli.global, args).await,
        Command::Health(args) => commands::system::health(&cli.global, args).await,
        Command::Workflow(args) => commands::workflow::run(&cli.global, args).await,
        Command::Run(args) => commands::run::run(&cli.global, args).await,
        Command::Step(args) => commands::step::run(&cli.global, args).await,
        Command::Handoff(args) => commands::handoff::run(&cli.global, args).await,
        Command::Overview(args) => commands::overview::run(&cli.global, args).await,
        Command::Notify(args) => commands::notify::run(&cli.global, args).await,
        Command::Sla(args) => commands::sla::run(&cli.global, args).await,
        Command::Pat(args) => commands::pat::run(&cli.global, args).await,
        Command::Completion(args) => commands::system::completion(args),
    }
}
