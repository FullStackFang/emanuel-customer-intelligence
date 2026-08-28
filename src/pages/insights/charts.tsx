import type React from "react";
import { Fragment } from "react";
import {
  Bar, BarChart, CartesianGrid, Legend, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis,
} from "recharts";
import type { RenderableText } from "recharts";
import type { CohortCell, CohortMakeupRow, CohortYear1, ConcentrationRow, DuesRow, FinancialCohortRow, FinancialYearClassRow, FinancialYearRow, OutcomeByTenureRow, ReasonCell, TrendRow } from "../../api";
import { Table } from "../../design-system";
import { fmt, fmtMoney, fmtMoneyShort, fyLabel, heatInk, heatStep } from "./format";

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

export function TableView<T>(props: TableProps<T>) {
  return (
    <details style={{ marginTop: "var(--space-3)" }}>
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

/** How many of today's members each join-year cohort still contributes — the count
    complement of the retention grid. Two newest cohorts emphasized, matching Year1Chart. */
export function CohortMakeupChart({ rows, emphasize }: { rows: CohortMakeupRow[]; emphasize: number[] }) {
  const data = rows.map((r) => ({
    fy: fyLabel(r.cohort),
    main: emphasize.includes(r.cohort) ? r.current : null,
    rest: emphasize.includes(r.cohort) ? null : r.current,
    pct: r.pct_of_base,
  }));
  return (
    <ResponsiveContainer width="100%" height={240}>
      <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 0 }} barCategoryGap="35%">
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="fy" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} tickFormatter={(v: number) => fmt(v)} width={44} />
        <Tooltip contentStyle={tooltipStyle} formatter={(v, _name, item: { payload?: { pct?: number } }) => [`${fmt(Number(v))} households · ${item.payload?.pct ?? 0}% of base`, "Still members"]} />
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

/** Pareto of the money in: the cumulative share of the year's dollars held by the top N%
    of member households, ranked by cash received. Received is the primary lens; billed
    (dashed) sits alongside so the reader sees whether the biggest payers are billed as
    heavily as they pay. A steep early rise means the base leans on a few households. */
export function ConcentrationChart({ rows }: { rows: ConcentrationRow[] }) {
  const data = rows.map((r) => ({
    band: `Top ${r.decile * 10}%`,
    Received: r.cumulative_received_share,
    Billed: r.cumulative_billed_share,
  }));
  return (
    <ResponsiveContainer width="100%" height={260}>
      <LineChart data={data} margin={{ top: 8, right: 24, bottom: 0, left: 0 }}>
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="band" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} interval={0} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} domain={[0, 100]} tickFormatter={(v: number) => `${v}%`} width={44} />
        <Tooltip contentStyle={tooltipStyle} formatter={(v, name) => [`${v}% of the year's ${String(name).toLowerCase()}`, name]} />
        <Legend wrapperStyle={{ fontSize: 12, fontFamily: "var(--font-body)" }} formatter={(value) => <span style={{ color: "var(--text-secondary)" }}>{value}</span>} />
        <Line type="monotone" dataKey="Received" stroke={PALETTE.series[0]} strokeWidth={2} dot={{ r: 3 }} isAnimationActive={false} />
        <Line type="monotone" dataKey="Billed" stroke={PALETTE.series[1]} strokeWidth={2} strokeDasharray="4 3" dot={{ r: 3 }} isAnimationActive={false} />
      </LineChart>
    </ResponsiveContainer>
  );
}

/** Money in over time: cash received (colored) beside amount billed (grey) per complete
    fiscal year, so both growth and the widening/narrowing collection gap read at a glance. */
export function MoneyOverTimeChart({ rows }: { rows: FinancialYearRow[] }) {
  const data = rows.map((r) => ({ fy: fyLabel(r.fy), Received: r.received, Billed: r.billed }));
  return (
    <ResponsiveContainer width="100%" height={260}>
      <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 8 }} barGap={2} barCategoryGap="30%">
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="fy" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} width={56} tickFormatter={(v: number) => fmtMoneyShort(v)} />
        <Tooltip contentStyle={tooltipStyle} formatter={(v, name) => [fmtMoney(Number(v)), name]} />
        <Legend wrapperStyle={{ fontSize: 12, fontFamily: "var(--font-body)" }} formatter={(value) => <span style={{ color: "var(--text-secondary)" }}>{value}</span>} />
        <Bar dataKey="Received" fill={PALETTE.series[4]} radius={[4, 4, 0, 0]} maxBarSize={40} isAnimationActive={false} />
        <Bar dataKey="Billed" fill={PALETTE.deemphasis} radius={[4, 4, 0, 0]} maxBarSize={40} isAnimationActive={false} />
      </BarChart>
    </ResponsiveContainer>
  );
}

/** Colors for the product classes, in the backend's dues-first order. */
const CLASS_COLORS = [...PALETTE.series, PALETTE.other];

/** Where the money comes in over time: received per product class, stacked per fiscal year,
    so a one-off gift year or steady dues growth stands out. Pivots the flat (fy, class) rows. */
