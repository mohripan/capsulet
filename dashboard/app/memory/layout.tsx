import { DashboardShell } from "../components";
import type { ReactNode } from "react";

export default function MemoryLayout({ children }: { children: ReactNode }) {
  return <DashboardShell>{children}</DashboardShell>;
}
// capsulet-claims: CAP-MEMORY-001, CAP-DASHBOARD-001
