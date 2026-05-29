 "use client";

import { Cpu, Network, Route, Server } from "lucide-react";
import { DashboardShell, LoadBar, PageHeader, PanelTitle } from "../components";
import { pools } from "../mock-data";

export default function ExecutionPoolsPage() {
  return (
    <DashboardShell actionLabel="Pool">
      <PageHeader
        eyebrow="Compute routing"
        title="Manage execution pools"
        description="Map Capsulet jobs onto Kubernetes node classes with selectors, tolerations, resources, timeouts, and concurrency limits."
      />

      <section className="contentGrid">
        {pools.map((pool) => (
          <section className="panel span4" key={pool.name}>
            <PanelTitle icon={Route} title={pool.name} action="Edit" />
            <article className={`poolCard ${pool.accent} flat`}>
              <div className="poolTop">
                <div>
                  <h2>{pool.label}</h2>
                  <p>{pool.selector}</p>
                </div>
                <span>{pool.nodes} nodes</span>
              </div>
              <div className="poolLoad">
                <LoadBar label="CPU" value={pool.cpu} />
                <LoadBar label="Memory" value={pool.memory} />
              </div>
              <div className="settingStack">
                <Setting label="Timeout" value={pool.timeout} />
                <Setting label="Concurrency" value={`${pool.concurrency} jobs`} />
                <Setting label="Running" value={`${pool.running}`} />
                <Setting label="Queued" value={`${pool.queued}`} />
              </div>
            </article>
          </section>
        ))}

        <section className="panel span8">
          <PanelTitle icon={Network} title="Node Placement" action="Inspect" />
          <div className="nodeMap">
            {["mini-a1", "mini-a2", "mini-a3", "mini-a4", "large-b1", "large-b2", "large-b3", "gpu-c1"].map((node) => (
              <div className="nodeBox" key={node}>
                <Server size={16} aria-hidden="true" />
                <strong>{node}</strong>
                <span>{node.split("-")[0]} pool</span>
              </div>
            ))}
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={Cpu} title="Scheduling Policy" action="View YAML" />
          <pre className="yamlPreview small">{`nodeSelector:
  capsulet.dev/pool: large
tolerations:
  - key: capsulet.dev/pool
    value: large
resources:
  requests:
    cpu: "2"
    memory: 4Gi`}</pre>
        </section>
      </section>
    </DashboardShell>
  );
}

function Setting({ label, value }: { label: string; value: string }) {
  return (
    <div className="settingRow">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
