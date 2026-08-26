// @vitest-environment jsdom
import type React from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import type { PageProps } from "../App";
import * as api from "../api";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

// Stub the charts module so jsdom never renders recharts; keep a TableView that
// renders each cell so column content (e.g. household names) is assertable.
vi.mock("./insights/charts", () => {
  const N = () => null;
  type Col = { key: string; render: (r: unknown) => React.ReactNode };
  return {
    TrendChart: N, FlowsChart: N, Year1Chart: N, CohortHeatmap: N,
    HBarChart: N, ReasonsChart: N, OutcomeByTenureChart: N, DuesChart: N,
    TableView: ({ rows, columns, getRowKey }: { rows: unknown[]; columns: Col[]; getRowKey: (r: unknown) => string }) => (
      <div data-testid="table">
        {rows.map((r) => (
          <div key={getRowKey(r)}>{columns.map((c) => <span key={c.key}>{c.render(r)}</span>)}</div>
        ))}
      </div>
    ),
  };
});

import InsightsPage from "./InsightsPage";

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
  outcome_by_tenure: [{ tenure_bucket: "1-2y", outcome: "Addressable Churn", n: 5 }],
  school_progression: [{ group: "Nursery → Religious school", n: 8, still_members: 6, pct: 75 }],
  school_gap: [{ bucket: "0-1y", n: 4, still_members: 3, pct: 75 }],
  dues: [], anchor_type: [], anchor_count: [],
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

function mockInvoke(over: Partial<Record<string, unknown>> = {}) {
  invoke.mockImplementation((cmd: string) => {
    const table: Record<string, unknown> = {
      get_insights: fakeInsights, get_risk_summary: fakeRisk,
      get_watch_list: fakeWatch, export_watch_list_csv: "C:/exports/watch.csv", ...over,
    };
    return Promise.resolve(cmd in table ? table[cmd] : undefined);
  });
}

describe("InsightsPage", () => {
  beforeEach(() => { invoke.mockReset(); mockInvoke(); });
  afterEach(() => cleanup());

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
});
