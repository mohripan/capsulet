"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  Activity,
  Archive,
  Bell,
  Box,
  CalendarClock,
  CheckCheck,
  ChevronDown,
  CircleDotDashed,
  DatabaseZap,
  FileCode2,
  GitBranch,
  Layers3,
  ListTree,
  LogOut,
  Network,
  Plus,
  PlugZap,
  RefreshCw,
  Route,
  Search,
  Server,
  Settings,
  ShieldCheck,
  Workflow
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { CSSProperties, PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { listProjects, selectedProjectId, setSelectedProjectId, type Project } from "./lib/api";

const nav: Array<[LucideIcon, string, string]> = [
  [Network, "Graph Workbench", "/memory"],
  [Layers3, "Subgraphs", "/memory/subgraphs"],
  [CircleDotDashed, "Claims", "/memory/traces"],
  [DatabaseZap, "Entities", "/memory/entities"],
  [CheckCheck, "Contradictions", "/memory/edges"],
  [FileCode2, "Schema Studio", "/memory/contracts"],
  [Activity, "Agent Sessions", "/runs"],
  [Route, "Retrieval Policies", "/workflows"],
  [ListTree, "Evaluations", "/logs"],
  [ShieldCheck, "Security", "/security"],
  [Settings, "Settings", "/settings"]
];

const legacyNav: Array<[LucideIcon, string, string]> = [
  [Workflow, "Automations", "/automations"],
  [PlugZap, "Trigger Plugins", "/trigger-plugins"],
  [GitBranch, "Workflows", "/workflows"],
  [FileCode2, "Job Definitions", "/job-definitions"],
  [Archive, "Artifacts", "/artifacts"],
  [Route, "Execution Pools", "/execution-pools"]
];

export function DashboardShell({
  children,
  actionLabel,
  actionHref
}: {
  children: ReactNode;
  actionLabel?: string;
  actionHref?: string;
}) {
  const pathname = usePathname();
  const [projects, setProjects] = useState<Project[]>([]);
  const [activeProject, setActiveProject] = useState("");

  useEffect(() => {
    void listProjects()
      .then((response) => {
        setProjects(response.projects);
        const storedProject = selectedProjectId();
        const nextProject = response.projects.some((project) => project.id === storedProject)
          ? storedProject
          : response.projects[0]?.id || "";
        if (nextProject) {
          setSelectedProjectId(nextProject);
        }
        setActiveProject((current) => current || nextProject);
      })
      .catch(() => {
        setProjects([]);
        setActiveProject("");
      });
  }, []);

  return (
    <main className="min-h-screen bg-capsulet-bg text-capsulet-text">
      <div className="grid min-h-screen grid-cols-[252px_minmax(0,1fr)] max-lg:grid-cols-1">
      <aside className="border-r border-capsulet-line bg-[#091620] px-3.5 py-4 max-lg:hidden" aria-label="Primary">
        <Link className="mb-6 flex items-center gap-2.5 rounded-md px-2" href="/memory">
          <div className="grid size-8 place-items-center rounded-md bg-docker-500 text-sm font-black text-white">
            <Box size={22} aria-hidden="true" />
          </div>
          <div className="min-w-0">
            <strong className="block text-sm text-slate-50">Capsulet</strong>
            <span className="block truncate text-xs text-capsulet-muted">Memory operating system</span>
          </div>
        </Link>

        <div className="mb-2 px-2 text-[11px] font-semibold uppercase tracking-wider text-capsulet-muted">Memory</div>
        <nav className="flex flex-col gap-0.5">
          {nav.map(([Icon, label, href]) => {
            const active = href === "/" ? pathname === "/" : pathname.startsWith(href);
            return (
              <Link
                className={
                  active
                    ? "flex items-center gap-2 rounded-md bg-docker-800 px-2.5 py-2 text-sm font-semibold text-docker-100"
                    : "flex items-center gap-2 rounded-md px-2.5 py-2 text-sm text-slate-300 hover:bg-[#0e1e2a] hover:text-white"
                }
                href={href}
                key={href}
              >
                <Icon size={18} aria-hidden="true" />
                <span className="truncate">{label}</span>
              </Link>
            );
          })}
        </nav>

        <div className="mt-6 mb-2 px-2 text-[11px] font-semibold uppercase tracking-wider text-capsulet-muted">Legacy ops</div>
        <nav className="flex flex-col gap-0.5">
          {legacyNav.map(([Icon, label, href]) => {
            const active = href === "/" ? pathname === "/" : pathname.startsWith(href);
            return (
              <Link
                className={
                  active
                    ? "flex items-center gap-2 rounded-md bg-[#102131] px-2.5 py-2 text-sm font-semibold text-slate-100"
                    : "flex items-center gap-2 rounded-md px-2.5 py-2 text-sm text-slate-400 hover:bg-[#0e1e2a] hover:text-white"
                }
                href={href}
                key={href}
              >
                <Icon size={17} aria-hidden="true" />
                <span className="truncate">{label}</span>
              </Link>
            );
          })}
        </nav>

        <div className="mt-6 rounded-md border border-capsulet-subtle bg-capsulet-canvas p-3">
          <div className="flex items-center gap-2 text-sm font-semibold text-slate-100">
            <Server size={17} aria-hidden="true" />
            Local memory stack
          </div>
          <div className="mt-2 flex flex-wrap gap-1.5 text-[11px] text-capsulet-muted">
            <span>private models</span>
            <span>governed graph</span>
          </div>
          <div className="mt-3 flex items-center gap-2 text-xs text-docker-200">
            <span className="size-2 rounded-full bg-docker-500" />
            Runtime connected
          </div>
        </div>
      </aside>

      <section className="min-w-0 bg-capsulet-bg">
        <header className="flex h-[58px] items-center justify-between gap-4 border-b border-capsulet-line bg-capsulet-shell px-4">
          <div className="flex min-w-0 flex-1 items-center gap-2 rounded-md border border-capsulet-line bg-capsulet-bg px-3 py-2 text-capsulet-muted md:max-w-[520px]">
            <Search size={18} aria-hidden="true" />
            <input
              aria-label="Search"
              className="min-w-0 flex-1 bg-transparent text-sm text-capsulet-text outline-none placeholder:text-capsulet-muted"
              placeholder="Search claims, entities, subgraphs, evidence"
            />
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <label className="hidden items-center gap-2 text-xs text-capsulet-muted md:flex">
              <span>Project</span>
              <select
                className="rounded-md border border-capsulet-line bg-capsulet-bg px-2 py-1.5 text-sm text-capsulet-text outline-none"
                value={activeProject}
                onChange={(event) => {
                  setActiveProject(event.target.value);
                  setSelectedProjectId(event.target.value);
                  window.location.reload();
                }}
              >
                {projects.length ? projects.map((project) => (
                  <option value={project.id} key={`${project.tenant_id}:${project.id}`}>{project.name}</option>
                )) : <option value="">No project</option>}
              </select>
            </label>
            <button className="grid size-9 place-items-center rounded-md border border-capsulet-line bg-capsulet-panel text-capsulet-muted hover:text-white" title="Refresh">
              <RefreshCw size={18} aria-hidden="true" />
            </button>
            <button className="grid size-9 place-items-center rounded-md border border-capsulet-line bg-capsulet-panel text-capsulet-muted hover:text-white" title="Notifications">
              <Bell size={18} aria-hidden="true" />
            </button>
            <form action="/api/auth/logout" method="post">
              <button className="grid size-9 place-items-center rounded-md border border-capsulet-line bg-capsulet-panel text-capsulet-muted hover:text-white" title="Sign out" type="submit">
                <LogOut size={18} aria-hidden="true" />
              </button>
            </form>
            {actionLabel && actionHref ? (
              <Link className="inline-flex items-center gap-2 rounded-md bg-docker-500 px-3 py-2 text-sm font-semibold text-white hover:bg-docker-600" href={actionHref}>
                <Plus size={18} aria-hidden="true" />
                {actionLabel}
              </Link>
            ) : null}
          </div>
        </header>
        {children}
      </section>
      </div>
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

function normalizedDateTimeValue(value: string) {
  if (!value) return "";
  const [date, rawTime = ""] = value.includes("T") ? value.split("T") : value.split(" ");
  if (!date) return "";
  return `${date}T${rawTime.slice(0, 5) || "00:00"}`;
}

function dateTimeInputValue(date: Date) {
  const offsetDate = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return offsetDate.toISOString().slice(0, 16);
}

export function defaultDateTimeRange(daysBack = 2) {
  const end = new Date();
  const start = new Date(end.getTime() - daysBack * 24 * 60 * 60 * 1000);
  return {
    start: dateTimeInputValue(start),
    end: dateTimeInputValue(end)
  };
}

export function DateTimePicker({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  const inputRef = useRef<HTMLInputElement | null>(null);

  function openPicker() {
    const picker = inputRef.current as (HTMLInputElement & { showPicker?: () => void }) | null;
    try {
      picker?.showPicker?.();
    } catch {
      // Some browsers only allow showPicker from direct pointer activation.
    }
    picker?.focus();
  }

  return (
    <div className="dateTimePicker">
      <input
        ref={inputRef}
        type="datetime-local"
        step={60}
        value={normalizedDateTimeValue(value)}
        onClick={openPicker}
        onFocus={openPicker}
        onChange={(event) => onChange(event.target.value)}
      />
      <button type="button" aria-label="Open date and time picker" title="Open date and time picker" onClick={openPicker}>
        <CalendarClock size={16} aria-hidden="true" />
      </button>
    </div>
  );
}

function formatDuration(seconds: number) {
  const safeSeconds = Math.max(0, Math.floor(Number.isFinite(seconds) ? seconds : 0));
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  const remainingSeconds = safeSeconds % 60;
  return [hours, minutes, remainingSeconds].map((part) => String(part).padStart(2, "0")).join(":");
}

function parseDuration(value: string) {
  const match = value.trim().match(/^(\d{1,3}):([0-5]\d):([0-5]\d)$/);
  if (!match) return null;
  return Number(match[1]) * 3600 + Number(match[2]) * 60 + Number(match[3]);
}

export function DurationInput({
  valueSeconds,
  minSeconds = 60,
  onChange
}: {
  valueSeconds: number;
  minSeconds?: number;
  onChange: (seconds: number) => void;
}) {
  const [draft, setDraft] = useState(formatDuration(valueSeconds));

  useEffect(() => {
    setDraft(formatDuration(valueSeconds));
  }, [valueSeconds]);

  function commit(value: string) {
    const seconds = parseDuration(value);
    if (seconds === null) {
      setDraft(formatDuration(Math.max(minSeconds, valueSeconds)));
      return;
    }
    const next = Math.max(minSeconds, seconds);
    setDraft(formatDuration(next));
    onChange(next);
  }

  return (
    <input
      className="durationInput"
      inputMode="numeric"
      placeholder="00:05:00"
      value={draft}
      onBlur={() => commit(draft)}
      onChange={(event) => {
        setDraft(event.target.value);
        const seconds = parseDuration(event.target.value);
        if (seconds !== null && seconds >= minSeconds) {
          onChange(seconds);
        }
      }}
    />
  );
}

export type ResizableGridColumn = {
  label: string;
  width: number;
  minWidth?: number;
  sortKey?: string;
};

export function ResizableGridTable({
  columns,
  children,
  className,
  sortKey,
  sortDirection,
  onSort
}: {
  columns: ResizableGridColumn[];
  children: ReactNode;
  className?: string;
  sortKey?: string;
  sortDirection?: "asc" | "desc";
  onSort?: (sortKey: string) => void;
}) {
  const [widths, setWidths] = useState(() => columns.map((column) => column.width));
  const template = useMemo(() => widths.map((width) => `${Math.round(width)}px`).join(" "), [widths]);
  const style = { "--table-columns": template } as CSSProperties;

  function startResize(index: number, event: ReactPointerEvent<HTMLButtonElement>) {
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = widths[index];
    const minWidth = columns[index].minWidth ?? 88;

    function onPointerMove(pointerEvent: PointerEvent) {
      const nextWidth = Math.max(minWidth, startWidth + pointerEvent.clientX - startX);
      setWidths((current) => current.map((width, widthIndex) => (widthIndex === index ? nextWidth : width)));
    }

    function onPointerUp() {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    }

    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }

  return (
    <div className={className ? `resizableTable ${className}` : "resizableTable"} style={style}>
      <div className="resizableHeader">
        {columns.map((column, index) => (
          <div className="resizableHeadCell" key={column.label}>
            {column.sortKey && onSort ? (
              <button className="sortHeaderButton" type="button" onClick={() => onSort(column.sortKey!)}>
                <span>{column.label}</span>
                <span aria-hidden="true">{sortKey === column.sortKey ? (sortDirection === "asc" ? "↑" : "↓") : ""}</span>
              </button>
            ) : (
              <span>{column.label}</span>
            )}
            <button
              aria-label={`Resize ${column.label} column`}
              className="columnResizeHandle"
              title={`Resize ${column.label} column`}
              type="button"
              onPointerDown={(event) => startResize(index, event)}
            />
          </div>
        ))}
      </div>
      {children}
    </div>
  );
}

export function PythonEditor({ value, onChange, rows = 16 }: { value: string; onChange: (value: string) => void; rows?: number }) {
  const minHeight = Math.max(190, rows * 20 + 34);
  const style = { "--python-editor-min-height": `${minHeight}px` } as CSSProperties;

  function handleKeyDown(event: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key !== "Tab") return;
    event.preventDefault();
    const target = event.currentTarget;
    const start = target.selectionStart;
    const end = target.selectionEnd;
    const next = `${value.slice(0, start)}    ${value.slice(end)}`;
    onChange(next);
    requestAnimationFrame(() => {
      target.selectionStart = start + 4;
      target.selectionEnd = start + 4;
    });
  }

  return (
    <div className="pythonEditor" style={style}>
      <pre aria-hidden="true" dangerouslySetInnerHTML={{ __html: highlightPython(value) }} />
      <textarea
        value={value}
        spellCheck={false}
        rows={rows}
        onKeyDown={handleKeyDown}
        onChange={(event) => onChange(event.target.value)}
      />
    </div>
  );
}

function highlightPython(value: string) {
  return escapeHtml(value)
    .replace(/\b(import|from|def|return|if|else|for|while|in|with|as|print|class|try|except)\b/g, "<span class=\"pyKeyword\">$1</span>")
    .replace(/(&quot;[^&]*?&quot;|'[^']*?')/g, "<span class=\"pyString\">$1</span>")
    .replace(/(#.*)$/gm, "<span class=\"pyComment\">$1</span>");
}

function escapeHtml(value: string) {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
