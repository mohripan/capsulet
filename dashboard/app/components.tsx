"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import {
  Activity,
  Archive,
  Bell,
  Box,
  ChevronDown,
  GitBranch,
  Home,
  Plus,
  RefreshCw,
  Route,
  Search,
  Server,
  Settings,
  ShieldCheck,
  Workflow
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

const nav: Array<[LucideIcon, string, string]> = [
  [Home, "Overview", "/"],
  [Workflow, "Automations", "/automations"],
  [GitBranch, "Workflows", "/workflows"],
  [Activity, "Runs", "/runs"],
  [Route, "Execution Pools", "/execution-pools"],
  [Archive, "Artifacts", "/artifacts"],
  [ShieldCheck, "Security", "/security"],
  [Settings, "Settings", "/settings"]
];

export function DashboardShell({
  children,
  actionLabel = "Automation"
}: {
  children: React.ReactNode;
  actionLabel?: string;
}) {
  const pathname = usePathname();

  return (
    <main className="shell">
      <aside className="sidebar" aria-label="Primary">
        <Link className="brand" href="/">
          <div className="brandMark">
            <Box size={22} aria-hidden="true" />
          </div>
          <div>
            <strong>Capsulet</strong>
            <span>Automation control plane</span>
          </div>
        </Link>

        <nav className="navList">
          {nav.map(([Icon, label, href]) => {
            const active = href === "/" ? pathname === "/" : pathname.startsWith(href);
            return (
              <Link className={active ? "navItem active" : "navItem"} href={href} key={href}>
                <Icon size={18} aria-hidden="true" />
                <span>{label}</span>
              </Link>
            );
          })}
        </nav>

        <div className="clusterPanel">
          <div className="clusterHeader">
            <Server size={17} aria-hidden="true" />
            <span>kind-capsulet</span>
          </div>
          <div className="clusterMeta">
            <span>Kubernetes 1.30</span>
            <span>capsulet ns</span>
          </div>
          <div className="clusterHealth">
            <span />
            Control plane healthy
          </div>
        </div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div className="search">
            <Search size={18} aria-hidden="true" />
            <input aria-label="Search" placeholder="Search automations, runs, artifacts" />
          </div>
          <div className="topbarActions">
            <button className="iconButton" title="Refresh">
              <RefreshCw size={18} aria-hidden="true" />
            </button>
            <button className="iconButton" title="Notifications">
              <Bell size={18} aria-hidden="true" />
            </button>
            <button className="primaryAction">
              <Plus size={18} aria-hidden="true" />
              {actionLabel}
            </button>
          </div>
        </header>
        {children}
      </section>
    </main>
  );
}

export function PageHeader({
  eyebrow,
  title,
  description
}: {
  eyebrow: string;
  title: string;
  description: string;
}) {
  return (
    <section className="pageHeader">
      <span>{eyebrow}</span>
      <h1>{title}</h1>
      <p>{description}</p>
    </section>
  );
}

export function PanelTitle({
  icon: Icon,
  title,
  action
}: {
  icon: LucideIcon;
  title: string;
  action: string;
}) {
  return (
    <div className="panelTitle">
      <div>
        <Icon size={18} aria-hidden="true" />
        <h2>{title}</h2>
      </div>
      <button>
        {action}
        <ChevronDown size={15} aria-hidden="true" />
      </button>
    </div>
  );
}

export function LoadBar({ label, value }: { label: string; value: number }) {
  return (
    <div className="loadBar">
      <div>
        <span>{label}</span>
        <span>{value}%</span>
      </div>
      <div className="track">
        <span style={{ width: `${value}%` }} />
      </div>
    </div>
  );
}

export function StateBadge({ state }: { state: string }) {
  return <span className={`state state-${state.toLowerCase()}`}>{state}</span>;
}
