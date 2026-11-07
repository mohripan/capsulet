import AgentRunDetailClient from "./agent-run-detail-client";

// capsulet-claims: CAP-AGENT-001, CAP-AGENT-002, CAP-AGENT-004, CAP-DASHBOARD-001

export default async function AgentRunPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  return <AgentRunDetailClient id={id} />;
}
