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
  ChevronDown,
  FileCode2,
  GitBranch,
  Home,
  ListTree,
  LogOut,
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
import type { CSSProperties, PointerEvent as ReactPointerEvent, ReactNode } from "react";

const nav: Array<[LucideIcon, string, string]> = [
  [Home, "Overview", "/"],
  [Workflow, "Automations", "/automations"],
  [GitBranch, "Workflows", "/workflows"],
  [FileCode2, "Job Definitions", "/job-definitions"],
  [Activity, "Runs", "/runs"],
  [ListTree, "Live Logs", "/logs"],
  [Route, "Execution Pools", "/execution-pools"],
  [Archive, "Artifacts", "/artifacts"],
  [ShieldCheck, "Security", "/security"],
  [Settings, "Settings", "/settings"]
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
            <form action="/api/auth/logout" method="post">
              <button className="iconButton" title="Sign out" type="submit">
                <LogOut size={18} aria-hidden="true" />
              </button>
            </form>
            {actionLabel && actionHref ? (
              <Link className="primaryAction" href={actionHref}>
                <Plus size={18} aria-hidden="true" />
                {actionLabel}
              </Link>
            ) : null}
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
