 "use client";

import {
  Activity,
  AlertTriangle,
  Archive,
  Braces,
  CheckCircle2,
  CircleDot,
  Clock3,
  Cpu,
  Database,
  FileCode2,
  Gauge,
  GitBranch,
  HardDrive,
  Network,
  Pause,
  Play,
  RadioTower,
  Route,
  TerminalSquare,
  Workflow,
  Zap
} from "lucide-react";
import { DashboardShell, LoadBar, PanelTitle, StateBadge } from "./components";
import { automations, pools, runs, timeline } from "./mock-data";
import type { LucideIcon } from "lucide-react";

export default function OverviewPage() {
  return (
    <DashboardShell>
      <section className="heroBand">
        <div className="heroText">
          <div className="eyebrow">
            <CircleDot size={14} aria-hidden="true" />
            Live cluster overview
          </div>
          <h1>Automation runs across Kubernetes execution pools</h1>
          <p>Route jobs from manual, scheduled, webhook, and dependency triggers into the right compute pool.</p>
        </div>
        <div className="heroStats" aria-label="Run summary">
          <Metric icon={Zap} label="Running" value="24" tone="good" />
          <Metric icon={Clock3} label="Queued" value="10" tone="warn" />
          <Metric icon={CheckCircle2} label="Success" value="98.2%" tone="good" />
          <Metric icon={AlertTriangle} label="Failed" value="3" tone="bad" />
        </div>
      </section>

      <section className="contentGrid">
        <section className="panel span8">
          <PanelTitle icon={Workflow} title="Automations" action="View all" />
          <div className="automationList">
            {automations.slice(0, 3).map((automation) => (
              <article className="automationRow" key={automation.name}>
                <div className="automationMain">
                  <div className="automationIcon">
                    <Workflow size={19} aria-hidden="true" />
                  </div>
                  <div>
                    <h2>{automation.name}</h2>
                    <p>{automation.target}</p>
                  </div>
                </div>
                <div className="triggerExpr">
                  <Braces size={16} aria-hidden="true" />
                  <span>{automation.trigger}</span>
                </div>
                <div className="poolPill">
                  <Route size={15} aria-hidden="true" />
                  {automation.pool}
                </div>
                <div className={automation.status === "enabled" ? "status enabled" : "status paused"}>
                  {automation.status === "enabled" ? <Play size={14} /> : <Pause size={14} />}
                  {automation.status}
                </div>
                <div className="successCell">
                  <strong>{automation.success}%</strong>
                  <span>{automation.lastRun}</span>
                </div>
              </article>
            ))}
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={GitBranch} title="Condition Builder" action="Edit" />
          <div className="logicBuilder">
            <div className="logicLine">
              <span className="logicToken group">(</span>
              <ConditionToken label="data_ready" />
              <span className="logicToken op">AND</span>
              <ConditionToken label="approved" />
              <span className="logicToken group">)</span>
            </div>
            <div className="logicLine">
              <span className="logicToken op">OR</span>
              <ConditionToken label="manual_override" />
            </div>
          </div>
          <div className="targetBox">
            <div>
              <span>Target workflow</span>
              <strong>train-model</strong>
            </div>
            <div>
              <span>Execution pool</span>
              <strong>large</strong>
            </div>
          </div>
        </section>

        <section className="panel span7">
          <PanelTitle icon={Activity} title="Recent Runs" action="Open queue" />
          <div className="runTable">
            <div className="runHeader">
              <span>Run</span>
              <span>Automation</span>
              <span>Pool</span>
              <span>State</span>
              <span>Duration</span>
              <span>Node</span>
            </div>
            {runs.slice(0, 4).map((run) => (
              <div className="runRow" key={run.id}>
                <span className="mono tableCell" title={run.id}>
                  {run.id}
                </span>
                <span className="tableCell" title={run.automation}>
                  {run.automation}
                </span>
                <span className="tableCell" title={run.pool}>
                  {run.pool}
                </span>
                <StateBadge state={run.state} />
                <span className="tableCell" title={run.duration}>
                  {run.duration}
                </span>
                <span className="tableCell" title={run.node}>
                  {run.node}
                </span>
              </div>
            ))}
          </div>
        </section>

        <section className="panel span5">
          <PanelTitle icon={Route} title="Execution Pools" action="Manage" />
          <div className="poolStack">
            {pools.map((pool) => (
              <article className={`poolCard ${pool.accent}`} key={pool.name}>
                <div className="poolTop">
                  <div>
                    <h2>{pool.name}</h2>
                    <p>{pool.label}</p>
                  </div>
                  <span>{pool.nodes} nodes</span>
                </div>
                <div className="poolLoad">
                  <LoadBar label="CPU" value={pool.cpu} />
                  <LoadBar label="Memory" value={pool.memory} />
                </div>
                <div className="poolFoot">
                  <span>{pool.running} running</span>
                  <span>{pool.queued} queued</span>
                </div>
              </article>
            ))}
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={Network} title="Topology" action="Inspect" />
          <div className="topology">
            <Node icon={RadioTower} label="Triggers" />
            <Connector />
            <Node icon={Gauge} label="Evaluator" />
            <Connector />
            <Node icon={Database} label="Postgres" />
            <Connector />
            <Node icon={Cpu} label="Workers" />
            <Connector />
            <Node icon={HardDrive} label="Object store" />
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={TerminalSquare} title="Live Logs" action="Tail" />
          <div className="terminal">
            {timeline.map(([time, event, detail]) => (
              <div className="logLine" key={`${time}-${event}`}>
                <span>{time}</span>
                <code>{event}</code>
                <em>{detail}</em>
              </div>
            ))}
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={FileCode2} title="YAML Export" action="Copy" />
          <pre className="yamlPreview">{`apiVersion: capsulet.dev/v1alpha1
kind: Automation
metadata:
  name: nightly-report
spec:
  execution:
    pool: mini
  condition:
    trigger: nightly`}</pre>
        </section>
      </section>
    </DashboardShell>
  );
}

function Metric({
  icon: Icon,
  label,
  value,
  tone
}: {
  icon: LucideIcon;
  label: string;
  value: string;
  tone: string;
}) {
  return (
    <div className={`metric ${tone}`}>
      <Icon size={20} aria-hidden="true" />
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function ConditionToken({ label }: { label: string }) {
  return (
    <span className="conditionToken">
      <CircleDot size={13} aria-hidden="true" />
      {label}
    </span>
  );
}

function Node({ icon: Icon, label }: { icon: LucideIcon; label: string }) {
  return (
    <div className="topologyNode">
      <Icon size={17} aria-hidden="true" />
      <span>{label}</span>
    </div>
  );
}

function Connector() {
  return <div className="connector" aria-hidden="true" />;
}
