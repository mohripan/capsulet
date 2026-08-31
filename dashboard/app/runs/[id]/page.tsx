import RunDetailClient from "./run-detail-client";

// capsulet-claims: CAP-JOB-001, CAP-DASHBOARD-001

export default async function RunDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  return <RunDetailClient id={id} />;
}
