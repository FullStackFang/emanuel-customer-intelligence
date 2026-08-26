import type { Insights } from "../../api";

export const fyLabel = (fy: number) => `FY${fy}`;
export const fmt = (n: number) => n.toLocaleString();

/** 7-step sequential ramp index for a retention percentage: 30% -> 0, 90% -> 6. */
export function heatStep(pct: number): number {
  const t = Math.max(0, Math.min(1, (pct - 30) / 60));
  return Math.round(t * 6);
}
/** Ink for a heat cell: white on the four darkest steps, primary text otherwise. */
export const heatInk = (pct: number) => (heatStep(pct) >= 4 ? "#ffffff" : "var(--text-primary)");

export const RULE_LABELS: Record<string, string> = {
  first_year: "First year",
  new_ns_only: "Nursery school only",
  intro_tier_aging: "Introductory tier aging out",
  rs_ended: "Religious school ended",
};

/** One plain sentence per card, built from the numbers so it never goes stale. */
export function soWhat(ins: Insights) {
  const last = ins.trend[ins.trend.length - 1];
  const prev = ins.trend[ins.trend.length - 2];
  const k = ins.kpis;
  const trend = last && prev
    ? `${fmt(last.active_end_of_fy)} member households at the end of ${fyLabel(last.fy)}, ${last.active_end_of_fy - prev.active_end_of_fy >= 0 ? "up" : "down"} ${fmt(Math.abs(last.active_end_of_fy - prev.active_end_of_fy))} on ${fyLabel(prev.fy)}; ${fmt(last.joins)} joined and ${fmt(last.resigns)} resigned.`
    : "Not enough history yet.";
  const year1 = `The ${fyLabel(k.year1_cohort)} cohort kept ${k.year1_pct}% of its households through the first year, against a ${k.year1_baseline_pct}% average for earlier cohorts.`;
  const five = ins.cohort_matrix.filter((c) => c.k === 5);
  const best = five.reduce<typeof five[number] | null>((a, c) => (!a || c.pct_retained > a.pct_retained ? c : a), null);
  const worst = five.reduce<typeof five[number] | null>((a, c) => (!a || c.pct_retained < a.pct_retained ? c : a), null);
  const cohort = best && worst
    ? `Five-year retention ranges from ${worst.pct_retained}% (${fyLabel(worst.cohort)} cohort) to ${best.pct_retained}% (${fyLabel(best.cohort)} cohort).`
    : "Five-year retention needs at least one cohort with five years of history.";
  const chTop = ins.channels[0];
  const chBottom = ins.channels[ins.channels.length - 1];
  const channels = chTop && chBottom
    ? `${chTop.label} joiners are the most durable (${chTop.pct}% still members); ${chBottom.label} joiners the least (${chBottom.pct}%).`
    : "Join-channel comparison needs at least 20 households per reason.";
  const schoolBest = [...ins.school].sort((a, b) => b.pct - a.pct)[0];
  const school = schoolBest
    ? `${schoolBest.group} households retain best at ${schoolBest.pct}%.`
    : "No school history available.";
  const latestFy = Math.max(...ins.reasons.map((r) => r.fy), 0);
  const topReason = ins.reasons.filter((r) => r.fy === latestFy).sort((a, b) => b.n - a.n)[0];
  const reasons = topReason
    ? `In ${fyLabel(latestFy)} the leading coded reason was ${topReason.reason} (${fmt(topReason.n)} households).`
    : "No coded resignation reasons yet.";
  return { trend, year1, cohort, channels, school, reasons };
}
