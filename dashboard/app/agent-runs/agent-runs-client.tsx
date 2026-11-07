"use client";

// capsulet-claims: CAP-AGENT-001, CAP-AGENT-002, CAP-DASHBOARD-001

import Link from "next/link";
import { useCallback, useEffect, useState } from "react";
import { Bot, RefreshCw } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle, ResizableGridTable, StateBadge } from "../components";
import { AgentRun, getErrorMessage, listAgentRuns, tokensLabel } from "../lib/api";

const columns = [
  { label: "Run", width: 320, minWidth: 160 },
  { label: "Agent", width: 260, minWidth: 140 },
  { label: "Status", width: 140, minWidth: 110 },
  { label: "Nodes finished", width: 140, minWidth: 100 },
  { label: "Tokens", width: 130, minWidth: 90 }
];

export default function AgentRunsClient() {
  const [runs, setRuns] = useState<AgentRun[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async function refresh() {
    setIsLoading(true);
    setError(null);
    try {
      const response = await listAgentRuns();
      setRuns(response.agent_runs);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <DashboardShell>
      <PageHeader
        eyebrow="Agent platform"
        title="Agent runs"
        description="Each run is executed by the graph worker. Its status says where it is, not whether its result is right."
      />

      <section className="contentGrid">
        <section className="panel span12">
          <PanelTitle icon={Bot} title="Runs" action="Live API" />
          <div className="panelActions">
            <button className="secondaryButton" onClick={refresh} disabled={isLoading}>
              <RefreshCw size={16} aria-hidden="true" />
              {isLoading ? "Refreshing" : "Refresh"}
            </button>
          </div>
          {error ? <div className="errorBox">{error}</div> : null}
          <ResizableGridTable columns={columns}>
            {!isLoading && runs.length === 0 ? (
              <div className="emptyState">No agent runs in this project yet.</div>
            ) : null}
            {runs.map((run) => (
              <Link className="resizableRow runRow interactiveRow" href={`/agent-runs/${encodeURIComponent(run.id)}`} key={run.id}>
                <span className="mono tableCell" title={run.id}>
                  {run.id}
                </span>
                <span className="mono tableCell" title={run.agent_id}>
                  {run.agent_id}
                </span>
                <StateBadge state={run.status} />
                <span className="tableCell">{run.state_version}</span>
                <span className="tableCell">{tokensLabel(run)}</span>
              </Link>
            ))}
          </ResizableGridTable>
        </section>
      </section>
    </DashboardShell>
  );
}
