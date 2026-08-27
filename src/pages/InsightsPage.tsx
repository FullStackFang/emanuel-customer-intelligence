import { useCallback, useEffect, useRef, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Alert, Badge, Button, Card, CardHeader, CardTitle, EmptyState, Icon, MenuButton } from "../design-system";
import { PageTitle, Stat } from "../design-system/ui-kits/grant-management/chrome.jsx";
import "./insights/print.css";
import { CohortHeatmap, DuesChart, FlowsChart, HBarChart, OutcomeByTenureChart, ReasonsHeatmap, TableView, TrendChart, Year1Chart } from "./insights/charts";
import { ZipGeographyMap } from "./insights/ZipGeographyMap";
import { EVIDENCE_LABELS, fmt, fyLabel, soWhat } from "./insights/format";

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
  multi_job: "Retention by number of jobs",
  outcome_by_tenure: "Exit outcomes by tenure",
  school_progression: "Nursery to religious school",
  school_gap: "Churn since religious school",
  dues: "Dues renewal state",
  anchor_type: "Retention by anchor type",
  anchor_count: "Retention by anchor count",
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

export default function InsightsPage({ status }: PageProps) {
  const [ins, setIns] = useState<api.Insights | null>(snapshot?.ins ?? null);
  const [risk, setRisk] = useState<api.RiskSummary | null>(snapshot?.risk ?? null);
  const [riskFailed, setRiskFailed] = useState(snapshot?.riskFailed ?? false);
  const [watch, setWatch] = useState<api.WatchListView | null>(null);
  const [tab, setTab] = useState<"overview" | "jobs" | "renewal" | "geography" | "risk">("overview");
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [exported, setExported] = useState<string | null>(null);
  const [progress, setProgress] = useState<api.InsightsProgress | null>(null);
  const [riskBusy, setRiskBusy] = useState(false);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [renderingPdf, setRenderingPdf] = useState(false);
  const pdfReportRef = useRef<HTMLDivElement>(null);

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
  const built = ins?.built_at ? new Date(ins.built_at).toLocaleString() : "not built";
  const anyAnchor = capOn("renewal") || capOn("school") || capOn("committee");
  const sectionClass = (key: typeof tab) => `insights-section${renderingPdf || tab === key ? "" : " insights-section-hidden"}`;
  const rebuilding = progress?.job === "rebuild" || busy === "rebuild";
  const rebuildProgress = progress?.job === "rebuild" ? progress : null;
  const riskProgress = progress?.job === "risk" ? progress : null;

  return (
    <div style={{ width: "100%", maxWidth: 1180, margin: "0 auto" }}>
      <div className="insights-screen-only">
        <PageTitle eyebrow="Customer Intelligence" title="Insights" actions={
          <MenuButton disabled={busy !== null} items={[
            { key: "pdf", label: "Download PDF report", icon: "file-text", disabled: !ins, onSelect: () => void doPdf() },
            { divider: true },
            ...api.INSIGHT_VIEWS.map((v) => ({ key: v, label: `CSV · ${VIEW_LABELS[v]}`, icon: "table", disabled: !ins, onSelect: () => void doExport(v) })),
            { divider: true },
            { key: "rebuild", label: "Rebuild insights", icon: "refresh-cw", onSelect: () => void load(true) },
          ]}>
            {busy === "pdf" ? "Rendering…" : busy === "export" ? "Exporting…" : busy === "rebuild" ? "Rebuilding…" : "Export"}
          </MenuButton>
        } />
        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-tertiary)", marginTop: "calc(-1 * var(--space-4))", marginBottom: "var(--space-4)" }}>
          Built {built} · fiscal years run June 1 – May 31 and are labeled by the year they end
          {riskBusy && (
            <span style={{ marginLeft: "var(--space-2)", display: "inline-flex", alignItems: "center", gap: "var(--space-1)" }}>
              · <span className="app-spinner" style={{ width: "0.8em", height: "0.8em" }} aria-hidden />
              {riskProgress ? `Risk analysis: step ${riskProgress.step} of ${riskProgress.steps} · ${elapsed}` : `Risk analysis running · ${elapsed}`}
            </span>
          )}
        </div>
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
          <div style={{ display: "grid", gridTemplateColumns: "repeat(5, 1fr)", gap: "var(--space-3)", marginBottom: "var(--space-5)" }}>
            <Stat label="Member households" value={fmt(ins.kpis.members_now)} sub={`${ins.kpis.net_vs_prior_fy >= 0 ? "+" : ""}${fmt(ins.kpis.net_vs_prior_fy)} vs ${fyLabel(ins.current_fy - 1)} · ${fyLabel(ins.current_fy)} in progress`} icon="users" tone="primary" />
            <Stat label={`Joins ${fyLabel(ins.current_fy)}`} value={fmt(ins.kpis.joins_this_fy)} sub="fiscal year to date" icon="user-plus" tone="success" />
            <Stat label={`Resignations ${fyLabel(ins.current_fy)}`} value={fmt(ins.kpis.resigns_this_fy)} sub="fiscal year to date" icon="user-minus" tone="neutral" />
            <Stat label="First-year retention" value={`${ins.kpis.year1_pct}%`} sub={`${fyLabel(ins.kpis.year1_cohort)} cohort · baseline ${ins.kpis.year1_baseline_pct}%`} icon="repeat" tone="primary" />
            <Stat label="Households at risk" value={fmt(ins.kpis.at_risk_count)} sub="current members matching a churn pattern" icon="triangle-alert" tone="accent" />
          </div>

          <Card className="insights-screen-only" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Lifecycle data coverage</CardTitle></CardHeader>
            <Lede>{ins.stale && !rebuilding ? "The local analysis is older than a source sync; it rebuilds automatically the next time Insights loads." : "Source availability is checked independently; unavailable sources are never treated as household behavior."}</Lede>
            <div style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap" }}>
              {ins.capabilities.map((capability) => (
                <Badge key={capability.key} tone={capability.available ? "success" : "neutral"}>
                  {capability.key}: {capability.available ? `${capability.mirrored_columns.length} fields available` : "not synced"}
                </Badge>
              ))}
            </div>
            {ins.capabilities.filter((capability) => !capability.available).map((capability) => (
              <p key={capability.key} style={{ margin: "var(--space-2) 0 0", fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
                {capability.unavailable_reason}
              </p>
            ))}
            {ins.capabilities.filter((capability) => capability.available).map((capability) => (
              <details key={`${capability.key}-fields`} style={{ marginTop: "var(--space-2)", fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
                <summary>{capability.key} synced fields</summary>
                <p style={{ margin: "var(--space-1) 0 0", fontFamily: "var(--font-mono)", overflowWrap: "anywhere" }}>
                  {capability.mirrored_columns.join(", ")}
                </p>
              </details>
            ))}
          </Card>

          <div className="insights-screen-only" role="tablist" aria-label="Insights sections" style={{ display: "flex", gap: "var(--space-2)", flexWrap: "wrap", marginBottom: "var(--space-4)" }}>
            {(["overview", "jobs", "renewal", "geography", "risk"] as const).map((key) => (
              <Button key={key} size="sm" variant={tab === key ? "primary" : "secondary"} onClick={() => setTab(key)}>
                {key === "overview" ? "Overview" : key === "jobs" ? "Jobs" : key === "renewal" ? "Renewal & Engagement" : key === "geography" ? "Geography" : "Risk"}
              </Button>
            ))}
          </div>

          {/* ── Overview ─────────────────────────────────────────────────────── */}
          <div className={renderingPdf || tab === "overview" ? "insights-overview" : "insights-overview insights-overview-hidden"}>
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
          </div>

          {/* ── Jobs ─────────────────────────────────────────────────────────── */}
          <div className={sectionClass("jobs")}>
          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Stickiness by Entry Job</CardTitle></CardHeader>
            {missing("Join_Reason__c") ? <Unavailable column="Join_Reason__c" /> : (
              <>
                <Lede>Joining reasons are stated Entry Job evidence. Households that joined 4–12 fiscal years ago; share still members today, counted under every reason they named. Associations, not claims about intent or causation. School-driven reasons are highlighted.</Lede>
                <HBarChart rows={ins.channels.map((c) => bar(c.label, c.pct, c.n, c.still_members))} emphasize={["Religious school", "Nursery school"]} />
                <SoWhat text={s!.channels} />
                <TableView rows={ins.channels} getRowKey={(r) => r.key} columns={[
                  { key: "l", header: "Entry Job", render: (r) => r.label },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                  { key: "s", header: "Still members", align: "right", render: (r) => fmt(r.still_members) },
                  { key: "p", header: "Share", align: "right", render: (r) => `${r.pct}%` },
                  { key: "t", header: "Avg tenure (yrs)", align: "right", render: (r) => r.avg_tenure },
                ]} />
              </>
            )}
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Retention by number of Entry Jobs</CardTitle></CardHeader>
            {missing("Join_Reason__c") ? <Unavailable column="Join_Reason__c" /> : (
              <>
                <Lede>Households grouped by how many joining reasons they stated. More stated reasons is an association with retention, not evidence of stronger intent.</Lede>
                <HBarChart rows={ins.multi_job.map((r) => bar(r.bucket, r.pct, r.n, r.still_members))} />
                <TableView rows={ins.multi_job} getRowKey={(r) => r.bucket} columns={[
                  { key: "b", header: "Stated jobs", render: (r) => r.bucket },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                  { key: "s", header: "Still members", align: "right", render: (r) => fmt(r.still_members) },
                  { key: "p", header: "Share", align: "right", render: (r) => `${r.pct}%` },
                  { key: "t", header: "Avg tenure (yrs)", align: "right", render: (r) => r.avg_tenure },
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
          </div>

          {/* ── Renewal & Engagement ─────────────────────────────────────────── */}
          <div className={sectionClass("renewal")}>
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

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Retention by Relationship Anchor</CardTitle></CardHeader>
            {!anyAnchor ? <Unavailable column="BillingStatement__c, Class_Enrolment__c, or Committee_Membership__c" /> : (
              <>
                <Lede>Households that held each observed anchor — dues renewal, nursery school, religious school, or committee service — in a recent fiscal year, and the share still active now. Only anchors from an available source appear.</Lede>
                <HBarChart rows={ins.anchor_type.map((a) => bar(a.label, a.pct, a.n, a.still_members))} />
                <TableView rows={ins.anchor_type} getRowKey={(r) => r.key} columns={[
                  { key: "l", header: "Anchor", render: (r) => r.label },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                  { key: "s", header: "Still active", align: "right", render: (r) => fmt(r.still_members) },
                  { key: "p", header: "Share", align: "right", render: (r) => `${r.pct}%` },
                ]} />
              </>
            )}
          </Card>

          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Retention by number of anchors</CardTitle></CardHeader>
            {!anyAnchor ? <Unavailable column="an anchor source" /> : (
              <>
                <Lede>Households grouped by the most distinct Relationship Anchors they held in a recent fiscal year. Holding more anchors is associated with, not proven to cause, higher retention.</Lede>
                <HBarChart rows={ins.anchor_count.map((a) => bar(a.label, a.pct, a.n, a.still_members))} />
                <TableView rows={ins.anchor_count} getRowKey={(r) => String(r.anchors)} columns={[
                  { key: "l", header: "Anchors held", render: (r) => r.label },
                  { key: "n", header: "Households", align: "right", render: (r) => fmt(r.n) },
                  { key: "s", header: "Still active", align: "right", render: (r) => fmt(r.still_members) },
                  { key: "p", header: "Share", align: "right", render: (r) => `${r.pct}%` },
                ]} />
              </>
            )}
          </Card>

          </div>

          {/* ── Geography ────────────────────────────────────────────────────── */}
          <div className={sectionClass("geography")}>
          <Card className="insights-report-card" style={{ marginBottom: "var(--space-4)" }}>
            <CardHeader><CardTitle>Membership geography</CardTitle></CardHeader>
            <ZipGeographyMap currentFy={ins.current_fy} capability={ins.capabilities.find((capability) => capability.key === "geography")} builtAt={ins.built_at ?? ""} initial={ins.geography ?? undefined} />
          </Card>
          </div>

          {/* ── Risk ─────────────────────────────────────────────────────────── */}
          <div className={sectionClass("risk")}>
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
              <>
                <Lede>A regularized logistic model of Addressable Churn passed rolling historical validation. Scores rank current households; they are associations from history, not predictions that any household will resign.</Lede>
                <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: "var(--space-3)", marginBottom: "var(--space-3)" }}>
                  <Stat label="ROC-AUC" value={risk.roc_auc.toFixed(3)} sub="discrimination (≥ 0.65)" icon="activity" tone="primary" />
                  <Stat label="Top-decile lift" value={`${risk.top_decile_lift.toFixed(2)}×`} sub="vs base rate (≥ 2.0)" icon="trending-up" tone="success" />
                  <Stat label="Brier score" value={risk.brier.toFixed(4)} sub={`baseline ${risk.baseline_brier.toFixed(4)}`} icon="target" tone="neutral" />
                  <Stat label="Watch List" value={fmt(risk.watch_list_count)} sub="evidence-gated households" icon="list-checks" tone="accent" />
                </div>
              </>
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
