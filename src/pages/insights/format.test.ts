import { describe, it, expect } from "vitest";
import { fyLabel, heatStep, heatInk, soWhat, RULE_LABELS } from "./format";
import type { Insights } from "../../api";

const base: Insights = {
  built_at: "2026-08-25T20:00:00Z", current_fy: 2026, unavailable: [],
  kpis: { members_now: 2490, net_vs_prior_fy: -62, joins_this_fy: 244, resigns_this_fy: 306,
    year1_cohort: 2025, year1_pct: 66.7, year1_baseline_pct: 87.4, at_risk_count: 12 },
  trend: [{ fy: 2025, joins: 321, resigns: 328, active_end_of_fy: 2552 }, { fy: 2026, joins: 244, resigns: 306, active_end_of_fy: 2490 }],
  year1: [{ cohort: 2024, n: 374, pct_retained: 69 }, { cohort: 2025, n: 321, pct_retained: 66.7 }],
  cohort_matrix: [{ cohort: 2015, n: 185, k: 5, pct_retained: 69.7 }, { cohort: 2019, n: 188, k: 5, pct_retained: 48.9 }],
  channels: [{ key: "clergy", label: "Clergy", n: 76, still_members: 49, pct: 64.5, avg_tenure: 4.7, left_within_2y: 11 },
             { key: "nursery_school", label: "Nursery school", n: 122, still_members: 32, pct: 26.2, avg_tenure: 4.6, left_within_2y: 34 }],
  school: [{ group: "Both nursery and religious school", n: 98, still_members: 67, pct: 68.4 }, { group: "No school history", n: 1115, still_members: 443, pct: 39.7 }],
  reasons: [{ fy: 2026, reason: "Non-payment", n: 90 }, { fy: 2026, reason: "Moved", n: 51 }],
};

describe("insights formatting", () => {
  it("labels fiscal years and steps the heat ramp", () => {
    expect(fyLabel(2025)).toBe("FY2025");
    expect(heatStep(0)).toBe(0);
    expect(heatStep(30)).toBe(0);
    expect(heatStep(60)).toBe(3);
    expect(heatStep(90)).toBe(6);
    expect(heatStep(100)).toBe(6);
    expect(heatInk(40)).toBe("var(--text-primary)");
    expect(heatInk(75)).toBe("#ffffff");
  });

  it("writes the so-what sentences from the numbers", () => {
    const s = soWhat(base);
    expect(s.year1).toContain("FY2025 cohort kept 66.7%");
    expect(s.year1).toContain("87.4%");
    expect(s.trend).toContain("2,490");
    expect(s.channels).toContain("Clergy");
    expect(s.channels).toContain("Nursery school");
    expect(s.school).toContain("68.4%");
    expect(s.reasons).toContain("Non-payment");
    expect(s.cohort).toContain("FY2015");
  });

  it("has a label for every at-risk rule", () => {
    expect(Object.keys(RULE_LABELS).sort()).toEqual(["first_year", "intro_tier_aging", "new_ns_only", "rs_ended"]);
  });
});
