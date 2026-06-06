"use client";

import { useEffect, useState } from "react";
import { RefreshCw, Route } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";
import { ExecutionPool, getErrorMessage, listExecutionPools } from "../lib/api";

export default function ExecutionPoolsPage() {
  const [pools, setPools] = useState<ExecutionPool[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    setIsLoading(true);
    setError(null);
    try {
      const response = await listExecutionPools();
      setPools(response.execution_pools);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  return (
    <DashboardShell>
      <PageHeader
        eyebrow="Compute routing"
        title="Configured execution pools"
        description="Execution pools are configured by the Capsulet runtime and chart. Jobs and workflow steps choose from these API-backed pools."
      />

      <section className="contentGrid">
        <section className="panel span12">
          <PanelTitle icon={Route} title="Execution Pools" action="Live API" />
          <div className="panelActions">
            <button className="secondaryButton" onClick={refresh} disabled={isLoading}>
              <RefreshCw size={16} aria-hidden="true" />
              {isLoading ? "Refreshing" : "Refresh"}
            </button>
          </div>
          {error ? <div className="errorBox">{error}</div> : null}
          <div className="resourceList">
            {!isLoading && pools.length === 0 ? (
              <div className="emptyState">No execution pools are configured for this API process.</div>
            ) : null}
            {pools.map((pool) => (
              <article className="resourceRow" key={pool.name}>
                <div className="resourceMain">
                  <div className="automationIcon">
                    <Route size={19} aria-hidden="true" />
                  </div>
                  <div>
                    <h2>{pool.name}</h2>
                    <p>{pool.description}</p>
                  </div>
                </div>
                <div className={pool.is_default ? "status enabled" : "status"}>
                  {pool.is_default ? "default" : "available"}
                </div>
              </article>
            ))}
          </div>
        </section>
      </section>
    </DashboardShell>
  );
}
