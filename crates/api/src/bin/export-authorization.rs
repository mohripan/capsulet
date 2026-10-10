use std::{env, fs, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/contracts/authorization-matrix.md");
    let generated = capsulet_api::authorization_matrix_markdown();
    let result = if env::args().any(|argument| argument == "--check") {
        fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|checked| {
                if checked.replace("\r\n", "\n") == generated {
                    Ok(())
                } else {
                    Err("authorization-matrix.md is stale; run export-authorization".to_string())
                }
            })
    } else {
        fs::write(path, generated).map_err(|error| error.to_string())
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
