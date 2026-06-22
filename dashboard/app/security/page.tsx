 "use client";

import { useEffect, useState } from "react";
import { KeyRound, LockKeyhole, Network, ShieldCheck } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";
import { AuditEvent, Principal, getCurrentPrincipal, getErrorMessage, listAuditEvents } from "../lib/api";

export default function SecurityPage() {
  const [principal, setPrincipal] = useState<Principal | null>(null);
  const [auditEvents, setAuditEvents] = useState<AuditEvent[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void getCurrentPrincipal()
      .then(async (identity) => {
        setPrincipal(identity);
        if (identity.role === "admin") {
          const audit = await listAuditEvents();
          setAuditEvents(audit.audit_events.slice(0, 8));
        }
      })
      .catch((reason) => setError(getErrorMessage(reason)));
  }, []);

  return (
    <DashboardShell actionLabel="Policy">
      <PageHeader
        eyebrow="Runtime hardening"
        title="Review sandbox and access controls"
        description="Capsulet runs user code, so security settings must be visible: service accounts, network policy, HMAC webhooks, and pod security."
      />
      {error ? <div className="errorBox">{error}</div> : null}

      <section className="contentGrid">
        <section className="panel span6">
          <PanelTitle icon={ShieldCheck} title="Pod Security Defaults" action="Enforced" />
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
          <PanelTitle icon={Network} title="Network Policy" action="Default deny" />
          <div className="policyMatrix">
            <Policy from="job pods" to="object storage" value="allow" />
            <Policy from="job pods" to="internet" value="deny by default" />
            <Policy from="workers" to="kubernetes api" value="allow" />
            <Policy from="dashboard" to="database" value="deny" />
          </div>
        </section>

        <section className="panel span6">
          <PanelTitle icon={KeyRound} title="Authenticated Session" action={principal?.role || "loading"} />
          <div className="settingStack">
            <Setting label="Principal" value={principal?.name || "-"} />
            <Setting label="Role" value={principal?.role || "-"} />
            <Setting label="API policy" value="deny by default" />
          </div>
        </section>

        <section className="panel span6">
          <PanelTitle icon={LockKeyhole} title="Service Accounts" action="Separated" />
          <div className="settingStack">
            <Setting label="capsulet-api" value="read/write metadata" />
            <Setting label="capsulet-worker" value="create/watch jobs" />
            <Setting label="capsulet-execution" value="no RBAC or API token" />
          </div>
        </section>

        <section className="panel span12">
          <PanelTitle icon={KeyRound} title="Recent Audit Events" action={`${auditEvents.length} shown`} />
          <div className="policyMatrix">
            {auditEvents.length ? auditEvents.map((event) => (
              <Policy key={event.id} from={`${event.principal} · ${event.role}`} to={`${event.method} ${event.path}`} value={String(event.status_code)} />
            )) : <div className="emptyState">{principal?.role === "admin" ? "No mutation audit events yet." : "Admin role is required to view audit events."}</div>}
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
