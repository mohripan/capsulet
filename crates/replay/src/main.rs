//! `capsulet-replay` — check a certificate against the evidence it cites, or
//! explain a run from its bundle.
//!
//! Usage: `capsulet-replay <bundle.json> [--html <page.html>]`
//!
//! A certificate bundle: exits 0 when the certificate still justifies its recorded
//! verdict, 1 when it does not, and 2 when the bundle cannot be read at all.
//!
//! A run bundle (`capsulet agent-runs bundle`, or `POST /v1/agent-runs/{id}/debug-bundle`):
//! checks its digest, folds its log again, makes every decision in it again from the
//! log before it, and says what the run did, why it ended, and which of its steps a
//! reader could reproduce without the installation. Exits 0 when the log supports
//! what the installation reported and every decision replays, 1 when either does
//! not, and 2 when the bundle cannot be read or does not match its digest.
//!
//! The distinction matters to a script: "this is wrong" and "I could not open the
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
use capsulet_runtime::bundle::{
    Explanation, RUN_BUNDLE_KIND, Rework, RunBundle, SealedRunBundle, explain,
};

mod html;

const USAGE: &str = "usage: capsulet-replay <bundle.json> [--html <page.html>]";

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let Some(path) = arguments.next().map(PathBuf::from) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    // A run bundle can also be written out as one page to open in a browser.
    let page = match (arguments.next(), arguments.next(), arguments.next()) {
        (None, ..) => None,
        (Some(flag), Some(page), None) if flag == "--html" => Some(PathBuf::from(page)),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("cannot read {}: {error}", path.display());
            return ExitCode::from(2);
        }
    };

    // A run bundle says so; anything else is read as a certificate bundle, as before.
    if let Ok(sealed) = serde_json::from_slice::<SealedRunBundle>(&bytes)
        && sealed.kind == RUN_BUNDLE_KIND
    {
        return match sealed.open() {
            Ok(bundle) => {
                let explanation = explain(&bundle);
                if let Some(page) = &page
                    && let Err(error) = std::fs::write(page, html::render(&bundle, &explanation))
                {
                    eprintln!("cannot write {}: {error}", page.display());
                    return ExitCode::from(2);
                }
                report_run(&bundle, &explanation)
            }
            Err(error) => {
                eprintln!("{}: {error}", path.display());
                ExitCode::from(2)
            }
        };
    }
    if page.is_some() {
        eprintln!(
            "--html is for run bundles; {} is a certificate bundle",
            path.display()
        );
        return ExitCode::from(2);
    }

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
        ReplayFinding::FamilyInputsMalformed { identity, detail } => format!(
            "`{identity}` cannot be re-decided: its pinned inputs are in the bundle and cannot be              read — {detail}"
        ),
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
        ReplayNote::FamilyRulesDiffer {
            identity,
            recorded,
            replaying,
        } => format!(
            "`{identity}` was decided under rules {recorded} and re-decided here under rules \
             {replaying}; a disagreement may be the rules rather than the bundle"
        ),
        ReplayNote::NothingWasReChecked => {
            "this certificate pins no evidence and names no verifier, so nothing was re-checked"
                .to_string()
        }
    }
}

