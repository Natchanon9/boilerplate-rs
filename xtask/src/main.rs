//! Dev tasks, run as `cargo xtask <task>` (stack-architecture.md). Not part
//! of the app.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod check_skills;
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
    /// Check every skill in .agents/skills against the Agent Skills specification
    CheckSkills(check_skills::CheckSkillsArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // `.env` never overrides a variable that is already set, so the environment wins, as
    // for the app (runtime.md AC-2).
    if let Err(err) = dotenvy::dotenv()
        && !err.not_found()
    {
        eprintln!("error: cannot load .env: {}", dotenv_error(&err));
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
        Task::CheckSkills(args) => match check_skills::run(&args) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(err) => {
                eprintln!("error: {err}");
                ExitCode::FAILURE
            }
        },
    }
}

/// A parse error from dotenvy quotes the whole line, which may hold a secret, so only its
/// position is printed (security.md SEC-9).
fn dotenv_error(err: &dotenvy::Error) -> String {
    match err {
        dotenvy::Error::LineParse(_, index) => {
            format!("a line could not be parsed (error at character {index})")
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::dotenv_error;

    #[test]
    fn a_parse_error_does_not_print_the_line() {
        let err = dotenvy::Error::LineParse("AUTH_JWT_SECRET=\"leaked-secret".to_owned(), 16);
        let message = dotenv_error(&err);
        assert!(!message.contains("leaked-secret"));
        assert!(message.contains("character 16"));
    }
}
