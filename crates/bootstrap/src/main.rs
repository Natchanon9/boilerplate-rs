use std::process::ExitCode;

use clap::Parser;

use bootstrap::cli::Cli;

#[tokio::main]
async fn main() -> ExitCode {
    // `.env` first, without overriding variables that are already set, so the
    // precedence is flag > env > `.env` (runtime.md AC-2).
    if let Err(err) = dotenvy::dotenv()
        && !err.not_found()
    {
        eprintln!("error: cannot load .env: {err}");
        return ExitCode::FAILURE;
    }

    let cli = Cli::parse();
    match bootstrap::run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
