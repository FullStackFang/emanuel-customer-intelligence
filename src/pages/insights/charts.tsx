import type React from "react";
import { Fragment } from "react";
import {
  Bar, BarChart, CartesianGrid, Legend, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis,
} from "recharts";
import type { RenderableText } from "recharts";
import type { CohortCell, CohortYear1, ReasonCell, TrendRow } from "../../api";
import { Table } from "../../design-system";
import { fmt, fyLabel, heatInk, heatStep } from "./format";

/* Palette — validated with dataviz/scripts/validate_palette.js (light, white surface).
   Hex values are the design-system tokens named beside them. Do not reorder. */
export const PALETTE = {
  series: ["#3b6eb8", "#d97706", "#0284c7", "#dc2626", "#059669", "#ca8a04"], // primary-500, warning-500, info-500, error-500, success-500, accent-600
  other: "#a8a29e",   // neutral-400 — "Other" / de-emphasis, not a series hue
  emphasis: "#3b6eb8", // primary-500
  deemphasis: "#d6d3d1", // neutral-300
  ramp: ["#dae6ff", "#bdd4ff", "#90baff", "#5c94fc", "#3b6eb8", "#2d5a9e", "#1e4785"], // primary-100…700
  grid: "#e7e5e4",     // neutral-200
  ink: "#78716c",      // neutral-500 (axis text)
};

const axisTick = { fontSize: 12, fill: PALETTE.ink, fontFamily: "var(--font-body)" };
const tooltipStyle = { fontFamily: "var(--font-body)", fontSize: 12, borderRadius: 8, border: "1px solid var(--border-default)" };

// The design-system Table.jsx has no types under allowJs; retype once here.
export interface TableProps<T> {
  getRowKey: (r: T) => string;
  rows: T[];
  empty?: string;
  columns: { key: string; header: string; align?: "left" | "right" | "center"; width?: number | string; render: (r: T) => React.ReactNode }[];
}
export const TypedTable = Table as unknown as <T>(props: TableProps<T>) => React.JSX.Element;

export function TableView<T>({ forceOpen, ...props }: TableProps<T> & { forceOpen?: boolean }) {
  return (
    <details open={forceOpen || undefined} style={{ marginTop: "var(--space-3)" }}>
      <summary style={{ cursor: "pointer", fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>Table view</summary>
      <div style={{ marginTop: "var(--space-2)" }}><TypedTable {...props} /></div>
    </details>
  );
}

export function TrendChart({ rows }: { rows: TrendRow[] }) {
  const data = rows.map((r) => ({ fy: fyLabel(r.fy), active: r.active_end_of_fy }));
  return (
    <ResponsiveContainer width="100%" height={240}>
      <LineChart data={data} margin={{ top: 8, right: 24, bottom: 0, left: 0 }}>
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="fy" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} interval={3} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} domain={["auto", "auto"]} tickFormatter={(v: number) => fmt(v)} width={48} />
        <Tooltip contentStyle={tooltipStyle} formatter={(v) => [fmt(Number(v)), "Member households"]} />
        <Line type="monotone" dataKey="active" stroke={PALETTE.emphasis} strokeWidth={2} dot={false} activeDot={{ r: 5, strokeWidth: 2, stroke: "#fff" }} isAnimationActive={false} />
      </LineChart>
    </ResponsiveContainer>
  );
}

export function FlowsChart({ rows }: { rows: TrendRow[] }) {
  const data = rows.map((r) => ({ fy: fyLabel(r.fy), Joins: r.joins, Resignations: r.resigns }));
  return (
    <ResponsiveContainer width="100%" height={240}>
      <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 0 }} barGap={2} barCategoryGap="30%">
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="fy" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} interval={3} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} width={40} />
        <Tooltip contentStyle={tooltipStyle} />
        <Legend wrapperStyle={{ fontSize: 12, fontFamily: "var(--font-body)" }} formatter={(value) => <span style={{ color: "var(--text-secondary)" }}>{value}</span>} />
        <Bar dataKey="Joins" fill={PALETTE.series[0]} radius={[4, 4, 0, 0]} maxBarSize={24} isAnimationActive={false} />
        <Bar dataKey="Resignations" fill={PALETTE.series[1]} radius={[4, 4, 0, 0]} maxBarSize={24} isAnimationActive={false} />
      </BarChart>
    </ResponsiveContainer>
  );
}

/** Single series with emphasized cohorts. Two stacked keys (one null per row) keep
    Recharts 3 happy without the deprecated <Cell>. */
export function Year1Chart({ rows, emphasize }: { rows: CohortYear1[]; emphasize: number[] }) {
  const data = rows.map((r) => ({
    fy: fyLabel(r.cohort),
    main: emphasize.includes(r.cohort) ? r.pct_retained : null,
    rest: emphasize.includes(r.cohort) ? null : r.pct_retained,
    n: r.n,
  }));
  return (
    <ResponsiveContainer width="100%" height={240}>
      <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 0 }} barCategoryGap="35%">
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="fy" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} domain={[0, 100]} tickFormatter={(v: number) => `${v}%`} width={44} />
        <Tooltip contentStyle={tooltipStyle} formatter={(v) => [`${v}%`, "Retained after 1 year"]} />
        <Bar dataKey="rest" stackId="a" fill={PALETTE.deemphasis} radius={[4, 4, 0, 0]} maxBarSize={24} isAnimationActive={false} />
        <Bar dataKey="main" stackId="a" fill={PALETTE.emphasis} radius={[4, 4, 0, 0]} maxBarSize={24} isAnimationActive={false} />
      </BarChart>
    </ResponsiveContainer>
  );
}

