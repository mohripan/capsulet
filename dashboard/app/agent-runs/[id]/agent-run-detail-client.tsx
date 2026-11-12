"use client";

// capsulet-claims: CAP-AGENT-001, CAP-AGENT-002, CAP-AGENT-004, CAP-AGENT-010, CAP-DASHBOARD-001

import Link from "next/link";
import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, Ban, Bot, Check, History, ListChecks, RefreshCw, X } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle, ResizableGridTable, StateBadge } from "../../components";
import {
  AgentRun,
  AgentRunEvent,
  AgentRunExplanation,
  cancelAgentRun,
  decideAgentRun,
  describeAgentRunEvent,
  explainAgentRun,
  getAgentRun,
  getAgentRunEvents,
  getErrorMessage,
  isFinishedAgentRun,
  resultTone,
  roleLabel,
  tokensLabel
} from "../../lib/api";

function ResultBadge({ result }: { result: string }) {
  return <span className={`state state-${resultTone(result)}`}>{result.replace(/_/g, " ")}</span>;
}

const eventColumns = [
  { label: "#", width: 70, minWidth: 50 },
  { label: "Event", width: 220, minWidth: 140 },
  { label: "Detail", width: 480, minWidth: 200 },
  { label: "Recorded", width: 210, minWidth: 140 }
];

