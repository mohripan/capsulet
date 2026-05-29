 "use client";

import { Database, GitBranch, Settings, SlidersHorizontal } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";

export default function SettingsPage() {
  return (
    <DashboardShell actionLabel="Save changes">
      <PageHeader
        eyebrow="Configuration"
        title="Configure platform defaults"
        description="Set the defaults that will eventually map to Helm values, API-managed resources, and operator policies."
      />

      <section className="contentGrid">
        <section className="panel span6">
          <PanelTitle icon={Settings} title="General" action="Edit" />
          <div className="settingStack">
            <Setting label="Namespace" value="capsulet" />
            <Setting label="Dashboard" value="enabled by default" />
            <Setting label="Default execution pool" value="mini" />
            <Setting label="Default timeout" value="120 seconds" />
          </div>
        </section>

        <section className="panel span6">
          <PanelTitle icon={Database} title="Storage and Retention" action="Edit" />
          <div className="settingStack">
            <Setting label="Script bundles" value="object storage" />
            <Setting label="Log chunks" value="object storage" />
            <Setting label="Artifacts" value="30 days" />
            <Setting label="Audit events" value="180 days" />
          </div>
        </section>

        <section className="panel span6">
          <PanelTitle icon={GitBranch} title="Eventing" action="Configure" />
          <div className="settingStack">
            <Setting label="Production bus" value="Kafka" />
            <Setting label="Local fallback" value="in-process or database-backed" />
            <Setting label="Idempotency" value="required" />
          </div>
        </section>

        <section className="panel span6">
          <PanelTitle icon={SlidersHorizontal} title="YAML Authoring" action="Export" />
          <div className="settingStack">
            <Setting label="Import" value="API and CLI" />
            <Setting label="Export" value="API and CLI" />
            <Setting label="Dashboard raw YAML" value="disabled for now" />
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
