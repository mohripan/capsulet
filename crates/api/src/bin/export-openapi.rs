use std::{env, fs, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let artifact = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("openapi.json");
    let generated = capsulet_api::canonical_openapi_json()
        .map_err(|error| format!("could not serialize OpenAPI: {error}"))?;
    if env::args().any(|argument| argument == "--check") {
        let checked = fs::read_to_string(&artifact)
            .map_err(|error| format!("could not read {}: {error}", artifact.display()))?;
        if checked != generated {
            return Err(format!(
                "{} is stale; run `cargo run -p capsulet-api --bin export-openapi`",
                artifact.display()
            ));
        }
        return Ok(());
    }
    fs::write(&artifact, generated)
        .map_err(|error| format!("could not write {}: {error}", artifact.display()))
}
