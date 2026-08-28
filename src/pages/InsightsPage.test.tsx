// @vitest-environment jsdom
import type React from "react";
import { StrictMode } from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup, act } from "@testing-library/react";
import type { PageProps } from "../App";
import * as api from "../api";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
// `listen` returns a Promise<UnlistenFn>; mirror that, and capture callbacks per event name so
// tests can emit backend events (`emit("insights:progress", payload)`).
const listeners = vi.hoisted(() => new Map<string, Set<(e: { payload: unknown }) => void>>());
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, cb: (e: { payload: unknown }) => void) => {
    if (!listeners.has(name)) listeners.set(name, new Set());
    listeners.get(name)!.add(cb);
    return Promise.resolve(() => { listeners.get(name)?.delete(cb); });
  }),
}));
function emit(name: string, payload: unknown) {
  act(() => { listeners.get(name)?.forEach((cb) => cb({ payload })); });
}

// Stub the charts module so jsdom never renders recharts; keep a TableView that
// renders each cell so column content (e.g. household names) is assertable.
vi.mock("./insights/charts", () => {
  const N = () => <div className="recharts-responsive-container" data-insights-chart />;
  type Col = { key: string; render: (r: unknown) => React.ReactNode };
  return {
    TrendChart: N, FlowsChart: N, Year1Chart: N, CohortHeatmap: N, CohortMakeupChart: N,
    HBarChart: N, ReasonsHeatmap: N, OutcomeByTenureChart: N, DuesChart: N,
    ConcentrationChart: N, RevenueMixChart: N,
    TableView: ({ rows, columns, getRowKey }: { rows: unknown[]; columns: Col[]; getRowKey: (r: unknown) => string }) => (
      <div data-testid="table">
        {rows.map((r) => (
          <div key={getRowKey(r)}>{columns.map((c) => <span key={c.key}>{c.render(r)}</span>)}</div>
        ))}
      </div>
    ),
  };
});

// The neighborhood map is a maplibre-gl surface that cannot mount in jsdom (no WebGL); stub it
// so the Retention-mode tests exercise the ZIP-level heatmap that renders beside it.
vi.mock("./insights/NeighborhoodRetentionMap", () => ({
  NeighborhoodRetentionMap: () => <div data-testid="neighborhood-map" />,
}));

import InsightsPage, { _resetInsightsSnapshot } from "./InsightsPage";
import { _resetGeoCache } from "./insights/ZipGeographyMap";

const cap = (key: string, available: boolean): api.SourceCapability => ({
  key, available, required_objects: [], mirrored_columns: available ? ["A"] : [],
  last_synced_at: null, unavailable_reason: available ? null : `Select and sync ${key}`,
});

const fakeInsights: api.Insights = {
  built_at: "2026-08-01T00:00:00Z", newest_source_sync_at: "2026-08-01T00:00:00Z", stale: false,
  capabilities: [cap("membership", true), cap("renewal", false), cap("school", false), cap("committee", false)],
  current_fy: 2027, unavailable: [],
  kpis: { members_now: 100, net_vs_prior_fy: -2, joins_this_fy: 5, resigns_this_fy: 7, year1_cohort: 2025, year1_pct: 70, year1_baseline_pct: 80, at_risk_count: 3 },
  trend: [{ fy: 2025, joins: 5, resigns: 4, active_end_of_fy: 100 }],
  year1: [{ cohort: 2024, n: 10, pct_retained: 70 }],
  cohort_matrix: [{ cohort: 2020, n: 10, k: 5, pct_retained: 60 }],
  cohort_makeup: [{ cohort: 2020, current: 6, pct_of_base: 6 }, { cohort: 2026, current: 4, pct_of_base: 4 }],
  channels: [{ key: "clergy", label: "Clergy", n: 20, still_members: 14, pct: 70, avg_tenure: 5, left_within_2y: 2 }],
  school: [{ group: "No school history", n: 50, still_members: 20, pct: 40 }],
  reasons: [{ fy: 2026, reason: "Non-payment", n: 10 }],
  multi_job: [{ bucket: "1 job", jobs: 1, n: 30, still_members: 20, pct: 66.7, avg_tenure: 6 }],
  outcome_by_tenure: [{ tenure_bucket: "1-2y", outcome: "No longer engaged", n: 5 }],
  school_progression: [{ group: "Nursery → Religious school", n: 8, still_members: 6, pct: 75 }],
  school_gap: [{ bucket: "0-1y", n: 4, still_members: 3, pct: 75 }],
  dues: [], anchor_type: [], anchor_count: [], geography: null, financials: null,
};

