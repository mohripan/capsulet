//! A run bundle as one page somebody can open in a browser, with no server, no
//! script and no network: `capsulet-replay run.json --html run.html`.
//!
//! It walks the same path the text report does, and lets a reader open each
//! step: the run, then each node, then what the node was observed to do (its
//! events, each marked with what the replay made of it), then the evidence
//! those events name by digest, then what the run's verification amounted to.
//! Everything on it comes from the bundle and from folding and re-deciding its
//! log; nothing is fetched and nothing is inferred beyond that.

use std::fmt::Write as _;

use capsulet_ir::definition::AssuranceMode;
use capsulet_ir::id::Identifier;
use capsulet_runtime::bundle::{Explanation, RunBundle, evidence_of, node_of};
use capsulet_runtime::replay::Role;
use capsulet_runtime::{RecordedEvent, RunEvent};

/// The page for `bundle`, given what [`capsulet_runtime::bundle::explain`] said
/// about it.
#[must_use]
pub fn render(bundle: &RunBundle, explanation: &Explanation) -> String {
    let mut page = String::new();
    let title = format!("Run {}", bundle.run_id);
    let _ = write!(
        page,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<main>\n",
        escape(&title)
    );
    summary(&mut page, bundle, explanation);
    verification(&mut page, bundle, explanation);
    nodes(&mut page, bundle, explanation);
    timeline(&mut page, bundle, explanation);
    offline(&mut page, bundle);
    page.push_str("</main>\n</body>\n</html>\n");
    page
}

fn summary(page: &mut String, bundle: &RunBundle, explanation: &Explanation) {
    let status = explanation.status.as_ref().map_or_else(
        |error| format!("does not fold: {error}"),
        |status| status.as_str().to_string(),
    );
    let verdict = if explanation.agrees() {
        "<span class=\"ok\">agrees</span>: the log supports what was reported, and every decision in \
         it is one the runtime makes again"
            .to_string()
    } else {
        "<span class=\"bad\">disagrees</span>: see below".to_string()
    };
    let decisions = &explanation.decisions;
    let spent = &explanation.spent;
    let _ = write!(
        page,
        "<h1>{}</h1>\n<p class=\"lede\">{verdict}</p>\n<dl class=\"facts\">\n",
        escape(&format!("Run {}", bundle.run_id))
    );
    for (term, value) in [
        ("Agent", bundle.agent_id.clone()),
        (
            "Project",
            format!("{}/{}", bundle.tenant_id, bundle.project_id),
        ),
        ("Status the log folds to", status),
        ("Status reported", bundle.reported.status.clone()),
        (
            "Trace",
            bundle
                .trace_id
                .clone()
                .unwrap_or_else(|| "none; the run was not traced".to_string()),
        ),
        (
            "Spent",
            format!(
                "{} tokens, {} cost micro-units, {} effects, {} unmeasured",
                spent.tokens, spent.cost_micro_units, spent.effects, spent.unmeasured
            ),
        ),
        (
            "Decisions",
            format!(
                "{} made again from the log before them, {} written by the outside world, {} \
                 written by a decision's own work",
                decisions.replayed, decisions.external, decisions.consequences
            ),
        ),
    ] {
        let _ = writeln!(page, "<dt>{}</dt><dd>{}</dd>", escape(term), escape(&value));
    }
    if let Some(reason) = &explanation.ended_because {
        let _ = writeln!(page, "<dt>Ended because</dt><dd>{}</dd>", escape(reason));
    }
    if let Some(wait) = &explanation.waiting_on {
        let _ = writeln!(page, "<dt>Waiting on</dt><dd>{}</dd>", escape(wait));
    }
    if !explanation.unfinished.is_empty() {
        let _ = writeln!(
            page,
            "<dt>Interrupted while running</dt><dd>{}</dd>",
            escape(&join(&explanation.unfinished))
        );
    }
    page.push_str("</dl>\n");
    if !explanation.disagreements.is_empty() || !decisions.divergences.is_empty() {
        page.push_str("<h2>Disagreements</h2>\n<ul class=\"problems\">\n");
        for disagreement in &explanation.disagreements {
            let _ = writeln!(page, "<li>{}</li>", escape(disagreement));
        }
        for divergence in &decisions.divergences {
            let _ = writeln!(
                page,
                "<li><a href=\"#event-{0}\">Event {0}</a> records {1}, but deciding again from the \
                 log before it gives: {2}</li>",
                divergence.position,
                escape(&divergence.recorded),
                escape(&divergence.decided)
            );
        }
        page.push_str("</ul>\n");
    }
}

