use std::{env, process::ExitCode, thread, time::Duration};

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .first()
        .is_some_and(|argument| argument == "__test-child")
    {
        return match arguments.get(1).map(String::as_str) {
            Some("fail") => ExitCode::FAILURE,
            // What a test binary's run looks like in a gate log, for the tests of
            // the verifier's test counts.
            Some("report") => {
                println!("     Running tests/fake.rs (target/debug/deps/fake-0)");
                println!(
                    "test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s"
                );
                ExitCode::SUCCESS
            }
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
