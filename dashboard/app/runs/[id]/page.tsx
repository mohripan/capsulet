import RunDetailClient from "./run-detail-client";

export default function RunDetailPage({ params }: { params: { id: string } }) {
  return <RunDetailClient id={params.id} />;
}
