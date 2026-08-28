import { describe, it, expect } from "vitest";
import { fyLabel, heatStep, heatInk, soWhat, RULE_LABELS, EVIDENCE_LABELS } from "./format";
import type { Insights } from "../../api";

const base: Insights = {
  built_at: "2026-08-25T20:00:00Z", newest_source_sync_at: "2026-08-25T20:00:00Z", stale: false,
  capabilities: [], current_fy: 2026, unavailable: [],
  kpis: { members_now: 2490, net_vs_prior_fy: -62, joins_this_fy: 244, resigns_this_fy: 306,
    year1_cohort: 2025, year1_pct: 66.7, year1_baseline_pct: 87.4, at_risk_count: 12 },
  trend: [{ fy: 2015, joins: 200, resigns: 100, active_end_of_fy: 2000 }, { fy: 2025, joins: 321, resigns: 328, active_end_of_fy: 2552 }, { fy: 2026, joins: 244, resigns: 306, active_end_of_fy: 2490 }],
  year1: [{ cohort: 2024, n: 374, pct_retained: 69 }, { cohort: 2025, n: 321, pct_retained: 66.7 }],
  cohort_matrix: [{ cohort: 2015, n: 185, k: 5, pct_retained: 69.7 }, { cohort: 2019, n: 188, k: 5, pct_retained: 48.9 }],
  cohort_makeup: [{ cohort: 2005, current: 50, pct_of_base: 2 }, { cohort: 2015, current: 120, pct_of_base: 4.8 }, { cohort: 2025, current: 214, pct_of_base: 8.6 }, { cohort: 2026, current: 217, pct_of_base: 8.7 }],
  membership_age: [
    { band: "New", households: 400, pct_of_base: 16 },
    { band: "Establishing", households: 500, pct_of_base: 20 },
    { band: "Settled", households: 600, pct_of_base: 24 },
    { band: "Long-standing", households: 700, pct_of_base: 28 },
    { band: "Legacy", households: 300, pct_of_base: 12 },
  ],
  membership_age_over_time: [
    { fy: 2016, band: "New", households: 300, pct_of_base: 30 },
    { fy: 2016, band: "Establishing", households: 200, pct_of_base: 20 },
    { fy: 2016, band: "Settled", households: 200, pct_of_base: 20 },
    { fy: 2016, band: "Long-standing", households: 200, pct_of_base: 20 },
    { fy: 2016, band: "Legacy", households: 100, pct_of_base: 10 },
    { fy: 2026, band: "New", households: 400, pct_of_base: 16 },
    { fy: 2026, band: "Establishing", households: 500, pct_of_base: 20 },
    { fy: 2026, band: "Settled", households: 600, pct_of_base: 24 },
    { fy: 2026, band: "Long-standing", households: 700, pct_of_base: 28 },
    { fy: 2026, band: "Legacy", households: 290, pct_of_base: 12 },
  ],
  channels: [{ key: "clergy", label: "Clergy", n: 76, still_members: 49, pct: 64.5, avg_tenure: 4.7, left_within_2y: 11 },
             { key: "nursery_school", label: "Nursery school", n: 122, still_members: 32, pct: 26.2, avg_tenure: 4.6, left_within_2y: 34 }],
  school: [{ group: "Both nursery and religious school", n: 98, still_members: 67, pct: 68.4 }, { group: "No school history", n: 1115, still_members: 443, pct: 39.7 }],
  reasons: [{ fy: 2026, reason: "Non-payment", n: 90 }, { fy: 2026, reason: "Moved", n: 51 }],
  multi_job: [], outcome_by_tenure: [], school_progression: [], school_gap: [],
  dues: [], anchor_type: [], anchor_count: [], geography: null,
  financials: {
    fiscal_year: 2025, households: 2490, paying_households: 2400,
    total_billed: 1000000, total_received: 900000,
    by_year: [], by_year_class: [],
    by_membership_age: [
      { band: "New", households: 400, received: 100000, share_of_households: 16, share_of_received: 11, received_per_household: 250 },
      { band: "Establishing", households: 500, received: 150000, share_of_households: 20, share_of_received: 17, received_per_household: 300 },
      { band: "Settled", households: 600, received: 200000, share_of_households: 24, share_of_received: 22, received_per_household: 333 },
      { band: "Long-standing", households: 700, received: 250000, share_of_households: 28, share_of_received: 28, received_per_household: 357 },
      { band: "Legacy", households: 300, received: 200000, share_of_households: 12, share_of_received: 22, received_per_household: 667 },
    ],
    by_growth: [
      { fy: 2015, complete: true, new_received: 100000, recurring_received: 400000 },
      { fy: 2025, complete: true, new_received: 90000, recurring_received: 810000 },
    ],
    concentration: [],
  },
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
    expect(s.trend).toContain("so far in FY2026");
    expect(s.channels).toContain("Clergy");
    expect(s.channels).toContain("Nursery school");
    expect(s.school).toContain("68.4%");
    expect(s.reasons).toContain("Non-payment");
    expect(s.cohort).toContain("FY2015");
    // Makeup by band: New (16%) + Establishing (20%) = 36% joined in the last five years;
    // Legacy is 12%; the largest band is Long-standing (700 households).
    expect(s.makeup).toContain("36%");
    expect(s.makeup).toContain("12% are Legacy");
    expect(s.makeup).toContain("Long-standing");
    expect(s.makeup).toContain("700");
    // Age mix over time: Legacy rose 10% → 12% and New fell 30% → 16% from FY2016 to FY2026,
    // stated as two literal endpoints (no straight-line "rising" claim).
    expect(s.ageShift).toContain("since FY2016");
    expect(s.ageShift).toContain("12% of members now versus 10%");
    expect(s.ageShift).toContain("16% versus 30%");
    // Survivors: FY2010→FY2025 joins are 2015(200)+2025(321)=521; survivors from those cohorts
    // are 2015(120)+2025(214)=334 (pre-2010 and the in-progress FY2026 are excluded) → 64.1%.
    expect(s.survivors).toContain("521");
    expect(s.survivors).toContain("334");
    expect(s.survivors).toContain("64.1%");
    expect(s.survivors).toContain("FY2010");
    expect(s.survivors).toContain("FY2025");
    // Financial value by age: Legacy carries the largest money-minus-households gap
    // (12% of households, 22% of the money).
    expect(s.financialAge).toContain("Legacy");
    expect(s.financialAge).toContain("12%");
    expect(s.financialAge).toContain("22%");
    // Growth vs recurring: latest year (FY2025) growth share is 10%, down from 20% in FY2015,
    // while the member base grew from 2,000 to 2,552 (27.6%) over the same years.
    expect(s.growthHealth).toContain("10%");
    expect(s.growthHealth).toContain("down from 20% in FY2015");
    expect(s.growthHealth).toContain("grew 27.6%");
  });

  it("reports growth direction honestly when the share rose then fell back", () => {
    // Growth share climbed to a peak (FY2025) then fell (FY2026): 2.6% → 5% → 3.3%. Comparing
    // only the first and last years would wrongly read "rising"; the sentence must show both the
    // recent drop (vs FY2025) and the longer rise (vs FY2023).
    const ins: Insights = {
      ...base,
      trend: [
        { fy: 2023, joins: 100, resigns: 50, active_end_of_fy: 2400 },
        { fy: 2026, joins: 120, resigns: 90, active_end_of_fy: 2470 },
      ],
      financials: {
        ...base.financials!,
        by_growth: [
          { fy: 2023, complete: true, new_received: 2600, recurring_received: 97400 },
          { fy: 2025, complete: true, new_received: 5000, recurring_received: 95000 },
          { fy: 2026, complete: true, new_received: 3300, recurring_received: 96700 },
        ],
      },
    };
    const s = soWhat(ins);
    expect(s.growthHealth).toContain("3.3%");
    expect(s.growthHealth).toContain("down from 5% in FY2025");
    expect(s.growthHealth).toContain("up from 2.6% in FY2023");
    expect(s.growthHealth).not.toContain("rising");
    expect(s.growthHealth).toContain("grew 2.9%");
  });

  it("has a label for every at-risk rule", () => {
    expect(Object.keys(RULE_LABELS).sort()).toEqual(["first_year", "intro_tier_aging", "new_ns_only", "rs_ended"]);
  });

  it("labels every Watch List evidence class without causal language", () => {
    for (const cls of ["recent_religious_school_end", "intro_tier_aging", "new_household", "lost_engagement_anchor"]) {
      const label = EVIDENCE_LABELS[cls];
      expect(label).toBeTruthy();
      expect(label.toLowerCase()).not.toMatch(/will|cause|because|predict/);
    }
  });
});
