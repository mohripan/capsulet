 "use client";

import { KeyRound, LockKeyhole, Network, ShieldCheck } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";

export default function SecurityPage() {
  return (
    <DashboardShell actionLabel="Policy">
      <PageHeader
        eyebrow="Runtime hardening"
        title="Review sandbox and access controls"
        description="Capsulet runs user code, so security settings must be visible: service accounts, network policy, HMAC webhooks, and pod security."
      />

      <section className="contentGrid">
        <section className="panel span6">
          <PanelTitle icon={ShieldCheck} title="Pod Security Defaults" action="Edit" />
          <div className="checkList">
            {[
              "runAsNonRoot",
              "allowPrivilegeEscalation=false",
              "readOnlyRootFilesystem",
              "capabilities.drop=ALL",
              "seccompProfile=RuntimeDefault"
            ].map((item) => (
              <div className="checkItem" key={item}>
                <ShieldCheck size={16} aria-hidden="true" />
                <span>{item}</span>
              </div>
            ))}
          </div>
        </section>

        <section className="panel span6">
          <PanelTitle icon={Network} title="Network Policy" action="Preview" />
          <div className="policyMatrix">
            <Policy from="job pods" to="object storage" value="allow" />
            <Policy from="job pods" to="internet" value="deny by default" />
            <Policy from="workers" to="kubernetes api" value="allow" />
            <Policy from="dashboard" to="database" value="deny" />
          </div>
        </section>

        <section className="panel span6">
          <PanelTitle icon={KeyRound} title="Webhook Authentication" action="Rotate" />
          <div className="settingStack">
            <Setting label="Mode" value="HMAC signed secret" />
            <Setting label="Timestamp window" value="5 minutes" />
            <Setting label="Replay protection" value="dedupe key" />
          </div>
        </section>

        <section className="panel span6">
          <PanelTitle icon={LockKeyhole} title="Service Accounts" action="Inspect RBAC" />
          <div className="settingStack">
            <Setting label="capsulet-api" value="read/write metadata" />
            <Setting label="capsulet-worker" value="create/watch jobs" />
            <Setting label="script-job" value="no api access" />
          </div>
        </section>
      </section>
    </DashboardShell>
  );
}

function Policy({ from, to, value }: { from: string; to: string; value: string }) {
  return (
    <div className="policyRow">
      <span>{from}</span>
      <span>{to}</span>
      <strong>{value}</strong>
    </div>
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