// A ZIP-geography payload for the on-demand `zip_geography` command.
const geo = (over: Partial<api.ZipGeography> = {}): api.ZipGeography => ({
  fiscal_year: 2026, mode: "density", segment: null, available: true,
  cells: [
    { zip: "10024", measure: 42, n: 42, joins: 6, exits: 2, retained: 30 },
    { zip: "10025", measure: 18, n: 18, joins: 3, exits: 1, retained: 12 },
  ],
  out_of_area: 7, suppressed_zips: 2,
  options: {
    join_fys: [2026, 2025, 2024], tiers: ["Household", "Other"], categories: ["Voting"],
    channels: [{ key: "religious_school", label: "Religious School" }],
    school: [{ key: "active_religious_school", label: "Active religious school" }],
  },
  ...over,
});
/** The batch command echoes one view per requested year, in request order, like the backend. */
const echoGeoYears = (a: unknown) => {
  const args = a as { mode: api.GeoMode; fiscalYears: number[]; segment: api.Segment | null };
  return args.fiscalYears.map((fy) => geo({ mode: args.mode, fiscal_year: fy, segment: args.segment }));
};
const fakeRisk: api.RiskSummary = {
  available: true, unavailable_reason: null, roc_auc: 0.72, top_decile_lift: 2.4, brier: 0.12, baseline_brier: 0.2,
  years: [{ test_fy: 2023, households: 250, exits: 30, sufficient: true }],
  coverage: [], removed_families: ["renewal", "school", "committee"], model_first_fy: 2021, model_last_fy: 2023, watch_list_count: 2,
};
const fakeWatch: api.WatchListView = {
  available: true, unavailable_reason: null, model_first_fy: 2021, model_last_fy: 2023, baseline_rate: 0.1, confidence: 0.72,
  rows: [{ account_id: "a", name: "Cohen Family", score: 0.9, evidence: [{ class: "recent_religious_school_end", detail: "x" }, { class: "intro_tier_aging", detail: "y" }] }],
};

const props = { status: { synced_rows: 100 } as unknown } as PageProps;

/** A command whose promise never settles, for asserting in-flight UI. */
const pending = () => new Promise<never>(() => {});
const rebuildEvent = (over: Partial<api.InsightsProgress> = {}): api.InsightsProgress => ({
  job: "rebuild", phase: "Reading membership records", step: 1, steps: 5, done: null, total: null, elapsed_ms: 0, ...over,
});

// Table values may be factories: they are called per invocation, so a rejected or
// never-settling promise is created lazily and only when that command is actually called.
function mockInvoke(over: Partial<Record<string, unknown>> = {}) {
  invoke.mockImplementation((cmd: string, args?: unknown) => {
    const table: Record<string, unknown> = {
      get_insights: fakeInsights, get_risk_summary: fakeRisk, get_insights_job: null,
      get_watch_list: fakeWatch, export_watch_list_csv: "C:/exports/watch.csv", ...over,
    };
    const v = cmd in table ? table[cmd] : undefined;
    return Promise.resolve(typeof v === "function" ? (v as (a: unknown) => unknown)(args) : v);
  });
}