export function CohortHeatmap({ cells }: { cells: CohortCell[] }) {
  const cohorts = [...new Set(cells.map((c) => c.cohort))].sort();
  const ks = [1, 2, 3, 4, 5, 6, 7, 8];
  const n = (c: number) => cells.find((x) => x.cohort === c)?.n ?? 0;
  const at = (c: number, k: number) => cells.find((x) => x.cohort === c && x.k === k);
  const cell = { height: 30, borderRadius: "var(--radius-sm)", display: "flex", alignItems: "center", justifyContent: "center", fontSize: "var(--text-xs)", fontVariantNumeric: "tabular-nums" } as const;
  return (
    <div>
      <div style={{ display: "grid", gridTemplateColumns: `110px repeat(${ks.length}, 1fr)`, gap: 2 }}>
        <div />
        {ks.map((k) => <div key={k} style={{ textAlign: "center", fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>{k} yr{k > 1 ? "s" : ""}</div>)}
        {cohorts.map((c) => (
          <Fragment key={c}>
            <div style={{ alignSelf: "center", textAlign: "right", paddingRight: 8, fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
              {fyLabel(c)} <span style={{ color: "var(--text-tertiary)" }}>({fmt(n(c))})</span>
            </div>
            {ks.map((k) => {
              const v = at(c, k);
              return v
                ? <div key={`${c}-${k}`} title={`${fyLabel(c)} cohort · ${v.pct_retained}% still members after ${k} year${k > 1 ? "s" : ""}`}
                    style={{ ...cell, background: PALETTE.ramp[heatStep(v.pct_retained)], color: heatInk(v.pct_retained) }}>
                    {Math.round(v.pct_retained)}%
                  </div>
                : <div key={`${c}-${k}`} style={cell} />;
            })}
          </Fragment>
        ))}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 8, fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>
        <span>30%</span>
        <div style={{ width: 160, height: 8, borderRadius: 4, background: `linear-gradient(90deg, ${PALETTE.ramp[0]}, ${PALETTE.ramp[6]})` }} />
        <span>90%</span>
      </div>
    </div>
  );
}

export interface HBarRow { label: string; pct: number; n: number; still: number }

export function HBarChart({ rows, emphasize }: { rows: HBarRow[]; emphasize?: string[] }) {
  const deemph = emphasize && emphasize.length > 0;
  const data = rows.map((r) => ({
    label: r.label,
    main: !deemph || emphasize!.includes(r.label) ? r.pct : null,
    rest: deemph && !emphasize!.includes(r.label) ? r.pct : null,
    n: r.n, still: r.still,
  }));
  return (
    <ResponsiveContainer width="100%" height={28 * rows.length + 24}>
      <BarChart data={data} layout="vertical" margin={{ top: 4, right: 48, bottom: 0, left: 8 }} barCategoryGap="25%">
        <CartesianGrid horizontal={false} stroke={PALETTE.grid} />
        <XAxis type="number" domain={[0, 100]} tick={axisTick} tickLine={false} axisLine={false} tickFormatter={(v: number) => `${v}%`} />
        <YAxis type="category" dataKey="label" width={190} tick={axisTick} tickLine={false} axisLine={false} />
        <Tooltip contentStyle={tooltipStyle} formatter={(v, _name, item: { payload?: HBarRow }) => [`${v}% (${fmt(item.payload?.still ?? 0)} of ${fmt(item.payload?.n ?? 0)})`, "Still members"]} />
        <Bar dataKey="rest" stackId="a" fill={PALETTE.deemphasis} radius={[0, 4, 4, 0]} maxBarSize={18} isAnimationActive={false} />
        <Bar dataKey="main" stackId="a" fill={PALETTE.emphasis} radius={[0, 4, 4, 0]} maxBarSize={18} isAnimationActive={false} label={{ position: "right", fontSize: 12, fill: PALETTE.ink, formatter: (v: RenderableText) => `${v}%` }} />
      </BarChart>
    </ResponsiveContainer>
  );
}

/** Fixed series order (matches PALETTE.series); everything else folds into "Other". */
export const REASON_ORDER = ["Non-payment", "Moved", "No longer engaged", "Deceased", "Young-adult tier aged out", "Joined another synagogue"];

export function ReasonsChart({ cells }: { cells: ReasonCell[] }) {
  const fys = [...new Set(cells.map((c) => c.fy))].sort();
  const data = fys.map((fy) => {
    const row: Record<string, number | string> = { fy: fyLabel(fy) };
    let other = 0;
    for (const c of cells.filter((x) => x.fy === fy)) {
      if (REASON_ORDER.includes(c.reason)) row[c.reason] = c.n; else other += c.n;
    }
    row["Other"] = other;
    return row;
  });
  return (
    <ResponsiveContainer width="100%" height={280}>
      <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 0 }} barCategoryGap="45%">
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="fy" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} width={40} />
        <Tooltip contentStyle={tooltipStyle} />
        <Legend wrapperStyle={{ fontSize: 12, fontFamily: "var(--font-body)" }} formatter={(value) => <span style={{ color: "var(--text-secondary)" }}>{value}</span>} />
        {REASON_ORDER.map((r, i) => (
          <Bar key={r} dataKey={r} stackId="a" fill={PALETTE.series[i]} maxBarSize={24} isAnimationActive={false} stroke="#fff" strokeWidth={1} />
        ))}
        <Bar dataKey="Other" stackId="a" fill={PALETTE.other} maxBarSize={24} radius={[4, 4, 0, 0]} isAnimationActive={false} stroke="#fff" strokeWidth={1} />
      </BarChart>
    </ResponsiveContainer>
  );
}
