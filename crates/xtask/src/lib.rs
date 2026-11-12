//! Cross-platform repository verification orchestration.

mod claims;
mod fixtures;
mod release;
mod security;
pub mod verify;

/// Runs the xtask command using already-tokenized arguments.
///
/// # Errors
///
/// Returns a diagnostic for invalid arguments, missing prerequisites, failed gates, or timeouts.
pub fn run(arguments: &[String]) -> Result<(), String> {
    match arguments.first().map(String::as_str) {
        Some("security") => return security::run(&arguments[1..]),
        Some("claims") => return claims::run(&arguments[1..]),
        Some("stack") => return verify::stack(&arguments[1..]),
        Some("release") => return release::run(&arguments[1..]),
        _ => {}
    }
    verify::run(arguments)
}
