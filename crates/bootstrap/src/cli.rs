//! The `app` command line (runtime.md AC-1).

use clap::{Parser, Subcommand};

use crate::config::Settings;

#[derive(Debug, Parser)]
#[command(name = "app", version, about = "rs-boilerplate service")]
pub struct Cli {
    #[command(flatten)]
    pub settings: Settings,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the HTTP routers: protected, public, and hidden
    Serve,
    /// Run the Kafka consumer
    Consume,
    /// Run the HTTP routers and the Kafka consumer in one process (for local development)
    All,
    /// Manage the database schema and demo data
    #[command(subcommand)]
    Db(DbCommand),
    /// Show the configuration
    #[command(subcommand)]
    Info(InfoCommand),
}

#[derive(Debug, Subcommand)]
pub enum InfoCommand {
    /// Print the effective configuration, with secrets redacted
    Env,
    /// List each optional feature as enabled or disabled
    Features,
}

#[derive(Debug, Subcommand)]
pub enum DbCommand {
    /// Apply every pending migration
    Migrate,
    /// Revert the most recently applied migration
    Rollback,
    /// Run the seed files for DB_PLATFORM in file-name order
    Seed,
}