/// What the run's verification amounted to, from the mode it was admitted in.
fn verification(page: &mut String, bundle: &RunBundle, explanation: &Explanation) {
    let mode = bundle
        .events
        .iter()
        .find_map(|recorded| match &recorded.event {
            RunEvent::Admitted { mode, .. } => Some(*mode),
            _ => None,
        });
    let said = match mode {
        Some(AssuranceMode::Observe) => {
            "Admitted in observe mode: the run captured its evidence, no obligation was evaluated, \
             and its outputs are unverified. No certificate was issued."
        }
        Some(AssuranceMode::Verify) => {
            "Admitted in verify mode: its obligations were evaluated and a verdict reported. This \
             bundle does not carry the certificate."
        }
        Some(AssuranceMode::Enforce) => {
            "Admitted in enforce mode: its obligations were evaluated and a protected boundary \
             refused to cross without them. This bundle does not carry the certificate."
        }
        None => "The log records no admission, so nothing says how the run was to be verified.",
    };
    let _ = write!(
        page,
        "<h2>Verification</h2>\n<p>{}</p>\n<p>Result: <span class=\"result {}\">{}</span>.</p>\n\
         <p>Definition: {}.</p>\n",
        escape(said),
        explanation.result.as_str(),
        explanation.result.as_str().replace('_', " "),
        if explanation.definition_matches {
            "the version the run was admitted with"
        } else {
            "<strong>not</strong> the version the run was admitted with"
        }
    );
}

/// Each node, then what it was observed to do, then the evidence that names.
fn nodes(page: &mut String, bundle: &RunBundle, explanation: &Explanation) {
    page.push_str("<h2>Nodes</h2>\n");
    for node in &explanation.nodes {
        let failures = if node.failures.is_empty() {
            String::new()
        } else {
            format!(", failed with {}", node.failures.join(", "))
        };
        let _ = writeln!(
            page,
            "<details class=\"node\"><summary><code>{}</code> <span class=\"kind\">{}</span> \
             <span class=\"result {}\" title=\"{}\">{}</span> {} started, {} finished{}; \
             {}</summary>",
            escape(node.node.as_str()),
            escape(&node.kind),
            node.result.as_str(),
            escape(node.result.meaning()),
            node.result.as_str().replace('_', " "),
            node.attempts,
            node.finished,
            escape(&failures),
            escape(node.rework.as_str())
        );
        let _ = writeln!(
            page,
            "<p class=\"meaning\">{}.</p>",
            escape(node.result.meaning())
        );
        if !node.rules.is_empty() {
            let stated: Vec<String> = node
                .rules
                .iter()
                .map(|(rule, value)| format!("{rule} = {value}"))
                .collect();
            let _ = writeln!(
                page,
                "<p class=\"rules\">Rules: <code>{}</code>. A check proves these rules held \
                 on its input, and nothing more.</p>",
                escape(&stated.join("; "))
            );
        }
        page.push_str("<ol class=\"events\">\n");
        for (recorded, role) in bundle
            .events
            .iter()
            .zip(&explanation.decisions.roles)
            .filter(|(recorded, _)| concerns(&recorded.event, &node.node))
        {
            event_item(page, recorded, *role, Place::Node);
        }
        page.push_str("</ol>\n</details>\n");
    }
    if !explanation.effects.is_empty() {
        page.push_str("<h2>Effects</h2>\n<ul>\n");
        for effect in &explanation.effects {
            let _ = writeln!(
                page,
                "<li><code>{}</code> on <code>{}</code>: {} attempts, {}</li>",
                escape(effect.effect.as_str()),
                escape(effect.node.as_str()),
                effect.attempts,
                escape(effect.outcome)
            );
        }
        page.push_str("</ul>\n");
    }
}

