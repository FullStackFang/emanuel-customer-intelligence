import type { FinancialGrowthRow, Insights } from "../../api";

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

/** Human year-range for each membership-age band, shown beside the band name so "Settled",
 *  "Long-standing", etc. read plainly instead of as undefined jargon. Membership age counts
 *  fiscal years since the current membership spell began. Mirrors MEMBERSHIP_AGE_BANDS in
 *  src-tauri/src/insights.rs — if those edges ever change, update these to match. */
export const BAND_RANGE: Record<string, string> = {
  New: "0–1 yrs",
  Establishing: "2–4 yrs",
  Settled: "5–9 yrs",
  "Long-standing": "10–24 yrs",
  Legacy: "25+ yrs",
};
/** A membership-age band name with its year-range appended, e.g. "Settled · 5–9 yrs". */
export const bandLabel = (band: string) => (BAND_RANGE[band] ? `${band} · ${BAND_RANGE[band]}` : band);

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
  // Makeup by membership age: the share that joined within the last five fiscal years (New +
  // Establishing, ages 0–4), the Legacy share (25+ years), and the largest band by households.
  const newEstShare = ins.membership_age
    .filter((r) => r.band === "New" || r.band === "Establishing")
    .reduce((sum, r) => sum + r.pct_of_base, 0);
  const legacyRow = ins.membership_age.find((r) => r.band === "Legacy");
  const largestBand = [...ins.membership_age].sort((a, b) => b.households - a.households)[0];
  const makeup = largestBand && largestBand.households > 0
    ? `${Math.round(newEstShare * 10) / 10}% of today's members joined within the last five years; ${legacyRow?.pct_of_base ?? 0}% are Legacy households of 25 years or more. The largest band is ${largestBand.band} at ${fmt(largestBand.households)} households.`
    : "Not enough membership history yet.";
  // How the age mix has shifted: the Legacy (oldest) and New (youngest) shares now versus the
  // earliest year on record — two literal endpoints, so it never implies a straight-line trend.
  const otYears = [...new Set(ins.membership_age_over_time.map((r) => r.fy))].sort((a, b) => a - b);
  const shareIn = (fy: number, band: string) => ins.membership_age_over_time.find((r) => r.fy === fy && r.band === band)?.pct_of_base ?? 0;
  const ageShift = otYears.length >= 2
    ? `The base's age mix has shifted since ${fyLabel(otYears[0])}: Legacy (25+ yrs) is ${shareIn(otYears[otYears.length - 1], "Legacy")}% of members now versus ${shareIn(otYears[0], "Legacy")}% then, and New (0–1 yr) is ${shareIn(otYears[otYears.length - 1], "New")}% versus ${shareIn(otYears[0], "New")}%.`
    : "Not enough history to show how the age mix has shifted.";
  // Survivors: of everyone who joined FY2010 through the last complete year, how many remain.
  // FY2010 is the retention grid's floor — before it, departures aren't reliably recorded.
  const floorFy = 2010;
  const lastCompleteFy = ins.current_fy - 1;
  const joinedSince = ins.trend
    .filter((t) => t.fy >= floorFy && t.fy <= lastCompleteFy)
    .reduce((sum, t) => sum + t.joins, 0);
  const stillHereSince = ins.cohort_makeup
    .filter((r) => r.cohort >= floorFy && r.cohort <= lastCompleteFy)
    .reduce((sum, r) => sum + r.current, 0);
  const survivors = joinedSince > 0
    ? `Of the ${fmt(joinedSince)} households that joined between ${fyLabel(floorFy)} and ${fyLabel(lastCompleteFy)}, ${fmt(stillHereSince)} (${Math.round((1000 * stillHereSince) / joinedSince) / 10}%) are still members.`
    : "Not enough cohort history since FY2010 yet.";
  // Financial value by age: the band whose share of the money most exceeds its share of
  // households — the one carrying more than its numbers suggest.
  const gapBand = ins.financials
    ? [...ins.financials.by_membership_age]
        .filter((r) => r.households > 0)
        .sort((a, b) => (b.share_of_received - b.share_of_households) - (a.share_of_received - a.share_of_households))[0]
    : undefined;
  const financialAge = gapBand
    ? `${gapBand.band} households are ${gapBand.share_of_households}% of members but bring in ${gapBand.share_of_received}% of the money received.`
    : "Not enough financial history by membership age yet.";
  // Growth vs recurring revenue: the latest complete year's growth share (new-member cash as a
  // slice of new + recurring), which way it has moved since the first billing year, and — so a
  // "recurring is up" read can't hide a shrinking base — how the member base itself moved over
  // the same years. Billing only reaches back a few years, so this is a short series by nature.
  const growthShare = (r: FinancialGrowthRow) => {
    const total = r.new_received + r.recurring_received;
    return total > 0 ? Math.round((1000 * r.new_received) / total) / 10 : 0;
  };
  const growthYears = ins.financials?.by_growth.filter((r) => r.complete) ?? [];
  const gFirst = growthYears[0];
  const gLast = growthYears[growthYears.length - 1];
  const activeAt = (fy: number) => ins.trend.find((t) => t.fy === fy)?.active_end_of_fy;
  const growthHealth = (() => {
    if (!gLast) return "Not enough billing history to separate growth from recurring revenue yet.";
    const lastShare = growthShare(gLast);
    const baseFrom = gFirst ? activeAt(gFirst.fy) : undefined;
    const baseTo = activeAt(gLast.fy);
    const basePct = baseFrom !== undefined && baseTo !== undefined && baseFrom > 0
      ? Math.round((1000 * (baseTo - baseFrom)) / baseFrom) / 10
      : null;
    const baseClause = basePct !== null
      ? ` The member base ${basePct >= 0 ? "grew" : "shrank"} ${Math.abs(basePct)}% over the same years.`
      : "";
    if (growthYears.length < 2) {
      return `New members brought ${lastShare}% of ${fyLabel(gLast.fy)} dues received; the rest is recurring revenue from earlier members.${baseClause || " Only one complete billing year so far — no trend yet."}`;
    }
    // Report the recent move (vs the prior year) and the longer move (vs the first year) as
    // separate facts. A share that rose then fell must never read as a single "rising".
    const firstShare = growthShare(gFirst);
    const prev = growthYears[growthYears.length - 2];
    const prevShare = growthShare(prev);
    const dir = (from: number) => (lastShare > from + 0.5 ? "up" : lastShare < from - 0.5 ? "down" : "flat");
    const recent = dir(prevShare);
    const long = dir(firstShare);
    const trend = recent === long
      ? recent === "flat"
        ? `holding near ${firstShare}% since ${fyLabel(gFirst.fy)}`
        : `${recent} from ${firstShare}% in ${fyLabel(gFirst.fy)}`
      : `${recent} from ${prevShare}% in ${fyLabel(prev.fy)}, though ${long} from ${firstShare}% in ${fyLabel(gFirst.fy)}`;
    return `New members are ${lastShare}% of ${fyLabel(gLast.fy)} dues received, ${trend}; recurring members from earlier years carry the rest.${baseClause}`;
  })();
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
  return { trend, year1, cohort, makeup, ageShift, survivors, financialAge, growthHealth, channels, school, reasons };
}
