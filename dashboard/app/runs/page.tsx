 "use client";

import { Activity, Archive, ListFilter, TerminalSquare } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle, StateBadge } from "../components";
import { runs, timeline } from "../mock-data";

export default function RunsPage() {
  return (
    <DashboardShell actionLabel="Submit job">
      <PageHeader
        eyebrow="Run queue"
        title="Watch job attempts and state transitions"
        description="Track queued, running, succeeded, failed, and timed-out runs with attempts, nodes, logs, and artifacts."
      />

      <section className="contentGrid">
        <section className="panel span8">
          <PanelTitle icon={Activity} title="Runs" action="Refresh" />
          <div className="runTable">
            <div className="runHeader runHeaderWide">
              <span>Run</span>
              <span>Automation</span>
              <span>Pool</span>
              <span>State</span>
              <span>Attempt</span>
              <span>Duration</span>
              <span>Node</span>
            </div>
            {runs.map((run) => (
              <div className="runRow runRowWide" key={run.id}>
                <span className="mono">{run.id}</span>
                <span>{run.automation}</span>
                <span>{run.pool}</span>
                <StateBadge state={run.state} />
                <span>{run.attempt}</span>
                <span>{run.duration}</span>
                <span>{run.node}</span>
              </div>
            ))}
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={ListFilter} title="Queue Filters" action="Reset" />
          <div className="tileGrid">
            {["queued", "running", "failed", "timed_out", "succeeded", "cancelled"].map((state) => (
              <div className="miniTile" key={state}>
                <strong>{state}</strong>
                <span>filter</span>
              </div>
            ))}
          </div>
        </section>

        <section className="panel span6">
          <PanelTitle icon={TerminalSquare} title="Selected Run Logs" action="Tail" />
          <div className="terminal tall">
            {timeline.concat(timeline).map(([time, event, detail], index) => (
              <div className="logLine" key={`${time}-${event}-${index}`}>
                <span>{time}</span>
                <code>{event}</code>
                <em>{detail}</em>
              </div>
            ))}
          </div>
        </section>

        <section className="panel span6">
          <PanelTitle icon={Archive} title="Attempt Artifacts" action="Open bucket" />
          <div className="artifactDrop">
            <strong>run_9ac41</strong>
            <span>input-bundle.tar.gz</span>
            <span>resized-images.zip</span>
            <span>stdout.log</span>
          </div>
        </section>
      </section>
    </DashboardShell>
  );
}
