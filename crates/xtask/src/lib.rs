//! Cross-platform repository verification orchestration.

pub mod verify;

/// Runs the xtask command using already-tokenized arguments.
///
/// # Errors
///
/// Returns a diagnostic for invalid arguments, missing prerequisites, failed gates, or timeouts.
pub fn run(arguments: &[String]) -> Result<(), String> {
    verify::run(arguments)
}
