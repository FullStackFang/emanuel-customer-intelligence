import { useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Alert, Badge, Button, Card, CardHeader, CardTitle, EmptyState, Icon, Menu, MenuButton } from "../design-system";
import { PageTitle, Stat } from "../design-system/ui-kits/grant-management/chrome.jsx";
import "./insights/print.css";
import { ClassOverTimeChart, CohortHeatmap, ConcentrationChart, DuesChart, FlowsChart, GrowthVsRecurringChart, HBarChart, JoinedVsStillHereChart, MembershipAgeChart, MembershipAgeOverTimeChart, MoneyOverTimeChart, OutcomeByTenureChart, ReasonsHeatmap, TableView, TrendChart, ValueByAgeChart, Year1Chart } from "./insights/charts";
import type { JoinedVsStillHereRow } from "./insights/charts";
import { ZipGeographyMap, MODES as GEO_MODES, MODE_ORDER as GEO_MODE_ORDER } from "./insights/ZipGeographyMap";
import { bandLabel, EVIDENCE_LABELS, fmt, fmtMoney, fyLabel, soWhat } from "./insights/format";

function SoWhat({ text }: { text: string }) {
  return (
    <p style={{ margin: "var(--space-3) 0 0", padding: "var(--space-2) var(--space-3)", background: "var(--bg-secondary)", borderRadius: "var(--radius-md)", fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
      <span style={{ fontWeight: "var(--font-semibold)", color: "var(--text-primary)" }}>So what: </span>{text}
    </p>
  );
}

function Lede({ children }: { children: string }) {
  return <p style={{ margin: "0 0 var(--space-3)", fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>{children}</p>;
}

/** The undated-join footnote shared by every card built over dated households. Renders nothing
    when there is no remainder, so band households sum to members_now exactly. */
function UndatedNote({ count }: { count: number }) {
  if (count <= 0) return null;
  return (
    <p style={{ margin: "var(--space-2) 0 0", fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>
      {fmt(count)} current household{count === 1 ? " has" : "s have"} no usable join date and {count === 1 ? "is" : "are"} not shown above.
    </p>
  );
}

/**
 * The aggregate summary row that heads a tab — the high-level KPIs for whatever section is open.
 * Sits directly under the sticky tab bar as the first child of each section, so switching tabs
 * swaps the headline stats with the content. Responsive: tiles wrap on narrow widths and compose
 * into the PDF ahead of their section (no screen-only gating).
 */
function TabStats({ children }: { children: ReactNode }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(160px, 1fr))", gap: "var(--space-3)", marginBottom: "var(--space-4)" }}>
      {children}
    </div>
  );
}

function Unavailable({ column }: { column: string }) {
  return <EmptyState icon="database" title="Not available" message={`This view needs ${column} to be synced and not withheld.`} action={undefined} />;
}

const VIEW_LABELS: Record<api.InsightView, string> = {
  trend: "Membership trend",
  year1: "First-year retention",
  cohort_matrix: "Cohort retention",
  channels: "Stickiness by join reason",
  school: "Stickiness by school history",
  reasons: "Why people leave",
  at_risk: "At-risk households",
  multi_job: "Retention by number of join reasons",
  outcome_by_tenure: "Exit outcomes by tenure",
  school_progression: "Nursery to religious school",
  school_gap: "Churn since religious school",
  dues: "Dues renewal state",
  anchor_type: "Retention by engagement driver",
  anchor_count: "Retention by number of engagement drivers",
};

/** Map a retention-style row to the horizontal-bar shape. */
const bar = (label: string, pct: number, n: number, still: number) => ({ label, pct, n, still });

// Phase lists mirror the backend's `insights:progress` contract; `step` indexes them 1-based.
const REBUILD_PHASES = ["Reading membership records", "Building yearly membership history", "Applying engagement sources", "Writing analysis tables", "Finalizing"] as const;
const RISK_PHASES = ["Building feature rows", "Rolling validation", "Fitting final model", "Scoring current households"] as const;

const fmtElapsed = (ms: number) => {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

const monoStyle = { fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)", color: "var(--text-tertiary)" } as const;

/**
 * Live status of a backend job: a phase checklist with the current phase's counter, a
 * progress bar (determinate once a phase reports a total), and `Step n of m · elapsed`.
 * Before the first event it shows a single "reading" row so the user still sees time pass.
 * `compact` collapses it to one line for the inline rebuild banner.
 */
function JobStatus({ labels, progress, elapsed, compact = false }: { labels: readonly string[]; progress: api.InsightsProgress | null; elapsed: string; compact?: boolean }) {
  const counter = progress && progress.done != null && progress.total != null ? `${fmt(progress.done)} of ${fmt(progress.total)}` : null;
  const spinner = <span className="app-spinner" style={{ width: "0.85em", height: "0.85em" }} aria-hidden />;
  if (compact) {
    return (
      <div style={{ ...monoStyle, display: "flex", alignItems: "center", gap: "var(--space-2)", marginTop: "var(--space-2)" }}>
        {spinner}
        <span>{progress ? [progress.phase, counter, elapsed].filter(Boolean).join(" · ") : `Reading the local mirror… · ${elapsed}`}</span>
      </div>
    );
  }
  // Overall job fraction: completed phases plus the current phase's known share.
  const pct = progress && progress.done != null && progress.total != null && progress.total > 0
    ? Math.min(100, Math.round((((progress.step - 1) + progress.done / progress.total) / progress.steps) * 100))
    : null;
  const row = (label: string, state: "done" | "current" | "pending", detail?: string | null) => (
    <li key={label} data-state={state} style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", color: state === "pending" ? "var(--text-tertiary)" : "var(--text-primary)" }}>
      <span style={{ display: "inline-flex", width: 16, justifyContent: "center", color: state === "done" ? "var(--color-success-600, var(--text-secondary))" : "inherit" }}>
        {state === "done" ? <Icon name="circle-check" size={16} /> : state === "current" ? spinner : <Icon name="circle" size={16} />}
      </span>
      <span>{label}</span>
      {detail && <span style={monoStyle}>{detail}</span>}
    </li>
  );
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)", textAlign: "left" }}>
      <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: "var(--space-2)", fontSize: "var(--text-sm)" }}>
        {progress === null
          ? row("Reading the local mirror…", "current")
          : labels.map((label, i) => {
            const n = i + 1;
            return row(label, n < progress.step ? "done" : n === progress.step ? "current" : "pending", n === progress.step ? counter : null);
          })}
      </ul>
      <div className={`app-progress${pct !== null ? " is-determinate" : ""}`} role="progressbar" aria-valuenow={pct ?? undefined} aria-valuemin={0} aria-valuemax={100}>
        {pct !== null && <div className="app-progress-fill" style={{ width: `${pct}%` }} />}
      </div>
      <div style={monoStyle}>{progress ? `Step ${progress.step} of ${progress.steps} · ${elapsed}` : elapsed}</div>
    </div>
  );
}

/**
 * Session snapshot of the last loaded Insights data, kept at module scope so it survives the
 * page unmounting on tab switches. Returning to Insights paints from this instantly and
 * revalidates in the background, instead of reloading from scratch every time.
 */
let snapshot: { ins: api.Insights; risk: api.RiskSummary | null; riskFailed: boolean } | null = null;
/**
 * The one in-flight `get_insights` call, shared by concurrent loads (StrictMode's doubled
 * mount effect, a quick tab flip). The backend serializes every command behind one store
 * lock, so a duplicate request is not free — it is a second full read the user waits on.
 */
let inflightInsights: Promise<api.Insights> | null = null;
function fetchInsights(force: boolean): Promise<api.Insights> {
  if (force || !inflightInsights) {
    inflightInsights = api.getInsights(force).finally(() => { inflightInsights = null; });
  }
  return inflightInsights;
}
/** Test hook: clear the session snapshot so each case starts from a cold page. */
export function _resetInsightsSnapshot() { snapshot = null; inflightInsights = null; }

const PDF_LAYOUT_TIMEOUT_MS = 3_000;

/** Wait until every chart-bearing card has a printable layout before native capture. */
export async function waitForPdfReportLayout(surface: HTMLElement): Promise<void> {
  await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  const deadline = Date.now() + PDF_LAYOUT_TIMEOUT_MS;
  while (true) {
    const charts = [...surface.querySelectorAll<HTMLElement>(".recharts-responsive-container")];
    const ready = charts.length > 0 && charts.every((chart) => {
      const card = chart.closest<HTMLElement>(".insights-report-card");
      const { width, height } = chart.getBoundingClientRect();
      const cardRect = card?.getBoundingClientRect();
      return width > 0 && height > 0 && (cardRect?.width ?? 0) > 0 && (cardRect?.height ?? 0) > 0;
    });
    if (ready) return;
    if (Date.now() >= deadline) throw new Error("PDF could not be rendered because the report layout did not become ready.");
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

/** Reset the surrounding scroll container to the top. Switching tabs should always land on the
 *  top of the report — the KPI cards, then the pinned tab bar — the same place every time. The
 *  previous handler called scrollIntoView on the tab bar, which only moved when you happened to
 *  be scrolled down, so it looked like the header "occasionally" jumped. */
function scrollScrollportToTop(from: HTMLElement | null) {
  for (let n: HTMLElement | null = from; n; n = n.parentElement) {
    const oy = getComputedStyle(n).overflowY;
    if (oy === "auto" || oy === "scroll") { n.scrollTo({ top: 0 }); return; }
  }
}

/** The "Geography" tab, rendered as one cohesive split control: the label navigates to the tab,
 *  the caret opens the map-mode menu. It reads as a single pill (one border, one shadow, clipped
 *  corners, a hairline divider) rather than two abutting Buttons; each half lights on hover. */
function GeoSplitTab({ active, mode, onGo, onPickMode }: {
  active: boolean;
  mode: api.GeoMode;
  onGo: () => void;
  onPickMode: (m: api.GeoMode) => void;
}) {
  const [hover, setHover] = useState<"label" | "caret" | null>(null);
  const fill = active ? "var(--color-primary-600)" : "var(--bg-secondary)";
  const seg = (on: boolean): CSSProperties => ({
    display: "inline-flex", alignItems: "center", justifyContent: "center",
    height: "100%", border: "none", cursor: "pointer", transition: "var(--transition-all)",
    fontFamily: "var(--font-body)", fontWeight: "var(--font-medium)", fontSize: "var(--text-sm)",
    background: on ? fill : "transparent",
    color: active ? "var(--text-inverse)" : on ? "var(--text-primary)" : "var(--text-secondary)",
  });
  return (
    <Menu align="left"
      items={GEO_MODE_ORDER.map((m) => ({ key: m, label: GEO_MODES[m].tab, active: mode === m, onSelect: () => onPickMode(m) }))}
      trigger={({ open, toggle }: { open: boolean; toggle: () => void }) => (
        <div style={{
          display: "inline-flex", alignItems: "stretch", height: "var(--btn-height-md)", overflow: "hidden",
          borderRadius: "var(--radius-lg)", transition: "var(--transition-all)",
          background: active ? "var(--color-primary-500)" : "var(--bg-primary)",
          border: active ? "1px solid transparent"
            : `1px solid ${hover || open ? "var(--border-strong)" : "var(--border-default)"}`,
          boxShadow: active ? "var(--shadow-sm)" : "none",
        }}>
          <button type="button" onClick={onGo}
            onMouseEnter={() => setHover("label")} onMouseLeave={() => setHover(null)}
            style={{ ...seg(hover === "label"), padding: "var(--btn-padding-md)" }}>
            Geography
          </button>
          <span aria-hidden style={{ width: 1, alignSelf: "stretch", background: active ? "rgba(255,255,255,0.28)" : "var(--border-default)" }} />
          <button type="button" onClick={toggle}
            aria-label="Choose geography view" aria-haspopup="menu" aria-expanded={open}
            onMouseEnter={() => setHover("caret")} onMouseLeave={() => setHover(null)}
            style={{ ...seg(hover === "caret" || open), padding: "0 var(--space-2)" }}>
            <Icon name="chevron-down" size={14} />
          </button>
        </div>
      )}
    />
  );
}

export default function InsightsPage({ status }: PageProps) {
  const [ins, setIns] = useState<api.Insights | null>(snapshot?.ins ?? null);
  const [risk, setRisk] = useState<api.RiskSummary | null>(snapshot?.risk ?? null);
  const [riskFailed, setRiskFailed] = useState(snapshot?.riskFailed ?? false);
  const [watch, setWatch] = useState<api.WatchListView | null>(null);
  const [tab, setTab] = useState<"overview" | "join" | "engagement" | "financials" | "risk" | "geography">("overview");
  // The Geography map mode is chosen from the tab header's "Geography" split button, so it lives
  // here (above ZipGeographyMap) and is passed down as a controlled prop.
  const [geoMode, setGeoMode] = useState<api.GeoMode>("density");
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [exported, setExported] = useState<string | null>(null);
  const [progress, setProgress] = useState<api.InsightsProgress | null>(null);
  const [riskBusy, setRiskBusy] = useState(false);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [renderingPdf, setRenderingPdf] = useState(false);
  const pdfReportRef = useRef<HTMLDivElement>(null);
  const tabsRef = useRef<HTMLDivElement>(null);

  // Live job progress: the backend emits phase events only while a rebuild or risk fit runs,
  // so this stays null on the cached paths. On mount, ask whether a job is already running so
  // a remounted page (tab revisit) resumes its live status instead of showing nothing.
  useEffect(() => {
    let cancelled = false;
    let un: (() => void) | undefined;
    const onProgress = (p: api.InsightsProgress) => {
      const at = Date.now();
      setProgress(p);
      setNow(at);
      // Earliest known start wins: a job that began before this page mounted pulls the clock
      // back, while events for a job we started ourselves keep our own start.
      setStartedAt((s) => { const fromJob = at - p.elapsed_ms; return s === null || fromJob < s ? fromJob : s; });
    };
    void api.onInsightsProgress(onProgress).then((u) => { if (cancelled) u(); else un = u; });
    api.getInsightsJob().then((p) => { if (p && !cancelled) onProgress(p); }).catch(() => {});
    return () => { cancelled = true; un?.(); };
  }, []);

  // Elapsed-time ticker: only runs while something is in flight.
  const ticking = busy === "load" || busy === "rebuild" || riskBusy || progress !== null;
  useEffect(() => {
    if (!ticking) return;
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [ticking]);
  const elapsed = fmtElapsed(startedAt === null ? 0 : now - startedAt);

  const load = useCallback(async (force = false) => {
    // Revalidate quietly when we already have data (a tab revisit): keep the current view
    // on screen and don't show the loader. Only a first load or an explicit rebuild blanks.
    const quiet = snapshot !== null && !force;
    if (!quiet) { setBusy(force ? "rebuild" : "load"); setRisk(null); setRiskFailed(false); }
    setErr(null); setWatch(null); setStartedAt(Date.now()); setNow(Date.now());
    try {
      // Render the membership views first: await get_insights, paint the page, then load
      // the risk analysis independently into the Risk tab. The two backend commands share
      // one store lock and cannot truly overlap, so awaiting insights first keeps the page
      // from waiting behind the risk compute. A risk rejection resolves to a visible
      // failure state (never a permanent spinner) and never blanks the lifecycle views.
      const i = await fetchInsights(force);
      setIns(i);
      setProgress((p) => (p?.job === "rebuild" ? null : p));
      snapshot = { ins: i, risk: snapshot?.risk ?? null, riskFailed: snapshot?.riskFailed ?? false };
      setRiskBusy(true);
      void api.getRiskSummary()
        .then((r) => { setRisk(r); setRiskFailed(false); snapshot = { ins: i, risk: r, riskFailed: false }; })
        .catch(() => { setRiskFailed(true); snapshot = { ins: i, risk: snapshot?.risk ?? null, riskFailed: true }; })
        .finally(() => { setRiskBusy(false); setProgress((p) => (p?.job === "risk" ? null : p)); });
    } catch (e) { setErr(String(e)); setProgress(null); }
    finally { if (!quiet) setBusy(null); }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const loadWatch = async () => {
    setBusy("watch"); setErr(null);
    try { setWatch(await api.getWatchList()); } catch (e) { setErr(String(e)); } finally { setBusy(null); }
  };
  const doExport = async (view: api.InsightView) => {
    setBusy("export"); setErr(null); setExported(null);
    try { setExported(await api.exportInsightsCsv(view)); } catch (e) { setErr(String(e)); } finally { setBusy(null); }
  };
  const doWatchExport = async () => {
    setBusy("watchexport"); setErr(null); setExported(null);
    try { setExported(await api.exportWatchListCsv()); } catch (e) { setErr(String(e)); } finally { setBusy(null); }
  };
  const doPdf = async () => {
    if (!ins) return;
    setBusy("pdf"); setErr(null); setExported(null);
    setRenderingPdf(true);
    try {
      const surface = pdfReportRef.current;
      if (!surface) throw new Error("PDF could not be rendered because the report surface is unavailable.");
      await waitForPdfReportLayout(surface);
      setExported(await api.exportInsightsPdf());
    } catch (e) { setErr(String(e)); } finally { setRenderingPdf(false); setBusy(null); }
  };

  if (status.synced_rows === 0) {
    return (
      <div>
        <PageTitle eyebrow="Customer Intelligence" title="Insights" actions={undefined} />
        <EmptyState icon="chart-line" title="Nothing synced yet" message="Select Account on the Data page and run Sync now from the Overview page. Insights are built from the local mirror after each sync." action={undefined} />
      </div>
    );
  }

  const missing = (col: string) => ins?.unavailable.includes(col) ?? false;
  const capOn = (key: string) => ins?.capabilities.find((c) => c.key === key)?.available ?? false;
  const schoolCols = ["FormerReligiousSchoolStudents__c", "ActiveReligiousSchoolStudents__c", "WasEverNSAffiliated__c"];
  const missingSchoolCol = schoolCols.find(missing);
  const s = ins ? soWhat(ins) : null;
  const latestTwo = ins ? ins.year1.slice(-2).map((r) => r.cohort) : [];
  // Current households with no usable join date fall into no membership-age band; the band
  // households therefore sum to members_now minus this remainder, which each affected card notes.
  const ageUndated = ins ? ins.kpis.members_now - ins.membership_age.reduce((sum, r) => sum + r.households, 0) : 0;
  // Age mix over time, pivoted to one row per year with each band's share, for the table view.
  const ageBandOrder = ["New", "Establishing", "Settled", "Long-standing", "Legacy"] as const;
  const ageOverTimeYears = ins
    ? [...new Set(ins.membership_age_over_time.map((r) => r.fy))].sort((a, b) => a - b).map((fy) => {
        const shares: Record<string, number> = {};
        for (const r of ins.membership_age_over_time) if (r.fy === fy) shares[r.band] = r.pct_of_base;
        return { fy, shares };
      })
    : [];
  // Joined vs. still here: pair each FY2010→current cohort's joins (from trend) with its survivors
  // (from cohort_makeup). Cohorts before FY2010 collapse into one survivors-only bar — the retention
  // grid's floor, before which departures aren't reliably recorded, so a joined bar would mislead.
  const survivorRows: JoinedVsStillHereRow[] = [];
  if (ins) {
    const floorFy = 2010;
    const joinsByFy = new Map(ins.trend.map((t) => [t.fy, t.joins]));
    const survivorsByFy = new Map(ins.cohort_makeup.map((r) => [r.cohort, r.current]));
    const beforeFloor = ins.cohort_makeup.filter((r) => r.cohort < floorFy).reduce((sum, r) => sum + r.current, 0);
    if (beforeFloor > 0) survivorRows.push({ label: `Before ${fyLabel(floorFy)}`, joined: null, stillHere: beforeFloor });
    for (let fy = floorFy; fy <= ins.current_fy; fy++) {
      const joined = joinsByFy.get(fy) ?? 0;
      const stillHere = survivorsByFy.get(fy) ?? 0;
      if (joined === 0 && stillHere === 0) continue;
      survivorRows.push({ label: fyLabel(fy), joined, stillHere });
    }
  }
  // Join-reasons tab headline aggregates: the joiner window is grouped mutually-exclusively by how
  // many reasons a household stated, so those counts sum to a clean denominator (channels double-count
  // households under every reason they named). The top join reason is the best-retained reason above
  // a small count floor, so a one-household reason at 100% never wins.
  const jobsTotal = ins ? ins.multi_job.reduce((sum, r) => sum + r.n, 0) : 0;
  const jobsStill = ins ? ins.multi_job.reduce((sum, r) => sum + r.still_members, 0) : 0;
  const jobsRetainedPct = jobsTotal > 0 ? Math.round((1000 * jobsStill) / jobsTotal) / 10 : 0;
  const topChannel = ins && ins.channels.length
    ? [...ins.channels].sort((a, b) => b.pct - a.pct).find((c) => c.n >= 20) ?? [...ins.channels].sort((a, b) => b.n - a.n)[0]
    : undefined;
  const topJobsBucket = ins && ins.multi_job.length ? ins.multi_job.reduce((m, r) => (r.jobs > m.jobs ? r : m)) : undefined;

  // Engagement & Renewal tab headline aggregates: the most recent dues year on record, and the
  // best-retained engagement driver. Each is gated on its own source below, so a tile only appears
  // with data.
  const duesLatest = ins && ins.dues.length ? ins.dues[ins.dues.length - 1] : undefined;
  const topAnchor = ins && ins.anchor_type.length ? [...ins.anchor_type].sort((a, b) => b.pct - a.pct)[0] : undefined;

  const fin = ins?.financials ?? null;
  const finYears = fin ? fin.by_year.filter((r) => r.complete) : [];
  const finCompleteFys = new Set(finYears.map((r) => r.fy));
  const finYearClass = fin ? fin.by_year_class.filter((r) => finCompleteFys.has(r.fy)) : [];
  const finLatest = fin ? fin.by_year.find((r) => r.fy === fin.fiscal_year) : undefined;
  const finPrior = fin ? fin.by_year.filter((r) => r.complete && r.fy < fin.fiscal_year).slice(-1)[0] : undefined;
  const finYoyPct = finLatest && finPrior && finPrior.received > 0 ? Math.round((1000 * (finLatest.received - finPrior.received)) / finPrior.received) / 10 : null;
  // Members with no usable join date fall into no financial age band either; the value card notes them.
  const finUndated = fin ? fin.households - fin.by_membership_age.reduce((sum, r) => sum + r.households, 0) : 0;
  // Growth vs recurring revenue: complete years only, each year's growth share, and the received
  // cash from undated households (in neither the new nor recurring bucket) so the card can note it.
  const finGrowthYears = fin ? fin.by_growth.filter((r) => r.complete) : [];
  const growthSharePct = (r: api.FinancialGrowthRow) => {
    const total = r.new_received + r.recurring_received;
    return total > 0 ? Math.round((1000 * r.new_received) / total) / 10 : 0;
  };
  const finGrowthUndated = fin
    ? Math.round(finGrowthYears.reduce((sum, r) => {
        const year = fin.by_year.find((y) => y.fy === r.fy);
        return sum + (year ? year.received - r.new_received - r.recurring_received : 0);
      }, 0))
    : 0;
  const built = ins?.built_at ? new Date(ins.built_at).toLocaleString() : "not built";
  const anyAnchor = capOn("renewal") || capOn("school") || capOn("committee");
  const sectionClass = (key: typeof tab) => `insights-section${renderingPdf || tab === key ? "" : " insights-section-hidden"}`;
  const rebuilding = progress?.job === "rebuild" || busy === "rebuild";
  const rebuildProgress = progress?.job === "rebuild" ? progress : null;
  const riskProgress = progress?.job === "risk" ? progress : null;

  return (
    <div style={{ width: "100%", maxWidth: 1180, margin: "0 auto" }}>
      <div className="insights-screen-only">
        {ins && rebuilding && (
          <Alert tone="info" style={{ marginBottom: "var(--space-4)" }}>
            Rebuilding insights from the latest sync — showing the previous build until it finishes.
            <JobStatus compact labels={REBUILD_PHASES} progress={rebuildProgress} elapsed={elapsed} />
          </Alert>
        )}
        {err && ins && <Alert tone="error" style={{ marginBottom: "var(--space-4)" }}>{err}</Alert>}
        {exported && (
          <Alert tone="success" style={{ marginBottom: "var(--space-4)" }}>
            Exported to <span style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)" }}>{exported}</span>{" "}
            <Button size="sm" variant="secondary" disabled={busy !== null} onClick={() => { api.revealExport(exported).catch((e) => setErr(String(e))); }}>Reveal</Button>
          </Alert>
        )}
      </div>

      {!ins && err ? (
        <Card style={{ marginTop: "var(--space-6)" }}>
          <EmptyState icon="triangle-alert" title="Insights could not load" message={err} action={<Button onClick={() => void load()}>Try again</Button>} />
        </Card>
      ) : !ins ? (
        <Card style={{ marginTop: "var(--space-6)", padding: "var(--space-8) var(--space-6)" }}>
          <div style={{ maxWidth: 420, margin: "0 auto", display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
            <div style={{ fontFamily: "var(--font-display)", fontSize: "var(--text-lg)", fontWeight: "var(--font-semibold)", color: "var(--text-primary)", textAlign: "center" }}>
              {rebuilding ? "Building insights" : "Loading insights"}
            </div>
            <JobStatus labels={REBUILD_PHASES} progress={rebuildProgress} elapsed={elapsed} />
            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-tertiary)", textAlign: "center" }}>
              This happens once after each sync, then it's cached. You can use other pages meanwhile.
            </div>
          </div>
        </Card>
      ) : (
        <>
          <div ref={pdfReportRef} data-testid={renderingPdf ? "insights-pdf-surface" : undefined} className={renderingPdf ? "insights-pdf-surface" : undefined}>
          <div className="insights-report-only" style={{ marginBottom: "var(--space-5)" }}>
            <div style={{ fontSize: "var(--text-2xs)", letterSpacing: "var(--tracking-wider)", textTransform: "uppercase", color: "var(--text-tertiary)", fontWeight: "var(--font-semibold)" }}>Temple Emanu-El · Customer Intelligence</div>
            <div style={{ fontFamily: "var(--font-display)", fontSize: "var(--text-2xl)", fontWeight: "var(--font-semibold)", color: "var(--text-primary)" }}>Membership Insights</div>
            <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>Built {built} · {fyLabel(ins.current_fy)} in progress · report generated {new Date().toLocaleDateString()}</div>
          </div>
          <div ref={tabsRef} className="insights-screen-only" style={{ position: "sticky", top: 0, zIndex: 5, background: "var(--bg-secondary)", paddingTop: "var(--space-3)", paddingBottom: "var(--space-3)", marginBottom: "var(--space-2)" }}>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "var(--space-3)", flexWrap: "wrap", background: "var(--bg-primary)", border: "1px solid var(--border-default)", borderRadius: "var(--radius-xl)", boxShadow: "var(--shadow-md)", padding: "var(--space-2) var(--space-3)" }}>
              <div role="tablist" aria-label="Insights sections" style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
              {(["overview", "join", "engagement", "financials", "risk", "geography"] as const).map((key) => {
                const goTo = () => { if (key !== tab) { setTab(key); scrollScrollportToTop(tabsRef.current); } };
                // Geography is a split button: the label navigates to the tab (keeping the current
                // mode), the caret opens the mode menu — picking a mode also jumps to the tab.
                if (key === "geography") {
                  return (
                    <GeoSplitTab key={key} active={tab === "geography"} mode={geoMode}
                      onGo={goTo} onPickMode={(m) => { setGeoMode(m); goTo(); }} />
                  );
                }
                return (
                  <Button key={key} variant={tab === key ? "primary" : "secondary"} onClick={goTo}>
                    {key === "overview" ? "Overview" : key === "join" ? "Join reasons" : key === "engagement" ? "Engagement & Renewal" : key === "financials" ? "Financials" : "Attrition & Risk"}
                  </Button>
                );
              })}
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: "var(--space-3)", marginLeft: "auto" }}>
              {riskBusy && (
                <span style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-1)", fontSize: "var(--text-xs)", color: "var(--text-tertiary)", whiteSpace: "nowrap" }}>
                  <span className="app-spinner" style={{ width: "0.8em", height: "0.8em" }} aria-hidden />
                  {riskProgress ? `Risk analysis: step ${riskProgress.step} of ${riskProgress.steps} · ${elapsed}` : `Risk analysis running · ${elapsed}`}
                </span>
              )}
              <MenuButton disabled={busy !== null} items={[
                { key: "pdf", label: "Download PDF report", icon: "file-text", disabled: !ins, onSelect: () => void doPdf() },
                { divider: true },
                ...api.INSIGHT_VIEWS.map((v) => ({ key: v, label: `CSV · ${VIEW_LABELS[v]}`, icon: "table", disabled: !ins, onSelect: () => void doExport(v) })),
                { divider: true },
                { key: "rebuild", label: "Rebuild insights", icon: "refresh-cw", onSelect: () => void load(true) },
              ]}>
                {busy === "pdf" ? "Rendering…" : busy === "export" ? "Exporting…" : busy === "rebuild" ? "Rebuilding…" : "Export"}
              </MenuButton>
            </div>
            </div>
          </div>

          {/* ── Overview ─────────────────────────────────────────────────────── */}
          <div className={renderingPdf || tab === "overview" ? "insights-overview" : "insights-overview insights-overview-hidden"}>
          <TabStats>
            <Stat label="Member households" value={fmt(ins.kpis.members_now)} sub={`${ins.kpis.net_vs_prior_fy >= 0 ? "+" : ""}${fmt(ins.kpis.net_vs_prior_fy)} vs ${fyLabel(ins.current_fy - 1)} · ${fyLabel(ins.current_fy)} in progress`} icon="users" tone="primary" />
            <Stat label={`Joins ${fyLabel(ins.current_fy)}`} value={fmt(ins.kpis.joins_this_fy)} sub="fiscal year to date" icon="user-plus" tone="success" />
            <Stat label={`Resignations ${fyLabel(ins.current_fy)}`} value={fmt(ins.kpis.resigns_this_fy)} sub="fiscal year to date" icon="user-minus" tone="neutral" />
            <Stat label="First-year retention" value={`${ins.kpis.year1_pct}%`} sub={`${fyLabel(ins.kpis.year1_cohort)} cohort · baseline ${ins.kpis.year1_baseline_pct}%`} icon="repeat" tone="primary" />
            <Stat label="Households at risk" value={fmt(ins.kpis.at_risk_count)} sub="current members matching a churn pattern" icon="triangle-alert" tone="accent" />
          </TabStats>
          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Membership over time</CardTitle></CardHeader>
            <Lede>Active member households at the end of each fiscal year, and the joins and resignations behind them. The current fiscal year is in progress.</Lede>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "var(--space-4)" }}>
              <TrendChart rows={ins.trend} />
              <FlowsChart rows={ins.trend} />
            </div>
            <SoWhat text={s!.trend} />
            <TableView rows={ins.trend} getRowKey={(r) => String(r.fy)} columns={[
              { key: "fy", header: "Fiscal year", render: (r) => fyLabel(r.fy) },
              { key: "j", header: "Joins", align: "right", render: (r) => fmt(r.joins) },
              { key: "r", header: "Resignations", align: "right", render: (r) => fmt(r.resigns) },
              { key: "a", header: "Active at year end", align: "right", render: (r) => fmt(r.active_end_of_fy) },
            ]} />
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>First-year retention by cohort</CardTitle></CardHeader>
            <Lede>Of the households that joined in each fiscal year, the share still members one year later. The two newest cohorts are highlighted. The newest cohort's first year is still in progress.</Lede>
            <Year1Chart rows={ins.year1} emphasize={latestTwo} />
            <SoWhat text={s!.year1} />
            <TableView rows={ins.year1} getRowKey={(r) => String(r.cohort)} columns={[
              { key: "c", header: "Cohort", render: (r) => fyLabel(r.cohort) },
              { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
              { key: "p", header: "Still members after 1 year", align: "right", render: (r) => `${r.pct_retained}%` },
            ]} />
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Cohort retention</CardTitle></CardHeader>
            <Lede>Each row is a join-year cohort; each cell is the share still members after that many years. Blank cells are years that haven't happened yet.</Lede>
            <CohortHeatmap cells={ins.cohort_matrix} />
            <SoWhat text={s!.cohort} />
            <TableView rows={ins.cohort_matrix} getRowKey={(r) => `${r.cohort}-${r.k}`} columns={[
              { key: "c", header: "Cohort", render: (r) => fyLabel(r.cohort) },
              { key: "k", header: "Years after", align: "right", render: (r) => r.k },
              { key: "p", header: "Still members", align: "right", render: (r) => `${r.pct_retained}%` },
            ]} />
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Makeup of today's members by membership age</CardTitle></CardHeader>
            <Lede>How old the base is, in one look: today's member households grouped by membership age — fiscal years since they joined — into five lifecycle bands: New (0–1 yrs), Establishing (2–4), Settled (5–9), Long-standing (10–24) and Legacy (25+).</Lede>
            <MembershipAgeChart rows={ins.membership_age} />
            <SoWhat text={s!.makeup} />
            <TableView rows={ins.membership_age} getRowKey={(r) => r.band} columns={[
              { key: "b", header: "Membership age", render: (r) => bandLabel(r.band) },
              { key: "n", header: "Member households", align: "right", render: (r) => fmt(r.households) },
              { key: "p", header: "Share of base", align: "right", render: (r) => `${r.pct_of_base}%` },
            ]} />
            <UndatedNote count={ageUndated} />
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>How the age mix has changed over time</CardTitle></CardHeader>
            <Lede>Each year's member base by age band, back to FY2010. A widening Legacy (dark) top means the base is aging; a thinning New (pale) bottom means fewer new members — built from membership records, so it isn't limited by billing.</Lede>
            <MembershipAgeOverTimeChart rows={ins.membership_age_over_time} />
            <SoWhat text={s!.ageShift} />
            <TableView rows={ageOverTimeYears} getRowKey={(r) => String(r.fy)} columns={[
              { key: "fy", header: "Fiscal year", render: (r) => fyLabel(r.fy) },
              ...ageBandOrder.map((b) => ({ key: b, header: b, align: "right" as const, render: (r: { shares: Record<string, number> }) => `${r.shares[b] ?? 0}%` })),
            ]} />
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Joined vs. still here</CardTitle></CardHeader>
            <Lede>{`For each join cohort from FY2010 to today, how many households joined that year beside how many are still members — how much of each intake is left. Cohorts before FY2010 are shown as survivors only: departures before then aren't reliably recorded, so a join count would overstate retention (the retention grid draws the same line for the same reason).`}</Lede>
            <JoinedVsStillHereChart rows={survivorRows} />
            <SoWhat text={s!.survivors} />
            <TableView rows={survivorRows} getRowKey={(r) => r.label} columns={[
              { key: "c", header: "Join cohort", render: (r) => r.label },
              { key: "j", header: "Joined", align: "right", render: (r) => (r.joined === null ? "—" : fmt(r.joined)) },
              { key: "s", header: "Still here", align: "right", render: (r) => fmt(r.stillHere) },
            ]} />
            <UndatedNote count={ageUndated} />
          </Card>

          </div>

          {/* ── Join reasons ─────────────────────────────────────────────────── */}
          <div className={sectionClass("join")}>
          {!missing("Join_Reason__c") && jobsTotal > 0 && (
            <TabStats>
              <Stat label="Households analyzed" value={fmt(jobsTotal)} sub="joined 4–12 fiscal years ago" icon="users" tone="primary" />
              {topChannel && <Stat label="Top join reason" value={topChannel.label} sub={`${topChannel.pct}% still members`} icon="repeat" tone="success" />}
              <Stat label="Still members" value={`${jobsRetainedPct}%`} sub="across the joiner window" icon="activity" tone="neutral" />
              {topJobsBucket && <Stat label="Most reasons stated" value={`${topJobsBucket.pct}%`} sub={`${topJobsBucket.bucket} · still members`} icon="layers" tone="primary" />}
            </TabStats>
          )}
          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Stickiness by join reason</CardTitle></CardHeader>
            {missing("Join_Reason__c") ? <Unavailable column="Join_Reason__c" /> : (
              <>
                <Lede>Stated reasons for joining, as evidence. Households that joined 4–12 fiscal years ago; share still members today, counted under every reason they named. Associations, not claims about intent or causation. School-driven reasons are highlighted.</Lede>
                <HBarChart rows={ins.channels.map((c) => bar(c.label, c.pct, c.n, c.still_members))} emphasize={["Religious school", "Nursery school"]} />
                <SoWhat text={s!.channels} />
                <TableView rows={ins.channels} getRowKey={(r) => r.key} columns={[
                  { key: "l", header: "Join reason", render: (r) => r.label },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                  { key: "s", header: "Still members", align: "right", render: (r) => fmt(r.still_members) },
                  { key: "p", header: "Share", align: "right", render: (r) => `${r.pct}%` },
                  { key: "t", header: "Avg tenure (yrs)", align: "right", render: (r) => r.avg_tenure },
                ]} />
              </>
            )}
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Retention by number of join reasons</CardTitle></CardHeader>
            {missing("Join_Reason__c") ? <Unavailable column="Join_Reason__c" /> : (
              <>
                <Lede>Households grouped by how many joining reasons they stated. More stated reasons is an association with retention, not evidence of stronger intent.</Lede>
                <HBarChart rows={ins.multi_job.map((r) => bar(r.bucket, r.pct, r.n, r.still_members))} />
                <TableView rows={ins.multi_job} getRowKey={(r) => r.bucket} columns={[
                  { key: "b", header: "Stated reasons", render: (r) => r.bucket },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                  { key: "s", header: "Still members", align: "right", render: (r) => fmt(r.still_members) },
                  { key: "p", header: "Share", align: "right", render: (r) => `${r.pct}%` },
                  { key: "t", header: "Avg tenure (yrs)", align: "right", render: (r) => r.avg_tenure },
                ]} />
              </>
            )}
          </Card>

          </div>

          {/* ── Engagement & Renewal ─────────────────────────────────────────── */}
          <div className={sectionClass("engagement")}>
          {((capOn("renewal") && duesLatest) || topAnchor) && (
            <TabStats>
              {capOn("renewal") && duesLatest && (
                <>
                  <Stat label={`Dues billed ${fyLabel(duesLatest.fy)}`} value={fmt(duesLatest.billed)} sub={`of ${fmt(duesLatest.active)} active households`} icon="badge-dollar-sign" tone="primary" />
                  <Stat label="Coverage missing" value={fmt(duesLatest.coverage_missing)} sub="no dues line found while active" icon="triangle-alert" tone="accent" />
                  <Stat label="Settled (eventual)" value={fmt(duesLatest.settled)} sub="of the households billed" icon="circle-check" tone="success" />
                </>
              )}
              {topAnchor && <Stat label="Top engagement driver" value={topAnchor.label} sub={`${topAnchor.pct}% still active`} icon="anchor" tone="neutral" />}
            </TabStats>
          )}
          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Retention by engagement driver</CardTitle></CardHeader>
            {!anyAnchor ? <Unavailable column="BillingStatement__c, Class_Enrolment__c, or Committee_Membership__c" /> : (
              <>
                <Lede>Households that held each observed engagement driver — dues renewal, nursery school, religious school, or committee service — in a recent fiscal year, and the share still active now. Only drivers from an available source appear.</Lede>
                <HBarChart rows={ins.anchor_type.map((a) => bar(a.label, a.pct, a.n, a.still_members))} />
                <TableView rows={ins.anchor_type} getRowKey={(r) => r.key} columns={[
                  { key: "l", header: "Engagement driver", render: (r) => r.label },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                  { key: "s", header: "Still active", align: "right", render: (r) => fmt(r.still_members) },
                  { key: "p", header: "Share", align: "right", render: (r) => `${r.pct}%` },
                ]} />
              </>
            )}
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Retention by number of engagement drivers</CardTitle></CardHeader>
            {!anyAnchor ? <Unavailable column="an engagement source" /> : (
              <>
                <Lede>Households grouped by the most distinct engagement drivers they held in a recent fiscal year. Holding more drivers is associated with, not proven to cause, higher retention.</Lede>
                <HBarChart rows={ins.anchor_count.map((a) => bar(a.label, a.pct, a.n, a.still_members))} />
                <TableView rows={ins.anchor_count} getRowKey={(r) => String(r.anchors)} columns={[
                  { key: "l", header: "Drivers held", render: (r) => r.label },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                  { key: "s", header: "Still active", align: "right", render: (r) => fmt(r.still_members) },
                  { key: "p", header: "Share", align: "right", render: (r) => `${r.pct}%` },
                ]} />
              </>
            )}
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Stickiness by school history</CardTitle></CardHeader>
            {missingSchoolCol ? <Unavailable column={missingSchoolCol} /> : (
              <>
                <Lede>Same joiner window, grouped by whether the household ever had a child in nursery or religious school.</Lede>
                <HBarChart rows={ins.school.map((g) => bar(g.group, g.pct, g.n, g.still_members))} />
                <SoWhat text={s!.school} />
                <TableView rows={ins.school} getRowKey={(r) => r.group} columns={[
                  { key: "g", header: "School history", render: (r) => r.group },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                  { key: "p", header: "Share still members", align: "right", render: (r) => `${r.pct}%` },
                ]} />
              </>
            )}
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Nursery to religious school</CardTitle></CardHeader>
            {missingSchoolCol ? <Unavailable column={missingSchoolCol} /> : (
              <>
                <Lede>Of nursery-school families, how many also became religious-school families, and how each group retains.</Lede>
                <HBarChart rows={ins.school_progression.map((g) => bar(g.group, g.pct, g.n, g.still_members))} />
                <TableView rows={ins.school_progression} getRowKey={(r) => r.group} columns={[
                  { key: "g", header: "Group", render: (r) => r.group },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                  { key: "s", header: "Still members", align: "right", render: (r) => fmt(r.still_members) },
                  { key: "p", header: "Share", align: "right", render: (r) => `${r.pct}%` },
                ]} />
              </>
            )}
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Churn since religious school</CardTitle></CardHeader>
            {missingSchoolCol ? <Unavailable column={missingSchoolCol} /> : (
              <>
                <Lede>Retention of religious-school families by how many completed fiscal years since their last active year. Only families whose religious school has ended are included.</Lede>
                <HBarChart rows={ins.school_gap.map((g) => bar(g.bucket, g.pct, g.n, g.still_members))} />
                <TableView rows={ins.school_gap} getRowKey={(r) => r.bucket} columns={[
                  { key: "b", header: "Years since", render: (r) => r.bucket },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                  { key: "s", header: "Still members", align: "right", render: (r) => fmt(r.still_members) },
                  { key: "p", header: "Share", align: "right", render: (r) => `${r.pct}%` },
                ]} />
              </>
            )}
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Dues renewal state</CardTitle></CardHeader>
            {!capOn("renewal") ? <Unavailable column="BillingStatement__c and BillingStatementLine__c" /> : (
              <>
                <Lede>Qualifying membership dues billed to active households by fiscal year. Coverage-missing means no dues line was found while the household was active — unknown billing, never proven non-renewal. Balance and receipt figures are eventual settlement, not an as-of historical state.</Lede>
                <DuesChart rows={ins.dues} />
                <TableView rows={ins.dues} getRowKey={(r) => String(r.fy)} columns={[
                  { key: "f", header: "Fiscal year", render: (r) => fyLabel(r.fy) },
                  { key: "a", header: "Active households", align: "right", render: (r) => fmt(r.active) },
                  { key: "b", header: "Dues billed", align: "right", render: (r) => fmt(r.billed) },
                  { key: "m", header: "Coverage missing", align: "right", render: (r) => fmt(r.coverage_missing) },
                  { key: "s", header: "Settled (eventual)", align: "right", render: (r) => fmt(r.settled) },
                ]} />
              </>
            )}
          </Card>

          </div>

          {/* ── Financials ───────────────────────────────────────────────────── */}
          <div className={sectionClass("financials")}>
          {!capOn("renewal") || !fin ? (
            <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
              <CardHeader><CardTitle>Financials</CardTitle></CardHeader>
              <Unavailable column="BillingStatement__c and BillingStatementLine__c with charge and received amounts" />
            </Card>
          ) : (
            <>
              <TabStats>
                <Stat label={`Received ${fyLabel(fin.fiscal_year)}`} value={fmtMoney(finLatest?.received ?? 0)} sub="latest complete year" icon="badge-dollar-sign" tone="success" />
                <Stat label="Collected" value={`${finLatest && finLatest.billed > 0 ? Math.round((1000 * finLatest.received) / finLatest.billed) / 10 : 0}%`} sub="of amount billed" icon="percent" tone="primary" />
                <Stat label="Year over year" value={finYoyPct === null ? "—" : `${finYoyPct >= 0 ? "+" : ""}${finYoyPct}%`} sub={finPrior ? `received vs ${fyLabel(finPrior.fy)}` : "no prior year"} icon="trending-up" tone={finYoyPct !== null && finYoyPct < 0 ? "accent" : "success"} />
              </TabStats>
              <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
                <CardHeader><CardTitle>Money in over time</CardTitle></CardHeader>
                <Lede>{`Dollars billed and received each fiscal year, across all billed households (not only today's members, so earlier years aren't understated). Figures begin with the FY2023 accounting-system migration — a short window, and that first year is only partial; the in-progress year is left off the bars.`}</Lede>
                <MoneyOverTimeChart rows={finYears} />
                <TableView rows={fin.by_year} getRowKey={(r) => String(r.fy)} columns={[
                  { key: "fy", header: "Fiscal year", render: (r) => `${fyLabel(r.fy)}${r.complete ? "" : " (partial)"}` },
                  { key: "b", header: "Billed", align: "right", render: (r) => fmtMoney(r.billed) },
                  { key: "r", header: "Received", align: "right", render: (r) => fmtMoney(r.received) },
                  { key: "c", header: "Collected", align: "right", render: (r) => `${r.billed > 0 ? Math.round((1000 * r.received) / r.billed) / 10 : 0}%` },
                ]} />
              </Card>

              <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
                <CardHeader><CardTitle>Growth vs. recurring revenue</CardTitle></CardHeader>
                <Lede>{`Each year's dues received, split by paying households that are new (joined that year) vs recurring (joined earlier); the line is growth's share. Figures begin with the FY2023 accounting-system migration, so it's a short series — read it as a direction.`}</Lede>
                <GrowthVsRecurringChart rows={fin.by_growth} />
                <SoWhat text={s!.growthHealth} />
                <TableView rows={finGrowthYears} getRowKey={(r) => String(r.fy)} columns={[
                  { key: "fy", header: "Fiscal year", render: (r) => fyLabel(r.fy) },
                  { key: "n", header: "New members", align: "right", render: (r) => fmtMoney(r.new_received) },
                  { key: "rec", header: "Recurring", align: "right", render: (r) => fmtMoney(r.recurring_received) },
                  { key: "gs", header: "Growth share", align: "right", render: (r) => `${growthSharePct(r)}%` },
                  { key: "mb", header: "Member households", align: "right", render: (r) => { const a = ins.trend.find((t) => t.fy === r.fy)?.active_end_of_fy; return a === undefined ? "—" : fmt(a); } },
                ]} />
                {finGrowthUndated > 0 && (
                  <p style={{ margin: "var(--space-2) 0 0", fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>
                    {`${fmtMoney(finGrowthUndated)} of cash received across these years came from households with no usable join date and is left out of both columns.`}
                  </p>
                )}
              </Card>

              <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
                <CardHeader><CardTitle>Where the money comes in over time</CardTitle></CardHeader>
                <Lede>Cash received by product class each complete fiscal year. A tall one-year segment — a big gift year, say — stands out against steady dues, so the makeup of the money is visible, not just the total.</Lede>
                <ClassOverTimeChart rows={finYearClass} />
              </Card>

              <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
                <CardHeader><CardTitle>Who carries the base</CardTitle></CardHeader>
                <Lede>{`Across today's member households, the cumulative share of ${fyLabel(fin.fiscal_year)} money held by the top-paying tenth, fifth, and so on — ranked by cash received. A steep early climb means a few households carry the base. Figures are aggregate; the smallest unit shown is a tenth of the membership, never a household.`}</Lede>
                <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: "var(--space-3)", marginBottom: "var(--space-4)" }}>
                  <Stat label="Top 10% of members" value={`${fin.concentration[0]?.cumulative_received_share ?? 0}%`} sub="of cash received" icon="trending-up" tone="primary" />
                  <Stat label="Top 20% of members" value={`${fin.concentration[1]?.cumulative_received_share ?? 0}%`} sub="of cash received" icon="users" tone="neutral" />
                  <Stat label="Paying households" value={fmt(fin.paying_households)} sub={`of ${fmt(fin.households)} members`} icon="badge-dollar-sign" tone="success" />
                </div>
                <ConcentrationChart rows={fin.concentration} />
                <TableView rows={fin.concentration} getRowKey={(r) => String(r.decile)} columns={[
                  { key: "d", header: "Member band", render: (r) => `Top ${r.decile * 10}%` },
                  { key: "h", header: "Households", align: "right", render: (r) => fmt(r.households) },
                  { key: "cr", header: "Cumulative received", align: "right", render: (r) => `${r.cumulative_received_share}%` },
                  { key: "cb", header: "Cumulative billed", align: "right", render: (r) => `${r.cumulative_billed_share}%` },
                ]} />
              </Card>

              <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
                <CardHeader><CardTitle>Value by membership age</CardTitle></CardHeader>
                <Lede>{`For each membership-age band — New (0–1 yrs), Establishing (2–4), Settled (5–9), Long-standing (10–24), Legacy (25+) — its share of member households beside its share of ${fyLabel(fin.fiscal_year)} cash received; the gap between the two is the headline. This is a one-year snapshot, not lifetime value (billing only reaches back to the FY2023 accounting-system migration). The per-household average is shown only for a band of at least ten households.`}</Lede>
                <ValueByAgeChart rows={fin.by_membership_age} />
                <SoWhat text={s!.financialAge} />
                <TableView rows={fin.by_membership_age} getRowKey={(r) => r.band} columns={[
                  { key: "b", header: "Membership age", render: (r) => bandLabel(r.band) },
                  { key: "h", header: "Households", align: "right", render: (r) => fmt(r.households) },
                  { key: "sh", header: "Share of households", align: "right", render: (r) => `${r.share_of_households}%` },
                  { key: "sr", header: "Share of money", align: "right", render: (r) => `${r.share_of_received}%` },
                  { key: "t", header: `Received ${fyLabel(fin.fiscal_year)}`, align: "right", render: (r) => fmtMoney(r.received) },
                  { key: "p", header: "Per household", align: "right", render: (r) => (r.received_per_household === null ? "—" : fmtMoney(r.received_per_household)) },
                ]} />
                {fin.by_membership_age.some((r) => r.households > 0 && r.received_per_household === null) && (
                  <p style={{ margin: "var(--space-2) 0 0", fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>
                    Bands with fewer than 10 households show no per-household average (shown as “—”), so no single household's dues can be inferred.
                  </p>
                )}
                <UndatedNote count={finUndated} />
              </Card>
            </>
          )}
          </div>

          {/* ── Attrition & Risk ─────────────────────────────────────────────── */}
          <div className={sectionClass("risk")}>
          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Why people leave</CardTitle></CardHeader>
            {missing("Resign_Reason__c") ? <Unavailable column="Resign_Reason__c" /> : (
              <>
                <Lede>Coded resignation reasons by fiscal year — each row a specific reason (most common on top), each column a year, darker cells more households. Affordability stays separate from disengagement, moving from death; only deaths, uncoded, and administrative exits fold into "Other / not actionable". Sparse coding shows as pale cells.</Lede>
                <ReasonsHeatmap cells={ins.reasons} />
                <SoWhat text={s!.reasons} />
                <TableView rows={ins.reasons} getRowKey={(r) => `${r.fy}-${r.reason}`} columns={[
                  { key: "f", header: "Fiscal year", render: (r) => fyLabel(r.fy) },
                  { key: "r", header: "Reason", render: (r) => r.reason },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                ]} />
              </>
            )}
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Exit Outcomes by tenure</CardTitle></CardHeader>
            {missing("Resign_Reason__c") ? <Unavailable column="Resign_Reason__c" /> : (
              <>
                <Lede>Resigned households grouped by how long they were members. The chart colors each exit by its family — addressable churn, conversion loss, structural exit, or not-actionable — while the table lists the specific reason. When a household names more than one reason, precedence picks a single primary.</Lede>
                <OutcomeByTenureChart rows={ins.outcome_by_tenure} />
                <TableView rows={ins.outcome_by_tenure} getRowKey={(r) => `${r.tenure_bucket}-${r.outcome}`} columns={[
                  { key: "t", header: "Tenure at exit", render: (r) => r.tenure_bucket },
                  { key: "o", header: "Exit reason", render: (r) => r.outcome },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                ]} />
              </>
            )}
          </Card>

          {risk && risk.available && (
            <TabStats>
              <Stat label="ROC-AUC" value={risk.roc_auc.toFixed(3)} sub="discrimination (≥ 0.65)" icon="activity" tone="primary" />
              <Stat label="Top-decile lift" value={`${risk.top_decile_lift.toFixed(2)}×`} sub="vs base rate (≥ 2.0)" icon="trending-up" tone="success" />
              <Stat label="Brier score" value={risk.brier.toFixed(4)} sub={`baseline ${risk.baseline_brier.toFixed(4)}`} icon="target" tone="neutral" />
              <Stat label="Watch List" value={fmt(risk.watch_list_count)} sub="evidence-gated households" icon="list-checks" tone="accent" />
            </TabStats>
          )}
          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Validated churn risk</CardTitle></CardHeader>
            {riskFailed ? (
              <Alert tone="warning" style={{ marginBottom: "var(--space-3)" }}>The churn risk analysis could not be computed from the local mirror. The membership views above are unaffected.</Alert>
            ) : risk === null && riskBusy ? (
              <>
                <Lede>Analyzing churn risk. The membership views are already final.</Lede>
                <JobStatus labels={RISK_PHASES} progress={riskProgress} elapsed={elapsed} />
              </>
            ) : risk === null ? (
              <Lede>Waiting for insights to load…</Lede>
            ) : risk.available ? (
              <Lede>A regularized logistic model of Addressable Churn passed rolling historical validation. Scores rank current households; they are associations from history, not predictions that any household will resign.</Lede>
            ) : (
              <>
                <Alert tone="warning" style={{ marginBottom: "var(--space-3)" }}>No validated household ranking. The model did not pass every validation gate, so no household scores or names are produced. Aggregate evidence below still stands.</Alert>
                {risk.unavailable_reason && <p style={{ margin: "0 0 var(--space-3)", fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>{risk.unavailable_reason}</p>}
              </>
            )}
            {risk && risk.model_first_fy != null && (
              <TableView rows={risk.years} getRowKey={(r) => String(r.test_fy)} empty="No rolling test years are available yet." columns={[
                { key: "y", header: "Test year", render: (r) => fyLabel(r.test_fy) },
                { key: "h", header: "Eligible households", align: "right", render: (r) => fmt(r.households) },
                { key: "e", header: "Addressable exits", align: "right", render: (r) => fmt(r.exits) },
                { key: "s", header: "Meets sample floor", render: (r) => <Badge tone={r.sufficient ? "success" : "neutral"}>{r.sufficient ? "yes" : "no"}</Badge> },
              ]} />
            )}
            {risk && risk.removed_families.length > 0 && (
              <p style={{ margin: "var(--space-2) 0 0", fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
                Feature families removed for low coverage: {risk.removed_families.join(", ")}.
              </p>
            )}
          </Card>

          </div>

          {/* ── Geography ────────────────────────────────────────────────────── */}
          <div className={sectionClass("geography")}>
          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Membership geography</CardTitle></CardHeader>
            <ZipGeographyMap currentFy={ins.current_fy} capability={ins.capabilities.find((capability) => capability.key === "geography")} builtAt={ins.built_at ?? ""} initial={ins.geography ?? undefined} mode={geoMode} />
          </Card>
          </div>
          </div>
          {/* Named households are screen-only and never enter the PDF report. */}
          <Card className="insights-screen-only" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Named Watch List</CardTitle></CardHeader>
            {!risk || !risk.available ? (
              <Lede>The named Watch List appears only when a model passes validation. There is no validated ranking right now, so no household names are produced.</Lede>
            ) : missing("Name") ? <Unavailable column="Name" /> : (
              <>
                <Lede>A review queue, not a prediction. Each listed household is in the model's top risk decile and shows at least two independent classes of current or recent evidence. Loading names is recorded in the audit log.</Lede>
                {watch === null ? (
                  <Button variant="secondary" disabled={busy !== null} onClick={() => void loadWatch()}>
                    {busy === "watch" ? "Loading…" : `Load named Watch List (${fmt(risk.watch_list_count)})`}
                  </Button>
                ) : !watch.available ? (
                  <Lede>No validated household ranking is available.</Lede>
                ) : (
                  <>
                    <p style={{ margin: "0 0 var(--space-3)", fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
                      Model period {watch.model_first_fy != null ? fyLabel(watch.model_first_fy) : "—"}–{watch.model_last_fy != null ? fyLabel(watch.model_last_fy) : "—"} · confidence (ROC-AUC) {watch.confidence.toFixed(3)} · comparison baseline {(watch.baseline_rate * 100).toFixed(1)}% churn
                    </p>
                    <TableView rows={watch.rows} getRowKey={(r) => r.account_id} empty="No household met the evidence gate." columns={[
                      { key: "n", header: "Household", render: (r) => r.name },
                      { key: "c", header: "Relative rank", align: "right", render: (r) => `${(r.score * 100).toFixed(0)}` },
                      { key: "e", header: "Observed evidence", render: (r) => <span style={{ display: "inline-flex", gap: 4, flexWrap: "wrap" }}>{r.evidence.map((ev) => <Badge key={ev.class} tone="warning" title={ev.detail}>{EVIDENCE_LABELS[ev.class] ?? ev.class}</Badge>)}</span> },
                    ]} />
                    <div style={{ marginTop: "var(--space-3)" }}>
                      <Button size="sm" variant="secondary" disabled={busy !== null} onClick={() => void doWatchExport()}>
                        {busy === "watchexport" ? "Exporting…" : "Export Watch List CSV"}
                      </Button>
                    </div>
                  </>
                )}
              </>
            )}
          </Card>
        </>
      )}
    </div>
  );
}
