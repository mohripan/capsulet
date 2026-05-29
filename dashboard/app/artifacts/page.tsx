 "use client";

import { Archive, Database, FileCode2, HardDrive } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";
import { artifacts } from "../mock-data";

export default function ArtifactsPage() {
  return (
    <DashboardShell actionLabel="Upload policy">
      <PageHeader
        eyebrow="Object storage"
        title="Inspect bundles, logs, and artifacts"
        description="Capsulet stores scripts, logs, and run outputs in object storage while PostgreSQL keeps metadata and references."
      />

      <section className="contentGrid">
        <section className="panel span8">
          <PanelTitle icon={Archive} title="Artifacts" action="Filter" />
          <div className="artifactTable">
            {artifacts.map((artifact) => (
              <article className="artifactRow" key={artifact.name}>
                <FileCode2 size={18} aria-hidden="true" />
                <div>
                  <h2>{artifact.name}</h2>
                  <p>{artifact.run} / {artifact.type}</p>
                </div>
                <span>{artifact.size}</span>
                <span>{artifact.retention}</span>
              </article>
            ))}
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={HardDrive} title="Storage Summary" action="Configure" />
          <div className="settingStack">
            <Setting label="Provider" value="MinIO / S3" />
            <Setting label="Bucket" value="capsulet-artifacts" />
            <Setting label="Used" value="42.8 GB" />
            <Setting label="Retention cleanup" value="daily" />
          </div>
        </section>

        <section className="panel span12">
          <PanelTitle icon={Database} title="Metadata Boundary" action="Details" />
          <div className="wideNotice">
            <strong>PostgreSQL stores references, not blobs.</strong>
            <span>Script bundles, log chunks, input payloads, output payloads, and artifacts are stored in object storage.</span>
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
