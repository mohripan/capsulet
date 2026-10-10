//! Cross-platform repository verification orchestration.

mod security;
pub mod verify;

/// Runs the xtask command using already-tokenized arguments.
///
/// # Errors
///
/// Returns a diagnostic for invalid arguments, missing prerequisites, failed gates, or timeouts.
pub fn run(arguments: &[String]) -> Result<(), String> {
    if arguments
        .first()
        .is_some_and(|command| command == "security")
    {
        return security::run(&arguments[1..]);
    }
    verify::run(arguments)
}
