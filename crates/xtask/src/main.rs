use std::{env, process::ExitCode, thread, time::Duration};

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .first()
        .is_some_and(|argument| argument == "__test-child")
    {
        return match arguments.get(1).map(String::as_str) {
            Some("fail") => ExitCode::FAILURE,
            Some("sleep") => {
                thread::sleep(Duration::from_secs(5));
                ExitCode::SUCCESS
            }
            _ => ExitCode::SUCCESS,
        };
    }
    match capsulet_xtask::run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
