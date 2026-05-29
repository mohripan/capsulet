 "use client";

import { Braces, CalendarClock, Pause, Play, Route, ShieldCheck, Workflow } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";
import { automations } from "../mock-data";

export default function AutomationsPage() {
  return (
    <DashboardShell actionLabel="Automation">
      <PageHeader
        eyebrow="Automation builder"
        title="Define trigger-driven work"
        description="Create named automations, compose trigger conditions, choose an execution pool, and bind them to jobs or workflows."
      />

      <section className="contentGrid">
        <section className="panel span8">
          <PanelTitle icon={Workflow} title="Automation Catalog" action="Filter" />
          <div className="resourceList">
            {automations.map((automation) => (
              <article className="resourceRow" key={automation.name}>
                <div className="resourceMain">
                  <div className="automationIcon">
                    <Workflow size={19} aria-hidden="true" />
                  </div>
                  <div>
                    <h2>{automation.name}</h2>
                    <p>{automation.owner} / {automation.target}</p>
                  </div>
                </div>
                <div className="triggerExpr">
                  <Braces size={16} aria-hidden="true" />
                  <span>{automation.condition}</span>
                </div>
                <div className="poolPill">
                  <Route size={15} aria-hidden="true" />
                  {automation.pool}
                </div>
                <div className={automation.status === "enabled" ? "status enabled" : "status paused"}>
                  {automation.status === "enabled" ? <Play size={14} /> : <Pause size={14} />}
                  {automation.status}
                </div>
              </article>
            ))}
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={Braces} title="Trigger Logic" action="Validate" />
          <div className="builderCanvas">
            <div className="builderNode">data_ready</div>
            <div className="builderOp">AND</div>
            <div className="builderNode">approved</div>
            <div className="builderOp wide">OR</div>
            <div className="builderNode">manual_override</div>
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={CalendarClock} title="Trigger Types" action="Add" />
          <div className="tileGrid">
            {["manual", "schedule", "delay", "webhook", "dependency", "event"].map((item) => (
              <div className="miniTile" key={item}>
                <strong>{item}</strong>
                <span>available</span>
              </div>
            ))}
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={ShieldCheck} title="Webhook Policy" action="Edit" />
          <div className="settingStack">
            <Setting label="Authentication" value="HMAC signatures" />
            <Setting label="Replay window" value="5 minutes" />
            <Setting label="Idempotency" value="dedupe key required" />
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={Route} title="Default Routing" action="Change" />
          <div className="settingStack">
            <Setting label="Default pool" value="mini" />
            <Setting label="Timeout" value="120 seconds" />
            <Setting label="Retention" value="job definition default" />
          </div>
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
