 "use client";

import { FormEvent, useEffect, useState } from "react";
import { KeyRound, LockKeyhole, Network, ShieldCheck } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";
import {
  AuditEvent,
  Principal,
  ServiceAccount,
  createServiceAccount,
  getCurrentPrincipal,
  getErrorMessage,
  listAuditEvents,
  listServiceAccounts,
  revokeServiceAccount
} from "../lib/api";

export default function SecurityPage() {
  const [principal, setPrincipal] = useState<Principal | null>(null);
  const [auditEvents, setAuditEvents] = useState<AuditEvent[]>([]);
  const [serviceAccounts, setServiceAccounts] = useState<ServiceAccount[]>([]);
  const [newToken, setNewToken] = useState("");
  const [accountName, setAccountName] = useState("ci-runner");
  const [accountRole, setAccountRole] = useState<"viewer" | "operator" | "admin">("operator");
  const [accountScopes, setAccountScopes] = useState("jobs:run,jobs:read");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void getCurrentPrincipal()
      .then(async (identity) => {
        setPrincipal(identity);
        if (identity.role === "admin") {
          const [audit, accounts] = await Promise.all([listAuditEvents(), listServiceAccounts()]);
          setAuditEvents(audit.audit_events.slice(0, 8));
          setServiceAccounts(accounts.service_accounts);
        }
      })
      .catch((reason) => setError(getErrorMessage(reason)));
  }, []);

  async function refreshServiceAccounts() {
    const accounts = await listServiceAccounts();
    setServiceAccounts(accounts.service_accounts);
  }

  async function submitServiceAccount(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setNewToken("");
    try {
      const response = await createServiceAccount({
        name: accountName,
        role: accountRole,
        scopes: accountScopes.split(",").map((scope) => scope.trim()).filter(Boolean)
      });
      setNewToken(response.token);
      await refreshServiceAccounts();
    } catch (reason) {
      setError(getErrorMessage(reason));
    }
  }

  async function revokeAccount(id: string) {
    setError(null);
    try {
      await revokeServiceAccount(id);
      await refreshServiceAccounts();
    } catch (reason) {
      setError(getErrorMessage(reason));
    }
  }

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
            <Setting label="Platform admin" value={principal?.platform_admin ? "yes" : "no"} />
            <Setting label="Tenant" value={principal?.tenant_id || "-"} />
            <Setting label="Project" value={principal?.project_id || "-"} />
            <Setting label="Project access" value={principal?.project_memberships.map((membership) => `${membership.project_id}:${membership.role}`).join(", ") || "-"} />
            <Setting label="API policy" value="deny by default" />
          </div>
        </section>

        <section className="panel span6">
          <PanelTitle icon={LockKeyhole} title="Service Accounts" action={`${serviceAccounts.length} configured`} />
          {principal?.role === "admin" ? (
            <form className="settingStack" onSubmit={submitServiceAccount}>
              <label>
                <span>Name</span>
                <input value={accountName} onChange={(event) => setAccountName(event.target.value)} />
              </label>
              <label>
                <span>Role</span>
                <select value={accountRole} onChange={(event) => setAccountRole(event.target.value as typeof accountRole)}>
                  <option value="viewer">viewer</option>
                  <option value="operator">operator</option>
                  <option value="admin">admin</option>
                </select>
              </label>
              <label>
                <span>Scopes</span>
                <input value={accountScopes} onChange={(event) => setAccountScopes(event.target.value)} />
              </label>
              <button className="primaryButton" type="submit">Create service account</button>
              {newToken ? <code className="tokenPreview">{newToken}</code> : null}
            </form>
          ) : (
            <div className="emptyState">Admin role is required to manage service accounts.</div>
          )}
        </section>

        <section className="panel span12">
          <PanelTitle icon={LockKeyhole} title="Service Account Inventory" action={`${serviceAccounts.length} shown`} />
          <div className="policyMatrix">
            {serviceAccounts.length ? serviceAccounts.map((account) => (
              <div className="policyRow" key={account.id}>
                <span>{account.name}</span>
                <span>{account.role} · {account.scopes.join(", ")}</span>
                <button className="secondaryButton" type="button" onClick={() => revokeAccount(account.id)} disabled={Boolean(account.revoked_at)}>
                  {account.revoked_at ? "Revoked" : "Revoke"}
                </button>
              </div>
            )) : <div className="emptyState">No service accounts have been created yet.</div>}
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