describe("InsightsPage", () => {
  beforeEach(() => { invoke.mockReset(); mockInvoke(); listeners.clear(); _resetInsightsSnapshot(); _resetGeoCache(); });
  afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); });

  it("loads aggregates and switches the visible tab section", async () => {
    render(<InsightsPage {...props} />);
    await screen.findByText("Membership over time");
    // Every section is in the DOM; the inactive ones carry the hidden class.
    const jobs = screen.getByText("Stickiness by Entry Job").closest(".insights-section")!;
    expect(jobs.className).toContain("insights-section-hidden");
    fireEvent.click(screen.getByRole("button", { name: "Jobs" }));
    expect(jobs.className).not.toContain("insights-section-hidden");
  });

  it("renders the financials tab from aggregate figures when billing is available", async () => {
    const financials: api.Financials = {
      fiscal_year: 2025, households: 100, paying_households: 90,
      total_billed: 200000, total_received: 180000,
      by_class: [
        { key: "membership", label: "Dues", billed: 150000, received: 140000 },
        { key: "tuition", label: "Tuition", billed: 50000, received: 40000 },
      ],
      concentration: Array.from({ length: 10 }, (_, i) => ({
        decile: i + 1, households: 10, billed_share: 10, received_share: 10,
        cumulative_billed_share: (i + 1) * 10, cumulative_received_share: (i + 1) * 10,
      })),
    };
    mockInvoke({ get_insights: { ...fakeInsights, capabilities: [cap("membership", true), cap("renewal", true), cap("school", false), cap("committee", false)], financials } });
    render(<InsightsPage {...props} />);
    await screen.findByText("Membership over time");
    fireEvent.click(screen.getByRole("button", { name: "Financials" }));
    // All three panels render, and the collection figures read off the totals.
    expect(screen.getByText("Who carries the dues base")).toBeTruthy();
    expect(screen.getByText("Where the money comes in")).toBeTruthy();
    expect(screen.getByText("Collection: billed vs received")).toBeTruthy();
    expect(screen.getByText("$180,000")).toBeTruthy(); // total received
    expect(screen.getByText("$20,000")).toBeTruthy();   // outstanding = billed - received
  });

  it("shows an unavailable state when an optional source is not synced", async () => {
    render(<InsightsPage {...props} />);
    await screen.findByText("Membership over time");
    // Renewal source is unavailable, so the dues card renders the not-available state.
    expect(screen.getAllByText("Not available").length).toBeGreaterThan(0);
  });

  it("loads household names only on explicit request and audits nothing before", async () => {
    render(<InsightsPage {...props} />);
    await screen.findByText("Membership over time");
    expect(invoke.mock.calls.some(([c]) => c === "get_watch_list")).toBe(false);
    fireEvent.click(await screen.findByRole("button", { name: /Load named Watch List/ }));
    await waitFor(() => expect(invoke.mock.calls.some(([c]) => c === "get_watch_list")).toBe(true));
    await screen.findByText("Cohen Family");
  });

  it("keeps named households out of the report and aggregates in it", async () => {
    render(<InsightsPage {...props} />);
    await screen.findByText("Membership over time");
    // The named Watch List card is screen-only and never enters the PDF.
    expect(screen.getByText("Named Watch List").closest(".insights-screen-only")).not.toBeNull();
    expect(screen.getByText("Named Watch List").closest(".insights-report-card")).toBeNull();
    // The aggregate risk card is a report card that composes into the PDF.
    expect(screen.getByText("Validated churn risk").closest(".insights-report-card")).not.toBeNull();
  });

  it("suppresses the ranking when validation fails", async () => {
    mockInvoke({ get_risk_summary: { ...fakeRisk, available: false, unavailable_reason: "ROC-AUC 0.51 below 0.65", watch_list_count: 0 } });
    render(<InsightsPage {...props} />);
    await screen.findByText("Membership over time");
    expect(screen.getByText(/No validated household ranking/)).toBeTruthy();
    // No load button when there is no validated ranking.
    expect(screen.queryByRole("button", { name: /Load named Watch List/ })).toBeNull();
  });

  it("shows a phase checklist with counts and elapsed time while a rebuild runs", async () => {
    vi.useFakeTimers();
    mockInvoke({ get_insights: pending });
    render(<InsightsPage {...props} />);
    expect(screen.getByText("Loading insights")).toBeTruthy();
    emit("insights:progress", rebuildEvent({ done: 4000, total: 13030 }));
    expect(screen.getByText("Building insights")).toBeTruthy();
    expect(screen.getByText(/4,000 of 13,030/)).toBeTruthy();
    expect(screen.getByText(/Step 1 of 5/)).toBeTruthy();
    // Every phase is listed; the first is current, the rest pending.
    expect(screen.getByText("Reading membership records").closest("li")!.getAttribute("data-state")).toBe("current");
    expect(screen.getByText("Finalizing").closest("li")!.getAttribute("data-state")).toBe("pending");
    act(() => { vi.advanceTimersByTime(61_000); });
    expect(screen.getByText(/Step 1 of 5 · 1:01/)).toBeTruthy();
    // No promised durations, just facts.
    expect(screen.queryByText(/minute/)).toBeNull();
  });

  it("keeps the previous build on screen and shows an inline rebuild banner on revisit", async () => {
    const { unmount } = render(<InsightsPage {...props} />);
    await screen.findByText("Membership over time");
    unmount();
    // Revisit: the backend is rebuilding after a sync, so get_insights hangs and emits progress.
    mockInvoke({ get_insights: pending });
    render(<InsightsPage {...props} />);
    expect(screen.getByText("Membership over time")).toBeTruthy();
    expect(screen.queryByText(/Rebuilding insights from the latest sync/)).toBeNull();
    emit("insights:progress", rebuildEvent({ phase: "Building yearly membership history", step: 2, done: 2019, total: 2027 }));
    expect(screen.getByText("Membership over time")).toBeTruthy();
    expect(screen.getByText(/Rebuilding insights from the latest sync/)).toBeTruthy();
    expect(screen.getByText(/Building yearly membership history/)).toBeTruthy();
    expect(screen.queryByText("Building insights")).toBeNull();
    // The stale-copy lede never claims a manual rebuild is needed while one is running.
    expect(screen.queryByText(/Rebuild Insights to refresh/)).toBeNull();
  });

  it("resumes live progress on mount when get_insights_job reports a running rebuild", async () => {
    mockInvoke({
      get_insights: pending,
      get_insights_job: rebuildEvent({ phase: "Applying engagement sources", step: 3, elapsed_ms: 120_000 }),
    });
    render(<InsightsPage {...props} />);
    await screen.findByText(/Step 3 of 5/);
    expect(screen.getByText(/Step 3 of 5 · 2:0\d/)).toBeTruthy();
    expect(screen.getByText("Applying engagement sources").closest("li")!.getAttribute("data-state")).toBe("current");
    expect(screen.getByText("Reading membership records").closest("li")!.getAttribute("data-state")).toBe("done");
  });

  it("shows risk analysis step progress in the Risk tab and header until the summary arrives", async () => {
    let resolveRisk!: (r: api.RiskSummary) => void;
    mockInvoke({ get_risk_summary: () => new Promise<api.RiskSummary>((res) => { resolveRisk = res; }) });
    render(<InsightsPage {...props} />);
    await screen.findByText("Membership over time");
    fireEvent.click(screen.getByRole("button", { name: "Risk" }));
    expect(screen.getByText(/Analyzing churn risk/)).toBeTruthy();
    emit("insights:progress", { job: "risk", phase: "Rolling validation", step: 2, steps: 4, done: 3, total: 14, elapsed_ms: 5000 });
    expect(screen.getByText(/Risk analysis: step 2 of 4/)).toBeTruthy();
    expect(screen.getByText(/Step 2 of 4/)).toBeTruthy();
    expect(screen.getByText("Rolling validation").closest("li")!.getAttribute("data-state")).toBe("current");
    expect(screen.getByText(/3 of 14/)).toBeTruthy();
    await act(async () => { resolveRisk(fakeRisk); });
    await screen.findByText("ROC-AUC");
    expect(screen.queryByText(/Risk analysis/)).toBeNull();
    expect(screen.queryByText(/Step 2 of 4/)).toBeNull();
    expect(document.querySelector(".app-spinner")).toBeNull();
  });

  it("shows an error state with retry instead of a spinner when insights fail to load", async () => {
    let calls = 0;
    mockInvoke({ get_insights: () => (++calls === 1 ? Promise.reject(new Error("mirror locked")) : Promise.resolve(fakeInsights)) });
    render(<InsightsPage {...props} />);
    await screen.findByText("Insights could not load");
    expect(screen.getByText(/mirror locked/)).toBeTruthy();
    expect(screen.queryByText("Loading insights")).toBeNull();
    expect(document.querySelector(".app-spinner")).toBeNull();
    expect(document.querySelector(".app-progress")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await screen.findByText("Membership over time");
    expect(invoke.mock.calls.filter(([c]) => c === "get_insights").length).toBe(2);
  });

  it("gives an executive summary of ZIP geography by mode and fiscal year", async () => {
    // The mock echoes the request (mode / fy / segment) like the real backend, so the
    // component's freshness gate — which prevents rendering one mode's cells under another
    // mode's view — is actually exercised.
    const echoGeo = (a: unknown) => {
      const args = a as { mode: api.GeoMode; fiscalYear: number; segment: api.Segment | null };
      return geo({ mode: args.mode, fiscal_year: args.fiscalYear, segment: args.segment });
    };
    mockInvoke({
      get_insights: { ...fakeInsights, capabilities: [...fakeInsights.capabilities, cap("geography", true)] },
      zip_geography: echoGeo,
      zip_geography_years: echoGeoYears,
    });
    render(<InsightsPage {...props} />);
    await screen.findByText("Membership geography");
    // Default view leads with "Where members are" for the last COMPLETED fiscal year
    // (currentFy 2027 is only weeks in, so the default is FY2026).
    expect(await screen.findByRole("region", { name: "Where members are — FY2026" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Where members are" }).getAttribute("aria-pressed")).toBe("true");
    await waitFor(() => expect(invoke.mock.calls.some(([c, a]) => c === "zip_geography" && (a as { mode: string })?.mode === "density" && (a as { fiscalYear: number })?.fiscalYear === 2026)).toBe(true));

    // Headline stats: total mapped households, the top ZIP, and its concentration share.
    // geo(): 10024 n=42, 10025 n=18 → 60 total, top ZIP 10024 at 70%.
    expect(await screen.findByText("Mapped households")).toBeTruthy();
    expect(screen.getAllByText("60").length).toBeGreaterThan(0);
    expect(screen.getByText(/70% of total/)).toBeTruthy();
    // Out-of-area members surface in the summary, never silently dropped.
    expect(screen.getByText(/7 outside NY/)).toBeTruthy();
    // Suppression is stated (count floor 5).
    expect(screen.getByText(/2 smaller ZIPs are hidden.*fewer than 5 in this view/)).toBeTruthy();
    expect(screen.getByTestId("zip-geography-table")).toBeTruthy();

    // Retention by area: a cohort × area heatmap across eight join-year cohorts, fetched in
    // ONE backend call — every command queues on one store lock, so eight calls would be
    // eight waits in a row.
    fireEvent.click(screen.getByRole("button", { name: "Retention by area" }));
    expect(await screen.findByRole("region", { name: "Retention by area — cohort trend" })).toBeTruthy();
    expect(await screen.findByTestId("zip-retention-table")).toBeTruthy();
    const retentionCalls = invoke.mock.calls.filter(([c, a]) => c === "zip_geography_years" && (a as { mode: string })?.mode === "retention");
    expect(retentionCalls.length).toBe(1);
    expect((retentionCalls[0][1] as { fiscalYears: number[] }).fiscalYears).toEqual([2026, 2025, 2024, 2023, 2022, 2021, 2020, 2019]);
    expect(invoke.mock.calls.some(([c, a]) => c === "zip_geography" && (a as { mode: string })?.mode === "retention")).toBe(false);
    expect(screen.getAllByText("10024").length).toBeGreaterThan(0);

    // Growth & decline splits gainers from losers.
    fireEvent.click(screen.getByRole("button", { name: "Growth & decline" }));
    expect(await screen.findByRole("region", { name: "Growth & decline — FY2026" })).toBeTruthy();
    await waitFor(() => expect(invoke.mock.calls.some(([c, a]) => c === "zip_geography" && (a as { mode: string })?.mode === "net_change")).toBe(true));
    expect(await screen.findByText("Losing ground")).toBeTruthy();
    expect(screen.getByText("Gaining members")).toBeTruthy();

    // Attrition tightens the suppression floor to 10.
    fireEvent.click(screen.getByRole("button", { name: "Attrition" }));
    expect(await screen.findByRole("region", { name: "Attrition — FY2026" })).toBeTruthy();
    await waitFor(() => expect(invoke.mock.calls.some(([c, a]) => c === "zip_geography" && (a as { mode: string })?.mode === "attrition")).toBe(true));
    expect(await screen.findByText(/fewer than 10 in this view/)).toBeTruthy();

    // The fiscal-year selector drives the time-varying views.
    fireEvent.change(screen.getByRole("combobox", { name: "Fiscal year" }), { target: { value: "2025" } });
    expect(await screen.findByRole("region", { name: "Attrition — FY2025" })).toBeTruthy();
    await waitFor(() => expect(invoke.mock.calls.some(([c, a]) => c === "zip_geography" && (a as { mode: string })?.mode === "attrition" && (a as { fiscalYear: number })?.fiscalYear === 2025)).toBe(true));
  });

  it("serves a revisited geography view from the session cache instead of refetching", async () => {
    const echoGeo = (a: unknown) => {
      const args = a as { mode: api.GeoMode; fiscalYear: number; segment: api.Segment | null };
      return geo({ mode: args.mode, fiscal_year: args.fiscalYear, segment: args.segment });
    };
    mockInvoke({
      get_insights: { ...fakeInsights, capabilities: [...fakeInsights.capabilities, cap("geography", true)] },
      zip_geography: echoGeo,
    });
    render(<InsightsPage {...props} />);
    await screen.findByRole("region", { name: "Where members are — FY2026" });
    const densityCalls = () => invoke.mock.calls.filter(([c, a]) =>
      c === "zip_geography" && (a as { mode: string })?.mode === "density" && (a as { fiscalYear: number })?.fiscalYear === 2026).length;
    await waitFor(() => expect(densityCalls()).toBe(1));

    // Leave the default view, then come back to the exact same selection.
    fireEvent.click(screen.getByRole("button", { name: "Attrition" }));
    await screen.findByRole("region", { name: "Attrition — FY2026" });
    fireEvent.click(screen.getByRole("button", { name: "Where members are" }));
    await screen.findByRole("region", { name: "Where members are — FY2026" });

    // The revisit paints from cache — the backend was never asked a second time for it.
    expect(densityCalls()).toBe(1);
  });

  it("asks the backend once per view even when StrictMode double-runs the mount effects", async () => {
    // Dev-mode StrictMode mounts effects twice. Every geography view queues behind one store
    // lock on the backend, so a duplicated request is a real, user-visible wait — the page
    // must coalesce concurrent identical requests, not just cache settled ones.
    const echoGeo = (a: unknown) => {
      const args = a as { mode: api.GeoMode; fiscalYear: number; segment: api.Segment | null };
      return geo({ mode: args.mode, fiscal_year: args.fiscalYear, segment: args.segment });
    };
    mockInvoke({
      get_insights: { ...fakeInsights, capabilities: [...fakeInsights.capabilities, cap("geography", true)] },
      zip_geography: echoGeo,
    });
    render(<StrictMode><InsightsPage {...props} /></StrictMode>);
    await screen.findByRole("region", { name: "Where members are — FY2026" });
    await screen.findByText("Mapped households");
    expect(invoke.mock.calls.filter(([c]) => c === "get_insights").length).toBe(1);
    expect(invoke.mock.calls.filter(([c]) => c === "zip_geography").length).toBe(1);
  });

  it("says what it is loading instead of a bare Loading…", async () => {
    mockInvoke({
      get_insights: { ...fakeInsights, capabilities: [...fakeInsights.capabilities, cap("geography", true)] },
      zip_geography: pending,
      zip_geography_years: pending,
    });
    render(<InsightsPage {...props} />);
    await screen.findByRole("region", { name: "Where members are — FY2026" });
    expect(screen.getByText("Loading member households by ZIP for FY2026…")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "New members" }));
    expect(await screen.findByText("Loading new members by ZIP for FY2026…")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Retention by area" }));
    expect(await screen.findByText("Computing retention by ZIP for the FY2019–FY2026 cohorts…")).toBeTruthy();
    expect(screen.queryByText("Loading…")).toBeNull();
  });

  it("paints the default geography view from the insights payload without a standalone request", async () => {
    // zip_geography never settles here: if the panel fetched the default itself, it would
    // hang at "Loading…" (the real-world symptom of queuing behind the risk analysis).
    mockInvoke({
      get_insights: { ...fakeInsights, capabilities: [...fakeInsights.capabilities, cap("geography", true)], geography: geo() },
      zip_geography: pending,
    });
    render(<InsightsPage {...props} />);
    // The default view is on screen straight from the payload — no separate zip_geography call.
    expect(await screen.findByRole("region", { name: "Where members are — FY2026" })).toBeTruthy();
    expect(screen.getAllByText("60").length).toBeGreaterThan(0); // 42 + 18 mapped households
    expect(invoke.mock.calls.some(([c]) => c === "zip_geography")).toBe(false);
  });

  it("shows the geographic-unavailable state and fetches no geography when no postal source is mirrored", async () => {
    mockInvoke({ get_insights: { ...fakeInsights, capabilities: [...fakeInsights.capabilities, cap("geography", false)] } });
    render(<InsightsPage {...props} />);
    expect(await screen.findByText(/Geographic membership insights are unavailable/)).toBeTruthy();
    expect(invoke.mock.calls.some(([c]) => c === "zip_geography")).toBe(false);
  });

  it.each(["Overview", "Jobs", "Renewal & Engagement", "Risk"])("lays out the aggregate report before exporting from %s", async (tab) => {
    const rect = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 800, height: 280 } as DOMRect);
    mockInvoke({ export_insights_pdf: pending });
    render(<InsightsPage {...props} />);
    await screen.findByText("Membership over time");
    fireEvent.click(screen.getByRole("button", { name: tab }));
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Download PDF report" }));

    const surface = await screen.findByTestId("insights-pdf-surface");
    expect(surface.contains(screen.getByText("Named Watch List").closest(".insights-screen-only"))).toBe(false);
    expect(invoke.mock.calls.some(([command]) => command === "export_insights_pdf")).toBe(false);
    await waitFor(() => expect(invoke.mock.calls.some(([command]) => command === "export_insights_pdf")).toBe(true));
    rect.mockRestore();
  });

  it("reports an unready report surface without invoking the native PDF command", async () => {
    render(<InsightsPage {...props} />);
    await screen.findByText("Membership over time");
    vi.useFakeTimers();
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 0, height: 0 } as DOMRect);
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Download PDF report" }));
    await act(async () => { await vi.advanceTimersByTimeAsync(3_100); });
    expect(screen.getByText(/PDF could not be rendered/)).toBeTruthy();
    expect(invoke.mock.calls.some(([command]) => command === "export_insights_pdf")).toBe(false);
  });

  it("surfaces a native PDF rendering failure without reporting an export path", async () => {
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 800, height: 280 } as DOMRect);
    mockInvoke({ export_insights_pdf: () => Promise.reject(new Error("webview print failed")) });
    render(<InsightsPage {...props} />);
    await screen.findByText("Membership over time");
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Download PDF report" }));

    await screen.findByText(/webview print failed/);
    expect(screen.queryByText(/Exported to/)).toBeNull();
  });
});
