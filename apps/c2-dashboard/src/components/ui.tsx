"use client";

import React, { useState } from "react";

export function cx(...classes: (string | false | null | undefined)[]): string {
  return classes.filter(Boolean).join(" ");
}

export function Panel({
  title,
  subtitle,
  actions,
  children,
  className,
}: {
  title?: React.ReactNode;
  subtitle?: React.ReactNode;
  actions?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <section className={cx("rounded-lg border border-line bg-panel", className)}>
      {(title || actions) && (
        <header className="flex items-start justify-between gap-3 border-b border-line px-4 py-2.5">
          <div className="min-w-0">
            {title && <h2 className="text-[12px] font-semibold uppercase tracking-[0.08em] text-muted">{title}</h2>}
            {subtitle && <p className="mt-0.5 text-[12px] text-muted/80">{subtitle}</p>}
          </div>
          {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
        </header>
      )}
      <div className="p-4">{children}</div>
    </section>
  );
}

const TONES = {
  neutral: "border-line text-muted",
  accent: "border-accent/40 text-accent",
  commit: "border-commit/40 text-commit",
  withhold: "border-withhold/40 text-withhold",
  reject: "border-reject/40 text-reject",
  review: "border-review/40 text-review",
  ai: "border-ai/40 text-ai",
} as const;

export type Tone = keyof typeof TONES;

export function Badge({ tone = "neutral", children, title, wrap }: { tone?: Tone; children: React.ReactNode; title?: string; wrap?: boolean }) {
  return (
    <span title={title} className={cx("inline-flex items-center gap-1 rounded border px-1.5 py-px text-[11px] font-medium", wrap ? "whitespace-normal [overflow-wrap:anywhere]" : "whitespace-nowrap", TONES[tone])}>
      {children}
    </span>
  );
}

export function outcomeTone(outcome: string | null | undefined): Tone {
  switch (outcome) {
    case "committed":
      return "commit";
    case "withheld":
      return "withhold";
    case "rejected_deterministic":
      return "reject";
    case "awaiting_human_review":
      return "review";
    default:
      return "neutral";
  }
}

export const OUTCOME_COLOR: Record<string, string> = {
  committed: "rgb(var(--commit))",
  withheld: "rgb(var(--withhold))",
  rejected_deterministic: "rgb(var(--reject))",
  awaiting_human_review: "rgb(var(--review))",
};

export function Button({
  children,
  onClick,
  disabled,
  tone = "default",
  title,
  type = "button",
}: {
  children: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  tone?: "default" | "primary" | "danger" | "quiet";
  title?: string;
  type?: "button" | "submit";
}) {
  const styles = {
    default: "border-line bg-raised text-text hover:border-muted/60",
    primary: "border-accent/50 bg-accent/10 text-accent hover:bg-accent/20",
    danger: "border-reject/50 bg-reject/10 text-reject hover:bg-reject/20",
    quiet: "border-transparent text-muted hover:text-text",
  }[tone];
  return (
    <button
      type={type}
      title={title}
      disabled={disabled}
      onClick={onClick}
      className={cx("rounded border px-2.5 py-1 text-[12px] font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-40", styles)}
    >
      {children}
    </button>
  );
}

export function Stat({ label, value, hint, tone }: { label: string; value: React.ReactNode; hint?: string; tone?: Tone }) {
  return (
    <div className="min-w-0" title={hint}>
      <div className="truncate text-[11px] uppercase tracking-wide text-muted">{label}</div>
      <div className={cx("text-[18px] font-semibold tabular-nums", tone && TONES[tone].split(" ")[1])}>{value}</div>
    </div>
  );
}

export function Mono({ children, className, title }: { children: React.ReactNode; className?: string; title?: string }) {
  return (
    <span title={title} className={cx("font-mono text-[12px]", className)}>
      {children}
    </span>
  );
}

export function CopyCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="group flex items-center gap-2 rounded border border-line bg-ink/60 px-2 py-1">
      <code className="min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-[12px] text-text/90">{command}</code>
      <button
        type="button"
        className="shrink-0 text-[11px] text-muted hover:text-text"
        onClick={() => {
          void navigator.clipboard?.writeText(command).then(() => {
            setCopied(true);
            setTimeout(() => setCopied(false), 1200);
          });
        }}
      >
        {copied ? "copied" : "copy"}
      </button>
    </div>
  );
}

export function Empty({ title, children }: { title: string; children?: React.ReactNode }) {
  return (
    <div className="rounded-lg border border-dashed border-line px-6 py-10 text-center">
      <p className="text-[14px] font-medium text-text">{title}</p>
      {children && <div className="mx-auto mt-2 max-w-xl space-y-2 text-muted">{children}</div>}
    </div>
  );
}

export function ErrorNote({ error }: { error: unknown }) {
  if (!error) return null;
  return (
    <p role="alert" className="rounded border border-reject/40 bg-reject/10 px-3 py-2 text-[12px] text-reject">
      {error instanceof Error ? error.message : String(error)}
    </p>
  );
}

export function Loading({ what }: { what: string }) {
  return <p className="py-6 text-center text-muted">Loading {what}…</p>;
}

export function Select<T extends string>({
  value,
  onChange,
  options,
  label,
}: {
  value: T;
  onChange: (value: T) => void;
  options: { value: T; label: string }[];
  label: string;
}) {
  return (
    <label className="flex min-w-0 items-center gap-2 text-[12px] text-muted">
      <span className="shrink-0">{label}</span>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value as T)}
        className="min-w-0 rounded border border-line bg-raised px-2 py-1 text-text"
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}

export function downloadText(name: string, text: string, type = "text/plain"): void {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function Table({ head, children, className }: { head: React.ReactNode[]; children: React.ReactNode; className?: string }) {
  return (
    <div className={cx("overflow-x-auto", className)}>
      <table className="w-full border-collapse text-left text-[12px]">
        <thead>
          <tr className="border-b border-line text-[11px] uppercase tracking-wide text-muted">
            {head.map((h, i) => (
              <th key={i} className="px-2 py-1.5 font-medium">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-line/60">{children}</tbody>
      </table>
    </div>
  );
}
