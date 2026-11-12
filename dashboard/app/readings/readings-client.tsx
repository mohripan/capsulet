"use client";

// capsulet-claims: CAP-CORRECTNESS-017, CAP-DASHBOARD-001

import { useCallback, useEffect, useState } from "react";
import { Check, ListChecks, RefreshCw, X } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";
import {
  OpenReading,
  ReadingDecision,
  getErrorMessage,
  listOpenReadings,
  readingLine,
  reviewReading,
  reviewedOutcome
} from "../lib/api";

export default function ReadingsClient() {
  const [readings, setReadings] = useState<OpenReading[]>([]);
  const [rationales, setRationales] = useState<Record<string, string>>({});
  const [outcomes, setOutcomes] = useState<Record<string, string>>({});
  const [reviewing, setReviewing] = useState("");
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async function refresh() {
    setIsLoading(true);
    setError(null);
    try {
      const response = await listOpenReadings();
      setReadings(response.readings);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function decide(open: OpenReading, decision: ReadingDecision) {
    const key = open.reading.reading;
    setReviewing(key);
    setError(null);
    try {
      const outcome = await reviewReading(open.certificate_id, key, decision, rationales[key] ?? "");
      setOutcomes((current) => ({ ...current, [open.certificate_id]: reviewedOutcome(outcome) }));
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setReviewing("");
    }
  }

  return (
    <DashboardShell>
      <PageHeader
        eyebrow="Correctness"
        title="Readings to review"
        description="Each reading is a step the kernel could not take: what a cited passage means, what it leaves out, or what a question asks. A decision records who made it, under which authority, and why. An answer is accepted on review only when every reading it rests on holds."
      />

      <section className="contentGrid">
        <section className="panel span12">
          <PanelTitle icon={ListChecks} title="Open readings" action={`${readings.length} open`} />
          <div className="panelActions">
            <button className="secondaryButton" onClick={refresh} disabled={isLoading}>
              <RefreshCw size={16} aria-hidden="true" />
              {isLoading ? "Refreshing" : "Refresh"}
            </button>
          </div>
          {error ? <div className="errorBox">{error}</div> : null}
          {Object.entries(outcomes).map(([certificate, outcome]) => (
            <p className="explanationNote" key={certificate}>
              <span className="mono">{certificate}</span>{` is now ${outcome}.`}
            </p>
          ))}
          {!isLoading && readings.length === 0 ? (
            <div className="emptyState">Nothing to review in this project.</div>
          ) : null}
          <div className="explanationNodes">
            {readings.map((open) => {
              const key = open.reading.reading;
              const rationale = rationales[key] ?? "";
              return (
                <article className="explanationNode" key={key}>
                  <p>
                    <strong>{open.question}</strong>{" "}
                    <span className="explanationKind mono">{open.certificate_id}</span>
                  </p>
                  <p className="explanationNote">
                    {open.reading.kind === "question" ? "A reading of the question: " : "A reading of the source: "}
                    <span className="mono">{readingLine(open.reading)}</span>
                  </p>
                  <p className="explanationNote">{open.reading.rationale}</p>
                  {open.reading.evidence_ids.length > 0 ? (
                    <p className="explanationNote">
                      Evidence: <span className="mono">{open.reading.evidence_ids.join(", ")}</span>
                    </p>
                  ) : null}
                  <label className="explanationNote">
                    Why
                    <textarea
                      aria-label={`Why, for ${key}`}
                      rows={2}
                      value={rationale}
                      onChange={(event) => setRationales((current) => ({ ...current, [key]: event.target.value }))}
                    />
                  </label>
                  <div className="panelActions">
                    <button
                      className="secondaryButton"
                      disabled={reviewing === key || rationale.trim() === ""}
                      onClick={() => void decide(open, "holds")}
                    >
                      <Check size={16} aria-hidden="true" />
                      Holds
                    </button>
                    <button
                      className="secondaryButton"
                      disabled={reviewing === key || rationale.trim() === ""}
                      onClick={() => void decide(open, "does_not_hold")}
                    >
                      <X size={16} aria-hidden="true" />
                      Does not hold
                    </button>
                  </div>
                </article>
              );
            })}
          </div>
        </section>
      </section>
    </DashboardShell>
  );
}