export function ClassOverTimeChart({ rows }: { rows: FinancialYearClassRow[] }) {
  const fys = [...new Set(rows.map((r) => r.fy))].sort((a, b) => a - b);
  // First-seen order preserves the backend's dues-first class ordering.
  const classes = [...new Map(rows.map((r) => [r.key, r.label])).entries()];
  const data = fys.map((fy) => {
    const o: Record<string, number | string> = { fy: fyLabel(fy) };
    for (const [key, label] of classes) o[label] = rows.find((r) => r.fy === fy && r.key === key)?.received ?? 0;
    return o;
  });
  return (
    <ResponsiveContainer width="100%" height={280}>
      <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 8 }} barCategoryGap="30%">
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="fy" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} width={56} tickFormatter={(v: number) => fmtMoneyShort(v)} />
        <Tooltip contentStyle={tooltipStyle} formatter={(v, name) => [fmtMoney(Number(v)), name]} />
        <Legend wrapperStyle={{ fontSize: 12, fontFamily: "var(--font-body)" }} formatter={(value) => <span style={{ color: "var(--text-secondary)" }}>{value}</span>} />
        {classes.map(([key, label], i) => (
          <Bar key={key} dataKey={label} stackId="a" fill={CLASS_COLORS[i % CLASS_COLORS.length]} maxBarSize={48} isAnimationActive={false} stroke="#fff" strokeWidth={1}
            radius={i === classes.length - 1 ? [4, 4, 0, 0] : undefined} />
        ))}
      </BarChart>
    </ResponsiveContainer>
  );
}

/** Per-cohort value: average money received per household by join cohort in the latest
    complete year, so cohorts of different sizes compare directly. Two newest emphasized. */
export function CohortValueChart({ rows, emphasize }: { rows: FinancialCohortRow[]; emphasize: number[] }) {
  const data = rows.map((r) => ({
    fy: fyLabel(r.cohort),
    main: emphasize.includes(r.cohort) ? r.received_per_household : null,
    rest: emphasize.includes(r.cohort) ? null : r.received_per_household,
    total: r.received,
    households: r.households,
  }));
  return (
    <ResponsiveContainer width="100%" height={240}>
      <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 8 }} barCategoryGap="35%">
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="fy" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} width={56} tickFormatter={(v: number) => fmtMoneyShort(v)} />
        <Tooltip contentStyle={tooltipStyle} formatter={(v, _name, item: { payload?: { total?: number; households?: number } }) => [`${fmtMoney(Number(v))} per household · ${fmtMoney(item.payload?.total ?? 0)} from ${fmt(item.payload?.households ?? 0)}`, "Received"]} />
        <Bar dataKey="rest" stackId="a" fill={PALETTE.deemphasis} radius={[4, 4, 0, 0]} maxBarSize={28} isAnimationActive={false} />
        <Bar dataKey="main" stackId="a" fill={PALETTE.emphasis} radius={[4, 4, 0, 0]} maxBarSize={28} isAnimationActive={false} />
      </BarChart>
    </ResponsiveContainer>
  );
}

/** Each fine reason's coarse family. The tenure chart colors by family (four
    accessible hues); the specific reason is carried by the table beneath it. */
const EXIT_FAMILY: Record<string, string> = {
  "Non-payment": "Addressable churn",
  "Financial hardship": "Addressable churn",
  "No longer engaged": "Addressable churn",
  "Displeased": "Addressable churn",
  "Joined another synagogue": "Addressable churn",
  "Aged out": "Conversion loss",
  "Introductory tier ended": "Conversion loss",
  "Moved": "Structural exit",
  "Elderly / ill": "Structural exit",
  "Other / not actionable": "Other / not actionable",
};
const FAMILY_ORDER = ["Addressable churn", "Conversion loss", "Structural exit", "Other / not actionable"];
// Validated 4-way categorical set (dataviz scripts/validate_palette.js): blue / amber /
// green clear the CVD floor with the legend + stacked-segment gaps as secondary
// encoding; the not-actionable tail uses the neutral de-emphasis hue.
const FAMILY_COLOR: Record<string, string> = {
  "Addressable churn": "#3b6eb8",
  "Conversion loss": "#d97706",
  "Structural exit": "#059669",
  "Other / not actionable": PALETTE.other,
};
const TENURE_ORDER = ["1-2y", "3-5y", "6-10y", "11+y"];

/** Exit composition by tenure at exit: a stacked count per tenure band, colored by
    the four reason families. The specific reason sits in the table beneath it. */
export function OutcomeByTenureChart({ rows }: { rows: OutcomeByTenureRow[] }) {
  const data = TENURE_ORDER.map((bucket) => {
    const row: Record<string, number | string> = { bucket };
    for (const family of FAMILY_ORDER) row[family] = 0;
    for (const r of rows.filter((r) => r.tenure_bucket === bucket)) {
      const family = EXIT_FAMILY[r.outcome] ?? "Other / not actionable";
      row[family] = (row[family] as number) + r.n;
    }
    return row;
  });
  return (
    <ResponsiveContainer width="100%" height={260}>
      <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 0 }} barCategoryGap="35%">
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="bucket" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} width={40} />
        <Tooltip contentStyle={tooltipStyle} />
        <Legend wrapperStyle={{ fontSize: 12, fontFamily: "var(--font-body)" }} formatter={(value) => <span style={{ color: "var(--text-secondary)" }}>{value}</span>} />
        {FAMILY_ORDER.map((family, i) => (
          <Bar key={family} dataKey={family} stackId="a" fill={FAMILY_COLOR[family]} maxBarSize={40} isAnimationActive={false} stroke="#fff" strokeWidth={1} radius={i === FAMILY_ORDER.length - 1 ? [4, 4, 0, 0] : undefined} />
        ))}
      </BarChart>
    </ResponsiveContainer>
  );
}

