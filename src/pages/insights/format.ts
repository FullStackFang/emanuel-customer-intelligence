import type { Insights } from "../../api";

export const fyLabel = (fy: number) => `FY${fy}`;
export const fmt = (n: number) => n.toLocaleString();
/** Whole-dollar currency, e.g. $1,234,000. Financial figures are aggregate totals, so
 *  cents add noise; round to the dollar. */
export const fmtMoney = (n: number) => n.toLocaleString(undefined, { style: "currency", currency: "USD", maximumFractionDigits: 0 });
/** Compact currency for axis ticks: $1.2M, $340K, $920. Keeps a money axis narrow. */
export const fmtMoneyShort = (n: number) => {
  const abs = Math.abs(n);
  if (abs >= 1_000_000) return `$${(n / 1_000_000).toFixed(abs >= 10_000_000 ? 0 : 1)}M`;
  if (abs >= 1_000) return `$${Math.round(n / 1_000)}K`;
  return `$${Math.round(n)}`;
};

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

/** Watch List evidence classes, phrased as observations, never as causes or predictions. */
export const EVIDENCE_LABELS: Record<string, string> = {
  recent_religious_school_end: "Religious school recently ended",
  intro_tier_aging: "Introductory tier aging",
  new_household: "New household",
  lost_engagement_anchor: "Recent engagement drop",
};

/** One plain sentence per card, built from the numbers so it never goes stale. */
export function soWhat(ins: Insights) {
  const last = ins.trend[ins.trend.length - 1];
  const prev = ins.trend[ins.trend.length - 2];
  const k = ins.kpis;
  const trend = last && prev
    ? `${fmt(last.active_end_of_fy)} member households so far in ${fyLabel(last.fy)}, ${last.active_end_of_fy - prev.active_end_of_fy >= 0 ? "up" : "down"} ${fmt(Math.abs(last.active_end_of_fy - prev.active_end_of_fy))} on ${fyLabel(prev.fy)}; ${fmt(last.joins)} joined and ${fmt(last.resigns)} resigned to date.`
    : "Not enough history yet.";
  const year1 = `The ${fyLabel(k.year1_cohort)} cohort kept ${k.year1_pct}% of its households through the first year, against a ${k.year1_baseline_pct}% average for earlier cohorts.`;
  const five = ins.cohort_matrix.filter((c) => c.k === 5);
  const best = five.reduce<typeof five[number] | null>((a, c) => (!a || c.pct_retained > a.pct_retained ? c : a), null);
  const worst = five.reduce<typeof five[number] | null>((a, c) => (!a || c.pct_retained < a.pct_retained ? c : a), null);
  const cohort = best && worst
    ? `Five-year retention ranges from ${worst.pct_retained}% (${fyLabel(worst.cohort)} cohort) to ${best.pct_retained}% (${fyLabel(best.cohort)} cohort).`
    : "Five-year retention needs at least one cohort with five years of history.";
  const recentCut = ins.current_fy - 5;
  const recentShare = ins.cohort_makeup.filter((r) => r.cohort > recentCut).reduce((sum, r) => sum + r.pct_of_base, 0);
  const topCohort = [...ins.cohort_makeup].sort((a, b) => b.current - a.current)[0];
  const makeup = topCohort
    ? `The five most recent cohorts make up ${Math.round(recentShare * 10) / 10}% of today's members; the ${fyLabel(topCohort.cohort)} cohort alone contributes ${fmt(topCohort.current)} (${topCohort.pct_of_base}% of the base).`
    : "Not enough cohort history yet.";
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
  return { trend, year1, cohort, makeup, channels, school, reasons };
}