/// Says what a run did, from its bundle alone.
fn report_run(bundle: &RunBundle, explanation: &Explanation) -> ExitCode {
    println!(
        "run {} of agent {}, in {}/{}",
        bundle.run_id, bundle.agent_id, bundle.tenant_id, bundle.project_id
    );
    println!(
        "trace: {}",
        bundle
            .trace_id
            .as_deref()
            .unwrap_or("none; the run was not traced")
    );
    match &explanation.status {
        Ok(status) => println!(
            "log: {} events, folding to `{}`",
            bundle.events.len(),
            status.as_str()
        ),
        Err(reason) => println!(
            "log: {} events that do not fold: {reason}",
            bundle.events.len()
        ),
    }
    println!(
        "definition: {}",
        if explanation.definition_matches {
            "the version the run was admitted with"
        } else {
            "NOT the version the run was admitted with"
        }
    );
    let spent = &explanation.spent;
    println!(
        "spent: {} tokens, {} cost micro-units, {} effects, {} unmeasured",
        spent.tokens, spent.cost_micro_units, spent.effects, spent.unmeasured
    );
    println!(
        "result: {}{}",
        explanation.result.as_str(),
        match explanation.result {
            capsulet_runtime::bundle::RunResult::Unverified => {
                ", completed in observe mode: its outputs were captured and nothing certified them"
            }
            capsulet_runtime::bundle::RunResult::SeeCertificate => {
                ", completed under a mode that certifies: its certificate says what it amounts to"
            }
            capsulet_runtime::bundle::RunResult::Abstained => ", ended without a result",
            capsulet_runtime::bundle::RunResult::Pending => ", has not ended",
        }
    );
    if let Some(reason) = &explanation.ended_because {
        println!("ended because: {reason}");
    }
    if let Some(wait) = &explanation.waiting_on {
        println!("waiting on: {wait}");
    }
    if !explanation.unfinished.is_empty() {
        let doing: Vec<&str> = explanation
            .unfinished
            .iter()
            .map(capsulet_ir::Identifier::as_str)
            .collect();
        println!("interrupted while running: {}", doing.join(", "));
    }

    print_steps(explanation);
    let decisions = &explanation.decisions;
    println!(
        "decisions: {} made again from the log before them, {} written by the outside \
         world, {} written by a decision's own work",
        decisions.replayed, decisions.external, decisions.consequences
    );
    print_offline(bundle, explanation);

    if explanation.agrees() {
        println!(
            "agrees: the log supports what was reported, and is one the runtime would \
             have written"
        );
        ExitCode::SUCCESS
    } else {
        println!("disagrees:");
        for disagreement in &explanation.disagreements {
            println!("  {disagreement}");
        }
        for divergence in &decisions.divergences {
            println!(
                "  event {} records {}, but deciding again from the log before it gives: {}",
                divergence.position, divergence.recorded, divergence.decided
            );
        }
        ExitCode::FAILURE
    }
}

/// Each node's attempts, and each effect's.
fn print_steps(explanation: &Explanation) {
    println!("nodes:");
    for node in &explanation.nodes {
        let failures = if node.failures.is_empty() {
            String::new()
        } else {
            format!(", failed with {}", node.failures.join(", "))
        };
        let rules = if node.rules.is_empty() {
            String::new()
        } else {
            let stated: Vec<String> = node
                .rules
                .iter()
                .map(|(rule, value)| format!("{rule}={value}"))
                .collect();
            format!(" ({})", stated.join(", "))
        };
        println!(
            "  {} ({}): {} started, {} finished{failures}; {}; {}{rules}",
            node.node,
            node.kind,
            node.attempts,
            node.finished,
            node.rework.as_str(),
            node.result.as_str()
        );
    }
    if !explanation.effects.is_empty() {
        println!("effects:");
        for effect in &explanation.effects {
            println!(
                "  {} on {}: {} attempts, {}",
                effect.effect, effect.node, effect.attempts, effect.outcome
            );
        }
    }
}

/// What a reader without the installation can and cannot do again.
fn print_offline(bundle: &RunBundle, explanation: &Explanation) {
    println!("offline:");
    println!(
        "  reproduced here: the fold of every event, the status, the spend, and every \
         decision"
    );
    let needing = |rework: Rework| -> Vec<&str> {
        explanation
            .nodes
            .iter()
            .filter(|node| node.rework == rework && node.attempts > 0)
            .map(|node| node.node.as_str())
            .collect()
    };
    for (rework, why) in [
        (
            Rework::Model,
            "a model, which can answer differently each time",
        ),
        (Rework::Memory, "the project's memory as it stood then"),
        (Rework::Effect, "the world it acted on"),
        (
            Rework::FromInputs,
            "only their input values, which a bundle never carries",
        ),
    ] {
        let nodes = needing(rework);
        if !nodes.is_empty() {
            println!("  not reproduced, needing {why}: {}", nodes.join(", "));
        }
    }
    println!(
        "  left out of this bundle: {}",
        if bundle.excluded.is_empty() {
            "nothing".to_string()
        } else {
            bundle.excluded.join("; ")
        }
    );
    if bundle.payloads_erased {
        println!("  this run's payloads had been erased when it was packed");
    }
}