/// Every event in order, with what the replay made of it.
fn timeline(page: &mut String, bundle: &RunBundle, explanation: &Explanation) {
    page.push_str(
        "<h2>Log</h2>\n<p class=\"legend\"><span class=\"role decision\">decision</span> made again \
         from the log before it · <span class=\"role external\">outside</span> written by the \
         outside world · <span class=\"role consequence\">work</span> written by a decision's own \
         work · <span class=\"role divergent\">diverges</span> the runtime would not have written \
         it</p>\n<ol class=\"events\">\n",
    );
    for (recorded, role) in bundle.events.iter().zip(&explanation.decisions.roles) {
        event_item(page, recorded, *role, Place::Log);
    }
    page.push_str("</ol>\n");
}

fn offline(page: &mut String, bundle: &RunBundle) {
    page.push_str(
        "<h2>Offline</h2>\n<p>Reproduced here: the fold of every event, the status, the spend, and \
         every decision. Not reproduced: a node's own work, which needs a model, the project's \
         memory, the world an effect acted on, or input values a bundle never carries.</p>\n",
    );
    let _ = writeln!(
        page,
        "<p>Left out of this bundle: {}.</p>",
        escape(&if bundle.excluded.is_empty() {
            "nothing".to_string()
        } else {
            bundle.excluded.join("; ")
        })
    );
    if bundle.payloads_erased {
        page.push_str("<p>This run's payloads had been erased when it was packed.</p>\n");
    }
}

/// Where an event is shown: in the log, which is the one place each event is
/// anchored, or under its node, which links to that place.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Log,
    Node,
}

fn event_item(page: &mut String, recorded: &RecordedEvent, role: Role, place: Place) {
    let (class, label) = match role {
        Role::Decision => ("decision", "decision"),
        Role::External => ("external", "outside"),
        Role::Consequence => ("consequence", "work"),
        Role::Divergent => ("divergent", "diverges"),
    };
    let position = recorded.position;
    let opening = match place {
        Place::Log => {
            format!("<li id=\"event-{position}\"><span class=\"position\">{position}</span>")
        }
        Place::Node => {
            format!("<li><a class=\"position\" href=\"#event-{position}\">{position}</a>")
        }
    };
    let _ = write!(
        page,
        "{opening} <span class=\"role {class}\">{label}</span> <code>{kind}</code>",
        kind = recorded.event.as_str()
    );
    let detail = detail(&recorded.event);
    if !detail.is_empty() {
        let _ = write!(page, " {}", escape(&detail));
    }
    for (name, digest) in evidence(&recorded.event) {
        let _ = write!(
            page,
            "<div class=\"evidence\"><code>{}</code> <code class=\"digest\">{}</code></div>",
            escape(&name),
            escape(&digest)
        );
    }
    page.push_str("</li>\n");
}

/// Whether `event` is about `node`.
fn concerns(event: &RunEvent, node: &Identifier) -> bool {
    node_of(event) == Some(node)
}

/// A short account of what the event says, beyond its kind.
fn detail(event: &RunEvent) -> String {
    match event {
        RunEvent::NodeStarted { node } => format!("`{node}`"),
        RunEvent::NodeFinished { node, .. } => format!("`{node}` finished"),
        RunEvent::NodeFailed { node, failure, .. } => {
            format!("`{node}` failed: {}", failure.as_str())
        }
        RunEvent::ResourcesConsumed { node, usage } => {
            let mut parts = Vec::new();
            if let Some(model) = &usage.model {
                parts.push(format!("model {model}"));
            }
            if let Some(tokens) = usage.input_tokens {
                parts.push(format!("{tokens} tokens in"));
            }
            if let Some(tokens) = usage.output_tokens {
                parts.push(format!("{tokens} tokens out"));
            }
            if let Some(latency) = usage.latency_ms {
                parts.push(format!("{latency}ms"));
            }
            format!("`{node}`: {}", parts.join(", "))
        }
        RunEvent::EffectClaimed {
            node,
            effect,
            attempt,
            ..
        }
        | RunEvent::EffectFinalized {
            node,
            effect,
            attempt,
            ..
        }
        | RunEvent::EffectAbandoned {
            node,
            effect,
            attempt,
            ..
        }
        | RunEvent::EffectUncertain {
            node,
            effect,
            attempt,
        } => format!("`{node}`/`{effect}` attempt {attempt}"),
        RunEvent::IterationStarted { region, index, .. } => format!("`{region}` #{index}"),
        RunEvent::IterationFinished { region, record } => format!("`{region}` #{}", record.index),
        RunEvent::LoopStopped { region, reason } => format!("`{region}`: {}", reason.as_str()),
        RunEvent::Suspended { wait } | RunEvent::Resumed { wait, .. } => wait.describe(),
        RunEvent::Failed { reason } => reason.describe(),
        RunEvent::Cancelled { by } | RunEvent::CancellationRequested { by } => {
            format!("by `{by}`")
        }
        _ => String::new(),
    }
}

