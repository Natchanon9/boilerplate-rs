//! Dev tasks, run as `cargo xtask <task>` (stack-architecture.md). Not part
//! of the app.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod produce;
mod token;

#[derive(Debug, Parser)]
#[command(
    name = "cargo xtask",
    bin_name = "cargo xtask",
    about = "Development tasks for rs-boilerplate"
)]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Debug, Subcommand)]
enum Task {
    /// Sign a dev access token with AUTH_JWT_SECRET and print it
    Token(token::TokenArgs),
    /// Send a command envelope, or a raw value, to a resource's commands topic
    KafkaProduce(produce::ProduceArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // `.env` never overrides a variable that is already set, so the environment wins, as
    // for the app (runtime.md AC-2).
    if let Err(err) = dotenvy::dotenv()
        && !err.not_found()
    {
        eprintln!("error: cannot load .env: {err}");
        return ExitCode::FAILURE;
    }
    match cli.task {
        Task::Token(args) => match token::sign_from_env(&args, |name| std::env::var(name).ok()) {
            Ok(token) => {
                println!("{token}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("error: {err}");
                ExitCode::FAILURE
            }
        },
        Task::KafkaProduce(args) => match produce::run(&args, |name| std::env::var(name).ok()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                ExitCode::FAILURE
            }
        },
    }
}
