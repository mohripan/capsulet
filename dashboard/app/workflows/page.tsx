 "use client";

import { GitBranch, Network, Route, Workflow } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";
import { workflows } from "../mock-data";

export default function WorkflowsPage() {
  return (
    <DashboardShell actionLabel="Workflow">
      <PageHeader
        eyebrow="Workflow lineage"
        title="Model multi-step automation"
        description="Inspect workflow definitions, step dependencies, pool routing, and lineage graph shape before dependency triggers are implemented."
      />

      <section className="contentGrid">
        <section className="panel span7">
          <PanelTitle icon={Workflow} title="Workflow Definitions" action="Import YAML" />
          <div className="workflowList">
            {workflows.map((workflow) => (
              <article className="workflowCard" key={workflow.name}>
                <div className="poolTop">
                  <div>
                    <h2>{workflow.name}</h2>
                    <p>{workflow.version} / {workflow.lastRun}</p>
                  </div>
                  <span>{workflow.success}% success</span>
                </div>
                <div className="stepRail">
                  {workflow.steps.map((step, index) => (
                    <div className="stepItem" key={step}>
                      <span>{index + 1}</span>
                      <strong>{step}</strong>
                    </div>
                  ))}
                </div>
                <div className="poolPill">
                  <Route size={15} aria-hidden="true" />
                  {workflow.pool}
                </div>
              </article>
            ))}
          </div>
        </section>

        <section className="panel span5">
          <PanelTitle icon={GitBranch} title="Lineage Graph" action="Open" />
          <div className="graphCanvas">
            <GraphNode label="prepare-data" x="8%" y="42%" />
            <GraphNode label="train" x="38%" y="18%" />
            <GraphNode label="evaluate" x="66%" y="42%" />
            <GraphNode label="publish" x="38%" y="68%" />
            <svg className="graphLines" viewBox="0 0 100 100" preserveAspectRatio="none">
              <path d="M22 48 L40 30 L66 46" />
              <path d="M22 48 L40 72 L66 48" />
            </svg>
          </div>
        </section>

        <section className="panel span12">
          <PanelTitle icon={Network} title="Dependency Trigger Preview" action="Configure" />
          <div className="wideNotice">
            <strong>Dependency triggers will attach to workflow lineage.</strong>
            <span>Downstream runs can be created from graph edges once workflow definitions and run history are stable.</span>
          </div>
        </section>
      </section>
    </DashboardShell>
  );
}

function GraphNode({ label, x, y }: { label: string; x: string; y: string }) {
  return (
    <div className="graphNode" style={{ left: x, top: y }}>
      {label}
    </div>
  );
}
