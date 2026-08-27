// @vitest-environment jsdom
import type React from "react";
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
    TrendChart: N, FlowsChart: N, Year1Chart: N, CohortHeatmap: N,
    HBarChart: N, ReasonsHeatmap: N, OutcomeByTenureChart: N, DuesChart: N,
    TableView: ({ rows, columns, getRowKey }: { rows: unknown[]; columns: Col[]; getRowKey: (r: unknown) => string }) => (
      <div data-testid="table">
        {rows.map((r) => (
          <div key={getRowKey(r)}>{columns.map((c) => <span key={c.key}>{c.render(r)}</span>)}</div>
        ))}
      </div>
    ),
  };
});

import InsightsPage, { _resetInsightsSnapshot } from "./InsightsPage";

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
  channels: [{ key: "clergy", label: "Clergy", n: 20, still_members: 14, pct: 70, avg_tenure: 5, left_within_2y: 2 }],
  school: [{ group: "No school history", n: 50, still_members: 20, pct: 40 }],
  reasons: [{ fy: 2026, reason: "Non-payment", n: 10 }],
  multi_job: [{ bucket: "1 job", jobs: 1, n: 30, still_members: 20, pct: 66.7, avg_tenure: 6 }],
  outcome_by_tenure: [{ tenure_bucket: "1-2y", outcome: "No longer engaged", n: 5 }],
  school_progression: [{ group: "Nursery → Religious school", n: 8, still_members: 6, pct: 75 }],
  school_gap: [{ bucket: "0-1y", n: 4, still_members: 3, pct: 75 }],
  dues: [], anchor_type: [], anchor_count: [],
  zip_attrition: [],
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
  invoke.mockImplementation((cmd: string) => {
    const table: Record<string, unknown> = {
      get_insights: fakeInsights, get_risk_summary: fakeRisk, get_insights_job: null,
      get_watch_list: fakeWatch, export_watch_list_csv: "C:/exports/watch.csv", ...over,
    };
    const v = cmd in table ? table[cmd] : undefined;
    return Promise.resolve(typeof v === "function" ? v() : v);
  });
}

describe("InsightsPage", () => {
  beforeEach(() => { invoke.mockReset(); mockInvoke(); listeners.clear(); _resetInsightsSnapshot(); });
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

  it("shows aggregate ZIP attrition with a fiscal-year selector and an unavailable source state", async () => {
    mockInvoke({ get_insights: { ...fakeInsights, capabilities: [...fakeInsights.capabilities, cap("zip_attrition", true)], zip_attrition: [{ fy: 2026, zip: "10024", start_households: 8, exits: 2, attrition_rate: 25 }, { fy: 2026, zip: "02108", start_households: 8, exits: 1, attrition_rate: 12.5 }, { fy: 2025, zip: "10024", start_households: 8, exits: 1, attrition_rate: 12.5 }] } });
    render(<InsightsPage {...props} />);
    await screen.findByText("ZIP attrition");
    expect(screen.getByText(/latest linked billing statement ZIP, with an Account ZIP fallback/)).toBeTruthy();
    expect(screen.getByRole("combobox", { name: "Fiscal year" })).toBeTruthy();
    expect(screen.getByRole("img", { name: "New York ZIP attrition map for FY2026" })).toBeTruthy();
    expect(screen.getByLabelText("10024: 25% attrition; 2 exits from 8 starting households")).toBeTruthy();
    expect(screen.getByText("10024")).toBeTruthy();
    expect(screen.getByText("25%")).toBeTruthy();
    expect(screen.getByText(/1 eligible ZIP is outside New York/)).toBeTruthy();
    fireEvent.change(screen.getByRole("combobox", { name: "Fiscal year" }), { target: { value: "2025" } });
    expect(screen.getByRole("img", { name: "New York ZIP attrition map for FY2025" })).toBeTruthy();

    cleanup();
    mockInvoke({ get_insights: { ...fakeInsights, capabilities: [...fakeInsights.capabilities, cap("zip_attrition", false)] } });
    render(<InsightsPage {...props} />);
    expect(await screen.findByText(/ZIP attrition is unavailable/)).toBeTruthy();
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
