//! `capsulet-replay` — check a certificate against the evidence it cites.
//!
//! Usage: `capsulet-replay <bundle.json>`
//!
//! Exits 0 when the certificate still justifies its recorded verdict, 1 when it
//! does not, and 2 when the bundle cannot be read at all. The distinction
//! matters to a script: "this certificate is wrong" and "I could not open the
//! file" are different problems.
//!
//! This program reads one file and prints to standard output. It opens no
//! socket, contacts no service, and consults no clock. That is not a promise in
//! a comment — `tests/cli.rs` asserts it over the dependency closure, and the
//! verification gate runs the built binary with its environment scrubbed and no
//! service running.

use std::path::PathBuf;
use std::process::ExitCode;

use capsulet_kernel::bundle::Bundle;
use capsulet_kernel::replay::{ReplayFinding, ReplayNote, ReplayOutcome, replay};

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let Some(path) = arguments.next().map(PathBuf::from) else {
        eprintln!("usage: capsulet-replay <bundle.json>");
        return ExitCode::from(2);
    };

    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("cannot read {}: {error}", path.display());
            return ExitCode::from(2);
        }
    };

    let bundle = match Bundle::read(&bytes) {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::from(2);
        }
    };

    let evidence = match bundle.evidence() {
        Ok(evidence) => evidence,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::from(2);
        }
    };

    let recorded = bundle.certificate.verdict();
    let outcome = replay(&bundle.certificate, &evidence);
    report(&outcome, recorded)
}

fn report(outcome: &ReplayOutcome, recorded: capsulet_ir::AssuranceVerdict) -> ExitCode {
    match outcome {
        ReplayOutcome::Reproduced { verdict, notes } => {
            println!("reproduced: {}", verdict.as_str());
            for note in notes {
                println!("  note: {}", describe_note(note));
            }
            ExitCode::SUCCESS
        }
        ReplayOutcome::Diverged {
            recomputed,
            findings,
            notes,
            ..
        } => {
            println!("diverged");
            println!("  recorded:   {}", recorded.as_str());
            println!("  recomputed: {}", recomputed.as_str());
            for finding in findings {
                println!("  finding: {}", describe_finding(finding));
            }
            for note in notes {
                println!("  note: {}", describe_note(note));
            }
            ExitCode::FAILURE
        }
        ReplayOutcome::Unreadable { reason } => {
            eprintln!("unreadable: {reason:?}");
            ExitCode::from(2)
        }
    }
}

/// Says what went wrong in a sentence, and names the digest where one is
/// involved, so a mismatch can be chased without a debugger.
fn describe_finding(finding: &ReplayFinding) -> String {
    match finding {
        ReplayFinding::SealBroken => {
            "the certificate has been altered since it was sealed".to_string()
        }
        ReplayFinding::EvidenceMissing { digest } => {
            format!("evidence {digest} is cited but not carried")
        }
        ReplayFinding::EvidenceTampered { recorded, found } => {
            format!("evidence {recorded} contains bytes that digest to {found}")
        }
        ReplayFinding::UnknownDeterministicVerifier { identity } => {
            format!("`{identity}` claims to be deterministic but this build does not know it")
        }
        ReplayFinding::FamilyDisagrees {
            identity,
            recorded,
            recomputed,
        } => format!(
            "`{identity}` recorded {recorded:?} but re-deciding it here gives {recomputed:?}"
        ),
        ReplayFinding::FamilyInputsMissing { identity } => {
            format!("`{identity}` cannot be re-decided: its pinned inputs are not in the bundle")
        }
        ReplayFinding::VerdictDiffers {
            recorded,
            recomputed,
        } => format!(
            "the certificate records {} but its contents support {}",
            recorded.as_str(),
            recomputed.as_str()
        ),
    }
}

fn describe_note(note: &ReplayNote) -> String {
    match note {
        ReplayNote::OracleNotReExecuted {
            identity,
            rationale,
        } => format!("`{identity}` was not re-run; its word was taken because {rationale}"),
        ReplayNote::KernelVersionDiffers {
            recorded,
            replaying,
        } => format!("recorded by `{recorded}`, replayed by `{replaying}`"),
        ReplayNote::FamilyReDecided { identity } => {
            format!("`{identity}` was re-decided here and agreed")
        }
    }
}