export default function AgentRunDetailClient({ id }: { id: string }) {
  const [run, setRun] = useState<AgentRun | null>(null);
  const [events, setEvents] = useState<AgentRunEvent[]>([]);
  const [explanation, setExplanation] = useState<AgentRunExplanation | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isActing, setIsActing] = useState(false);

  const refresh = useCallback(async function refresh() {
    setError(null);
    try {
      const [nextRun, log] = await Promise.all([getAgentRun(id), getAgentRunEvents(id)]);
      setRun(nextRun);
      setEvents(log.events);
      // A run from before agent runs executed has no log to explain.
      setExplanation(nextRun.execution ? await explainAgentRun(id) : null);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsLoading(false);
    }
  }, [id]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // A run that is still going is worth watching. One that has finished will not change.
  const isFinished = run ? isFinishedAgentRun(run.status) : true;
  useEffect(() => {
    if (isFinished) return;
    const timer = setInterval(() => void refresh(), 2000);
    return () => clearInterval(timer);
  }, [isFinished, refresh]);

  async function act(action: () => Promise<unknown>) {
    setIsActing(true);
    setError(null);
    try {
      await action();
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsActing(false);
    }
  }

  return (
    <DashboardShell>
      <PageHeader eyebrow="Agent run" title={id} description="Everything here is read from the run's event log." />

      <section className="contentGrid">
        <section className="panel span12">
          <div className="detailToolbar">
            <Link className="secondaryButton" href="/agent-runs">
              <ArrowLeft size={16} aria-hidden="true" />
              Agent runs
            </Link>
            <button className="secondaryButton" onClick={refresh} disabled={isLoading}>
              <RefreshCw size={16} aria-hidden="true" />
              Refresh
            </button>
            {run?.status === "waiting" ? (
              <>
                <button className="secondaryButton" onClick={() => void act(() => decideAgentRun(id, true))} disabled={isActing}>
                  <Check size={16} aria-hidden="true" />
                  Approve
                </button>
                <button className="dangerButton" onClick={() => void act(() => decideAgentRun(id, false))} disabled={isActing}>
                  <X size={16} aria-hidden="true" />
                  Refuse
                </button>
              </>
            ) : null}
            {run && !isFinished ? (
              <button className="dangerButton" onClick={() => void act(() => cancelAgentRun(id))} disabled={isActing}>
                <Ban size={16} aria-hidden="true" />
                {isActing ? "Sending" : "Cancel run"}
              </button>
            ) : null}
          </div>
          {error ? <div className="errorBox">{error}</div> : null}
          {isLoading && !run ? <div className="emptyState">Loading agent run.</div> : null}
        </section>

        {run ? (
          <section className="panel span5">
            <PanelTitle icon={Bot} title="Run" action="API" />
            <div className="detailGrid">
              <span>Agent</span>
              <strong className="mono">{run.agent_id}</strong>
              <span>Status</span>
              <StateBadge state={run.status} />
              <span>Assurance</span>
              <strong>{run.assurance}</strong>
              <span>Nodes finished</span>
              <strong>{run.state_version}</strong>
              <span>Tokens</span>
              <strong>{tokensLabel(run)}</strong>
              {run.execution?.detail ? (
                <>
                  <span>Why it ended</span>
                  <strong>{run.execution.detail}</strong>
                </>
              ) : null}
              {run.execution?.waiting_on ? (
                <>
                  <span>Waiting on</span>
                  <strong>{run.execution.waiting_on}</strong>
                </>
              ) : null}
              {run.execution?.payloads_erased_at ? (
                <>
                  <span>Payloads erased</span>
                  <strong>{run.execution.payloads_erased_at}</strong>
                </>
              ) : null}
              <span>Definition</span>
              <strong className="mono">{run.execution?.definition_digest ?? "None: this run predates agent execution."}</strong>
            </div>
          </section>
        ) : null}

        {run ? (
          <section className="panel span7">
            <PanelTitle icon={Bot} title="State" action="Latest" />
            {run.execution?.payloads_erased_at ? (
              <div className="emptyState">Erased on request. The event log below still records every step, by digest.</div>
            ) : (
              <pre className="terminal logBlock">{JSON.stringify(run.state, null, 2)}</pre>
            )}
          </section>
        ) : null}

        {explanation ? (
          <section className="panel span12">
            <PanelTitle
              icon={ListChecks}
              title="What the run amounts to"
              action={explanation.agrees ? "Every decision replays" : "Disagrees with its log"}
            />
            <div className="detailGrid">
              <span>Result</span>
              <ResultBadge result={explanation.result} />
              <span>Assurance</span>
              <strong>{explanation.assurance_mode ?? "not recorded"}</strong>
              <span>Decisions</span>
              <strong>
                {`${explanation.decisions.replayed} made again from the log before them, ${explanation.decisions.external} from outside, ${explanation.decisions.consequences} by a decision's own work`}
              </strong>
              {explanation.trace_id ? (
                <>
                  <span>Trace</span>
                  <strong className="mono">{explanation.trace_id}</strong>
                </>
              ) : null}
            </div>
            {explanation.disagreements.length > 0 || explanation.decisions.divergences.length > 0 ? (
              <ul className="explanationProblems">
                {explanation.disagreements.map((disagreement) => (
                  <li key={disagreement}>{disagreement}</li>
                ))}
                {explanation.decisions.divergences.map((divergence) => (
                  <li key={divergence.position}>
                    {`Event ${divergence.position} records ${divergence.recorded}; deciding again gives: ${divergence.decided}`}
                  </li>
                ))}
              </ul>
            ) : null}
            <p className="explanationNote">
              Each label is what the run&apos;s log records, read the way an offline replay of its bundle reads it. None
              certifies a result: a check that held proves only that its rules held on its input.
            </p>
            <div className="explanationNodes">
              {explanation.nodes
                .filter((node) => node.result !== "structural")
                .map((node) => (
                  <details className="explanationNode" key={node.node}>
                    <summary>
                      <span className="mono">{node.node}</span> <span className="explanationKind">{node.kind}</span>{" "}
                      <ResultBadge result={node.result} />{" "}
                      <span className="explanationKind">{`${node.attempts} started, ${node.finished} finished`}</span>
                    </summary>
                    <p className="explanationNote">{`${node.meaning}.`}</p>
                    {node.rules.length > 0 ? (
                      <p className="explanationNote">
                        Rules: <span className="mono">{node.rules.map((rule) => `${rule.rule} = ${rule.value}`).join("; ")}</span>
                      </p>
                    ) : null}
                    <ol className="explanationEvents">
                      {node.events.map((event) => (
                        <li key={event.position}>
                          <span className="mono">{`#${event.position}`}</span>{" "}
                          <span className={`explanationRole role-${event.role}`}>{roleLabel(event.role)}</span>{" "}
                          <span className="mono">{event.kind}</span>
                          {event.evidence.map((evidence) => (
                            <div className="explanationEvidence" key={`${event.position}-${evidence.name}`}>
                              <span>{evidence.name}</span> <span className="mono">{evidence.digest}</span>
                            </div>
                          ))}
                        </li>
                      ))}
                    </ol>
                  </details>
                ))}
            </div>
          </section>
        ) : null}

        <section className="panel span12">
          <PanelTitle icon={History} title="Event log" action={`${events.length} events`} />
          <ResizableGridTable columns={eventColumns}>
            {!isLoading && events.length === 0 ? <div className="emptyState">No events.</div> : null}
            {events.map((event) => (
              <div className="resizableRow runRow" key={event.position}>
                <span className="tableCell">{event.position}</span>
                <span className="mono tableCell">{event.kind}</span>
                <span className="tableCell" title={describeAgentRunEvent(event)}>
                  {describeAgentRunEvent(event)}
                </span>
                <span className="tableCell">{new Date(event.recorded_at).toLocaleString()}</span>
              </div>
            ))}
          </ResizableGridTable>
        </section>
      </section>
    </DashboardShell>
  );
}