/// The evidence an event names by digest, as text for the page.
fn evidence(event: &RunEvent) -> Vec<(String, String)> {
    evidence_of(event)
        .into_iter()
        .map(|(name, digest)| (name, digest.to_string()))
        .collect()
}

fn join(identifiers: &[Identifier]) -> String {
    identifiers
        .iter()
        .map(Identifier::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Text made safe for an HTML element or attribute.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

const STYLE: &str = "\
:root{--bg:#fff;--fg:#1b1f24;--muted:#5b6470;--line:#d8dee4;--ok:#1a7f37;--bad:#cf222e;\
--decision:#0969da;--external:#8250df;--work:#6e7781;--code:#f3f5f7}\
@media (prefers-color-scheme:dark){:root{--bg:#0d1117;--fg:#e6edf3;--muted:#9198a1;\
--line:#30363d;--ok:#3fb950;--bad:#f85149;--decision:#58a6ff;--external:#bc8cff;\
--work:#9198a1;--code:#161b22}}\
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,sans-serif}\
main{max-width:60rem;margin:0 auto;padding:1.5rem 1rem 3rem}\
h1{font-size:1.5rem;margin:0 0 .25rem}h2{font-size:1.15rem;margin:2rem 0 .5rem;\
border-bottom:1px solid var(--line);padding-bottom:.25rem}\
code{font:13px ui-monospace,monospace;background:var(--code);padding:0 .25rem;border-radius:3px}\
.lede{margin:.25rem 0 1rem}.ok{color:var(--ok);font-weight:600}.bad{color:var(--bad);font-weight:600}\
.facts{display:grid;grid-template-columns:max-content 1fr;gap:.25rem 1rem}\
.facts dt{color:var(--muted)}.facts dd{margin:0;overflow-wrap:anywhere}\
.problems li{color:var(--bad)}details.node{border:1px solid var(--line);border-radius:6px;\
margin:.5rem 0;padding:.25rem .75rem}details.node summary{cursor:pointer}\
.kind{color:var(--muted)}ol.events{list-style:none;padding:0;margin:.5rem 0}\
ol.events li{padding:.25rem 0;border-top:1px solid var(--line);overflow-wrap:anywhere}\
.position{display:inline-block;min-width:2.5rem;color:var(--muted);text-align:right}\
.role{display:inline-block;min-width:4.5rem;text-align:center;font-size:12px;border-radius:10px;\
border:1px solid currentColor;margin:0 .25rem}.role.decision{color:var(--decision)}\
.role.external{color:var(--external)}.role.consequence{color:var(--work)}\
.role.divergent{color:var(--bad);font-weight:600}\
.evidence{margin-left:7.75rem;color:var(--muted)}.digest{font-size:12px}\
.legend{color:var(--muted);font-size:13px}\
.result{display:inline-block;font-size:12px;border-radius:10px;padding:0 .5rem;\
background:var(--code);margin:0 .25rem}.result.checked{color:var(--ok)}\
.result.disputed,.result.failed,.result.abstained{color:var(--bad)}\
.result.proposed,.result.waived{color:var(--external)}\
.meaning,.rules{color:var(--muted);font-size:13px;margin:.25rem 0}";
