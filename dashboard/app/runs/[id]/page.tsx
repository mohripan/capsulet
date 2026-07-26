import RunDetailClient from "./run-detail-client";

export default async function RunDetailPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  return <RunDetailClient id={id} />;
}
