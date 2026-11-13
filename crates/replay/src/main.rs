//! `capsulet-replay` — check a certificate against the evidence it cites, or
//! explain a run from its bundle.
//!
//! Usage: `capsulet-replay <bundle.json> [--html <page.html>] [--packs <dir>]`
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
//! A run whose definition pins domain packs names them. Given `--packs <dir>`,
//! each pin is checked against `<dir>/<name>`: the pack there has to be exactly
//! the one pinned, by manifest digest, with its files intact. A pin the
//! replayer does not hold, or holds as another pack, is reported and fails the
//! replay, never substituted.
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

const USAGE: &str = "usage: capsulet-replay <bundle.json> [--html <page.html>] [--packs <dir>]";

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let Some(path) = arguments.next().map(PathBuf::from) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    // A run bundle can also be written out as one page to open in a browser, and
    // its pinned packs checked against the packs a replayer holds.
    let mut page = None;
    let mut packs = None;
    loop {
        match (arguments.next(), arguments.next()) {
            (None, _) => break,
            (Some(flag), Some(value)) if flag == "--html" && page.is_none() => {
                page = Some(PathBuf::from(value));
            }
            (Some(flag), Some(value)) if flag == "--packs" && packs.is_none() => {
                packs = Some(PathBuf::from(value));
            }
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }

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
                let run = report_run(&bundle, &explanation);
                let pins = report_pins(&bundle.definition, packs.as_deref());
                if run == ExitCode::SUCCESS { pins } else { run }
            }
            Err(error) => {
                eprintln!("{}: {error}", path.display());
                ExitCode::from(2)
            }
        };
    }
    if page.is_some() || packs.is_some() {
        eprintln!(
            "--html and --packs are for run bundles; {} is a certificate bundle",
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
        ReplayNote::ModelNotReExecuted {
            identity,
            family,
            independent_of_proposer,
        } => format!(
            "`{identity}` is a model ({family}, {}); it endorses or disputes and decided nothing \
             here, and it was not re-run",
            if *independent_of_proposer {
                "independent of the proposer"
            } else {
                "the proposer's own family"
            }
        ),
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
/// The packs a definition pins, and, given a directory of held packs, whether
/// each held one is exactly the pack pinned. Fails when any is not.
fn report_pins(definition: &capsulet_ir::Definition, held: Option<&std::path::Path>) -> ExitCode {
    let (lines, all_held) = pin_lines(definition, held);
    for line in lines {
        println!("{line}");
    }
    if all_held {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// A line for each pin, and whether every pin is held exactly as pinned. Pins
/// are only reported, and nothing fails, when no directory of packs is given.
fn pin_lines(
    definition: &capsulet_ir::Definition,
    held: Option<&std::path::Path>,
) -> (Vec<String>, bool) {
    let mut lines = Vec::new();
    let mut all_held = true;
    for pin in &definition.packs {
        let named = format!("pack {} {} ({})", pin.name, pin.version, pin.manifest);
        let Some(directory) = held else {
            lines.push(format!(
                "{named}: pinned; pass --packs <dir> to check the pack you hold"
            ));
            continue;
        };
        let found = capsulet_pack::read(&directory.join(&pin.name))
            .and_then(|pack| capsulet_pack::identify(&pack));
        match found {
            Ok(digest) if digest == pin.manifest.to_string() => {
                lines.push(format!("{named}: held, and it is the pack this run pinned"));
            }
            Ok(digest) => {
                all_held = false;
                lines.push(format!(
                    "{named}: held as {digest}, NOT the pack this run pinned"
                ));
            }
            Err(error) => {
                all_held = false;
                lines.push(format!("{named}: not held as pinned: {error}"));
            }
        }
    }
    (lines, all_held)
}

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

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// `dir/sample`, a pack whose manifest digest this returns.
    fn held_pack(dir: &Path, contract: &str) -> String {
        let root = dir.join("sample");
        std::fs::create_dir_all(root.join("contracts")).expect("dir");
        std::fs::write(root.join("contracts/sample.contract"), contract).expect("contract");
        std::fs::write(
            root.join("pack.json"),
            r#"{"schema_version": "capsulet.pack/v1", "name": "sample", "version": "1.0.0",
                "contracts": ["contracts/sample.contract"], "files": []}"#,
        )
        .expect("manifest");
        capsulet_pack::refresh(&root).expect("lists its files")
    }

    fn pinning(manifest: &str) -> capsulet_ir::Definition {
        let bundle: capsulet_runtime::bundle::SealedRunBundle =
            serde_json::from_str(include_str!("../tests/fixtures/run-succeeded.json"))
                .expect("a fixture");
        let mut definition = bundle.open().expect("opens").definition;
        definition.packs = vec![capsulet_ir::PackPin {
            name: "sample".to_string(),
            version: "1.0.0".to_string(),
            manifest: manifest.parse().expect("a digest"),
        }];
        definition
    }

    #[test]
    fn a_held_pack_is_checked_against_its_pin_and_never_substituted() {
        let directory =
            std::env::temp_dir().join(format!("capsulet-replay-pins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let manifest = held_pack(
            &directory,
            "entity Organization:\n  fields:\n    name: string\n",
        );

        let (lines, held) = super::pin_lines(&pinning(&manifest), Some(&directory));
        assert!(held, "{lines:?}");
        assert!(
            lines[0].ends_with("held, and it is the pack this run pinned"),
            "{lines:?}"
        );

        let other = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
        let (lines, held) = super::pin_lines(&pinning(other), Some(&directory));
        assert!(!held);
        assert!(
            lines[0].contains("NOT the pack this run pinned"),
            "{lines:?}"
        );

        std::fs::write(
            directory.join("sample/contracts/sample.contract"),
            "entity Changed:\n",
        )
        .expect("tamper");
        let (lines, held) = super::pin_lines(&pinning(&manifest), Some(&directory));
        assert!(!held);
        assert!(lines[0].contains("not held as pinned"), "{lines:?}");

        let (lines, held) = super::pin_lines(&pinning(&manifest), Some(&directory.join("nowhere")));
        assert!(!held, "{lines:?}");
        let (lines, held) = super::pin_lines(&pinning(&manifest), None);
        assert!(held, "only reported without a directory: {lines:?}");
        assert!(lines[0].contains("pass --packs <dir>"), "{lines:?}");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
