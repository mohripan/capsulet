import { DashboardShell } from "../components";
import type { ReactNode } from "react";

export default function MemoryLayout({ children }: { children: ReactNode }) {
  return <DashboardShell>{children}</DashboardShell>;
}
// capsulet-claims: CAP-MEMORY-001, CAP-MEMORY-002, CAP-MEMORY-003, CAP-MEMORY-004, CAP-MEMORY-005, CAP-MEMORY-006, CAP-MEMORY-007, CAP-DASHBOARD-001
