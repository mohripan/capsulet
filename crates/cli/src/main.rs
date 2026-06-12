use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    capsulet_cli::run().await
}