/** Dues renewal state per fiscal year: billed vs. coverage-missing among active households.
    Coverage-missing is unknown billing, never proven non-renewal. */
export function DuesChart({ rows }: { rows: DuesRow[] }) {
  const data = rows.map((r) => ({ fy: fyLabel(r.fy), Billed: r.billed, "Coverage missing": r.coverage_missing }));
  return (
    <ResponsiveContainer width="100%" height={240}>
      <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: 0 }} barCategoryGap="35%">
        <CartesianGrid vertical={false} stroke={PALETTE.grid} />
        <XAxis dataKey="fy" tick={axisTick} tickLine={false} axisLine={{ stroke: PALETTE.grid }} />
        <YAxis tick={axisTick} tickLine={false} axisLine={false} width={40} />
        <Tooltip contentStyle={tooltipStyle} />
        <Legend wrapperStyle={{ fontSize: 12, fontFamily: "var(--font-body)" }} formatter={(value) => <span style={{ color: "var(--text-secondary)" }}>{value}</span>} />
        <Bar dataKey="Billed" stackId="a" fill={PALETTE.series[4]} maxBarSize={28} isAnimationActive={false} stroke="#fff" strokeWidth={1} />
        <Bar dataKey="Coverage missing" stackId="a" fill={PALETTE.other} maxBarSize={28} radius={[4, 4, 0, 0]} isAnimationActive={false} stroke="#fff" strokeWidth={1} />
      </BarChart>
    </ResponsiveContainer>
  );
}

/** Total resignations per fine reason across the shown years, ranked high to low.
    Reason identity is carried by the axis label, so every reason reads cleanly
    without relying on color; the per-year split lives in the table beside it. */
export function rankedReasons(cells: ReasonCell[]) {
  const totals = new Map<string, number>();
  for (const cell of cells) totals.set(cell.reason, (totals.get(cell.reason) ?? 0) + cell.n);
  return [...totals.entries()]
    .map(([reason, n]) => ({ reason, n }))
    .sort((a, b) => b.n - a.n || a.reason.localeCompare(b.reason));
}

/** Reasons over time: rows are the specific reasons (most common on top), columns
    are fiscal years, and cell shade is the household count on a single-hue ramp
    (darker = more). The count sits in each cell so identity and magnitude never
    rely on color alone. */
export function ReasonsHeatmap({ cells }: { cells: ReasonCell[] }) {
  const reasons = rankedReasons(cells);
  const years = [...new Set(cells.map((c) => c.fy))].sort((a, b) => a - b);
  if (reasons.length === 0 || years.length === 0) return null;
  const max = Math.max(1, ...cells.map((c) => c.n));
  const at = (reason: string, fy: number) => cells.find((c) => c.reason === reason && c.fy === fy)?.n ?? 0;
  const cell = { height: 30, borderRadius: "var(--radius-sm)", display: "flex", alignItems: "center", justifyContent: "center", fontSize: "var(--text-xs)", fontVariantNumeric: "tabular-nums" } as const;
  return (
    <div>
      <div style={{ display: "grid", gridTemplateColumns: `180px repeat(${years.length}, 1fr)`, gap: 2 }}>
        <div />
        {years.map((fy) => <div key={fy} style={{ textAlign: "center", fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>{fyLabel(fy)}</div>)}
        {reasons.map(({ reason, n }) => (
          <Fragment key={reason}>
            <div style={{ alignSelf: "center", textAlign: "right", paddingRight: 8, fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
              {reason} <span style={{ color: "var(--text-tertiary)" }}>({fmt(n)})</span>
            </div>
            {years.map((fy) => {
              const v = at(reason, fy);
              if (v === 0) return <div key={`${reason}-${fy}`} style={cell} />;
              const step = Math.round((v / max) * 6);
              return (
                <div key={`${reason}-${fy}`} title={`${reason} · ${fyLabel(fy)} · ${fmt(v)} household${v === 1 ? "" : "s"}`}
                  style={{ ...cell, background: PALETTE.ramp[step], color: step >= 4 ? "#ffffff" : "var(--text-primary)" }}>
                  {fmt(v)}
                </div>
              );
            })}
          </Fragment>
        ))}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 8, fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>
        <span>0</span>
        <div style={{ width: 160, height: 8, borderRadius: 4, background: `linear-gradient(90deg, ${PALETTE.ramp[0]}, ${PALETTE.ramp[6]})` }} />
        <span>{fmt(max)}</span>
      </div>
    </div>
  );
}
