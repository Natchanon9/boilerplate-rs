//! Composition root: configuration, the CLI, and the wiring of every adapter
//! (stack-architecture.md). The only crate allowed to use `anyhow`.

pub mod cli;
pub mod commands;
pub mod config;
pub mod features;
mod shutdown;
mod telemetry;
pub mod wiring;

use std::io;

use cli::{Cli, Command, DbCommand, InfoCommand};

/// Runs one CLI command to completion.
pub async fn run(cli: Cli) -> anyhow::Result<()> {
    let Cli { settings, command } = cli;
    if !matches!(command, Command::Info(_)) {
        telemetry::init(&settings.log_level)?;
    }
    match command {
        Command::Serve => commands::run::serve(&settings, shutdown::on_signal()).await?,
        Command::Consume => commands::run::consume(&settings, shutdown::on_signal()).await?,
        Command::All => commands::run::all(&settings, shutdown::on_signal()).await?,
        Command::Db(DbCommand::Migrate) => commands::db::migrate(&settings).await?,
        Command::Db(DbCommand::Rollback) => commands::db::rollback(&settings).await?,
        Command::Db(DbCommand::Seed) => commands::db::seed(&settings).await?,
        Command::Info(InfoCommand::Env) => commands::info::env(&settings, &mut io::stdout())?,
        Command::Info(InfoCommand::Features) => {
            commands::info::features(&settings, &mut io::stdout())?
        }
    }
    Ok(())
}
