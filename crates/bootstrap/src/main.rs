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
        eprintln!("error: cannot load .env: {}", dotenv_error(&err));
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
