import { useEffect, useMemo, useRef, useState } from "react";
import { neighborhoodGeography, zipGeographyYears, type GeoMode, type Segment, type SourceCapability, type ZipGeoCell, type ZipGeography } from "../../api";
import { Stat } from "../../design-system/ui-kits/grant-management/chrome.jsx";
import { NeighborhoodRetentionMap } from "./NeighborhoodRetentionMap";
import { fmt, fyLabel } from "./format";

// Executive framing of ZIP geography: a standing header (where members are, at a glance) sits
// above the mode toggle and stays constant across modes; each mode then opens with a plain-
// language takeaway and the ranked detail. "Where members are" carries the new-member ratio per
// ZIP, so a separate provenance mode is not offered.

const POS = "#1e4785";
const NEG = "#b91c1c";
const GAIN = "#047857";
// Gold: the slice of a "Where members are" bar that joined this fiscal year. Its width within a
// bar is that area's new-member share; across bars its absolute width tracks new-member volume.
const NEW = "#d4a017";

type ModeConfig = { tab: string; measureLabel: string };
const MODES: Record<GeoMode, ModeConfig> = {
  density: { tab: "Where members are", measureLabel: "Member households" },
  provenance: { tab: "New members", measureLabel: "New members" },
  net_change: { tab: "Growth & decline", measureLabel: "Net change" },
  attrition: { tab: "Attrition", measureLabel: "Attrition rate" },
  retention: { tab: "Retention by area", measureLabel: "Retained to date" },
};
// provenance is folded into density (the per-ZIP new-member ratio), so it is not offered as a
// standalone mode; its MODES entry is kept only to satisfy the GeoMode enum the backend defines.
const MODE_ORDER: GeoMode[] = ["density", "retention", "net_change", "attrition"];

const SEGMENT_ALL = "";
const encodeSegment = (seg: Segment | null): string => (seg == null ? SEGMENT_ALL : `${seg.kind}:${seg.value}`);
const pctOf = (part: number, whole: number) => (whole > 0 ? Math.round((part / whole) * 1000) / 10 : 0);

// Session cache of loaded ZIP-geography views, so revisiting one (flipping a mode back,
// or leaving and re-entering the Insights page) paints from memory instead of re-running
// the full household load + billing-ZIP join on the backend. The mirror only changes on a
// rebuild, which changes `built_at`; keying by it means a rebuild misses and everything
// else hits. Kept at module scope so it survives the panel unmounting, mirroring the
// InsightsPage `snapshot`. Bounded in practice by (built_at × 5 modes × ~6 years × segments).
const geoCache = new Map<string, ZipGeography>();
// Requests still in flight, so concurrent asks for the same view (StrictMode's doubled mount
// effect, a rapid re-click) share one backend call instead of queuing twice on the store lock.
const geoInflight = new Map<string, Promise<ZipGeography>>();
const geoKey = (builtAt: string, fy: number, mode: GeoMode, segment: Segment | null) =>
  `${builtAt}|${mode}|${fy}|${encodeSegment(segment)}`;
/** Fetch one geography view (density / growth / attrition), rolled up to neighborhoods; served
 *  from `geoCache` on a built_at + selection hit. Retention keeps its own ZIP-level path. */
function loadGeo(builtAt: string, fy: number, mode: GeoMode, segment: Segment | null): Promise<ZipGeography> {
  const k = geoKey(builtAt, fy, mode, segment);
  const hit = geoCache.get(k);
  if (hit) return Promise.resolve(hit);
  const pending = geoInflight.get(k);
  if (pending) return pending;
  const p = neighborhoodGeography(fy, mode, segment)
    .then((d) => { geoCache.set(k, d); return d; })
    .finally(() => { geoInflight.delete(k); });
  geoInflight.set(k, p);
  return p;
}
const geoYearsInflight = new Map<string, Promise<ZipGeography[]>>();
/** Fetch one mode × segment across many years in ONE backend call, in request order. Every
 *  year that comes back seeds the per-view cache, so a later single-view ask is a hit too. */
function loadGeoYears(builtAt: string, mode: GeoMode, segment: Segment | null, years: number[]): Promise<ZipGeography[]> {
  const hits = years.map((y) => geoCache.get(geoKey(builtAt, y, mode, segment)));
  if (hits.every((h) => h !== undefined)) return Promise.resolve(hits as ZipGeography[]);
  const k = `${builtAt}|${mode}|${years.join(",")}|${encodeSegment(segment)}`;
  const pending = geoYearsInflight.get(k);
  if (pending) return pending;
  const p = zipGeographyYears(mode, segment, years)
    .then((views) => { for (const v of views) geoCache.set(geoKey(builtAt, v.fiscal_year, mode, segment), v); return views; })
    .finally(() => { geoYearsInflight.delete(k); });
  geoYearsInflight.set(k, p);
  return p;
}
/** Test hook: clear the session geography cache so each case starts cold. */
export function _resetGeoCache() { geoCache.clear(); geoInflight.clear(); geoYearsInflight.clear(); }

/** A ranked row with a proportional bar. `frac` is 0..1 of the widest bar in its group. `sub2`,
 *  when present, renders on a second line under the value — used for the per-area new-member ratio.
 *  `newFrac` (0..1), when set, paints a gold segment over the right of the bar sized to that share,
 *  so the "new" portion is visible at a glance. */
function BarRow({ zip, frac, color, value, sub, sub2, newFrac }: { zip: string; frac: number; color: string; value: string; sub?: string; sub2?: string; newFrac?: number }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "minmax(0, 1fr) minmax(56px, 1.7fr) 132px", alignItems: "center", gap: "var(--space-3)", fontSize: "var(--text-sm)", padding: "var(--space-1) 0" }}>
      <span style={{ justifySelf: "start", maxWidth: "100%", padding: "2px 9px", borderRadius: 999, background: "var(--bg-secondary)", border: "1px solid var(--border-default)", color: "var(--text-primary)", fontWeight: "var(--font-medium)", fontSize: "var(--text-xs)", lineHeight: 1.35 }}>{zip}</span>
      <span style={{ display: "block", height: 18, background: "var(--bg-secondary)", borderRadius: 4, overflow: "hidden" }}>
        <span style={{ position: "relative", display: "block", height: "100%", width: `${Math.max(2, frac * 100)}%`, background: color, borderRadius: 4 }}>
          {newFrac != null && newFrac > 0 && (
            <span title="joined this fiscal year" style={{ position: "absolute", top: 0, bottom: 0, right: 0, width: `${Math.min(100, newFrac * 100)}%`, background: NEW, borderTopRightRadius: 4, borderBottomRightRadius: 4 }} />
          )}
        </span>
      </span>
      <span style={{ textAlign: "right", fontVariantNumeric: "tabular-nums" }}>
        <span style={{ display: "block" }}>
          <span style={{ fontWeight: "var(--font-semibold)", color: "var(--text-primary)" }}>{value}</span>
          {sub && <span style={{ marginLeft: 6, color: "var(--text-tertiary)", fontSize: "var(--text-xs)" }}>{sub}</span>}
        </span>
        {sub2 && <span style={{ display: "block", marginTop: 2, color: "var(--text-tertiary)", fontSize: "var(--text-xs)" }}>{sub2}</span>}
      </span>
    </div>
  );
}

function SectionTitle({ children, tone }: { children: React.ReactNode; tone?: string }) {
  return <div style={{ fontSize: "var(--text-xs)", fontWeight: "var(--font-semibold)", letterSpacing: "var(--tracking-wider)", textTransform: "uppercase", color: tone ?? "var(--text-tertiary)", margin: "var(--space-2) 0 var(--space-1)" }}>{children}</div>;
}

function Headline({ children }: { children: React.ReactNode }) {
  return <p style={{ margin: "0 0 var(--space-3)", fontSize: "var(--text-base)", lineHeight: "var(--leading-relaxed)", color: "var(--text-primary)" }}>{children}</p>;
}

export function ZipGeographyMap({ currentFy, capability, builtAt, initial }: { currentFy: number; capability?: SourceCapability; builtAt: string; initial?: ZipGeography }) {
  const available = capability?.available ?? false;
  // The current fiscal year is only weeks/months in, so attrition and net change read as
  // empty; default to the last completed year and let the reader step forward to the live
  // (in-progress) snapshot if they want it.
  const lastCompleteFy = currentFy - 1;
  const [mode, setMode] = useState<GeoMode>("density");
  // Segment filtering was removed from the UI; every view is all-members. Kept as a null
  // constant so the fetch/cache keys and the child maps' props stay unchanged.
  const segment: Segment | null = null;
  const [fy] = useState<number>(lastCompleteFy);
  // Retention has its own multi-cohort trend view (below); the single-fetch model here
  // drives the other four modes.
  const isRetention = mode === "retention";
  const key = `${mode}|${fy}|${encodeSegment(segment)}`;
  // The default view (density · last completed FY · all members) is delivered in the
  // get_insights payload as `initial`, so the panel paints it on first render — no standalone
  // request that would queue behind the risk analysis for the store lock and sit at "Loading…".
  const defaultKey = `density|${lastCompleteFy}|${encodeSegment(null)}`;
  const reqId = useRef(0);
  // `loaded` is tagged with the request key, so the view can only render a response that
  // matches the current selection — a request always resolves to either data or an error
  // for its key, so the view can never get stuck "Loading" forever.
  const [loaded, setLoaded] = useState<{ key: string; data?: ZipGeography; error?: string } | null>(
    () => (initial ? { key: defaultKey, data: initial } : null),
  );
  // The density view (last completed FY, all members) drives the standing header, which stays on
  // screen in every mode. Held separately from `loaded` so switching to another mode never blanks
  // it. Served from the payload/cache when available, so it costs no extra backend call.
  const [densityData, setDensityData] = useState<ZipGeography | undefined>(initial);

  // Seed the session cache with the payload-delivered default so this panel — and a later
  // revisit — serves it from memory instead of re-requesting it across the locked path.
  useEffect(() => {
    if (initial) geoCache.set(geoKey(builtAt, lastCompleteFy, "density", null), initial);
  }, [initial, builtAt, lastCompleteFy]);

  // Keep the standing header's density snapshot current. loadGeo coalesces with the main fetch
  // below and hits the seeded cache, so this adds no second request for the same view.
  useEffect(() => {
    if (!available) return;
    let live = true;
    loadGeo(builtAt, lastCompleteFy, "density", null)
      .then((d) => { if (live) setDensityData(d); })
      .catch(() => {});
    return () => { live = false; };
  }, [available, builtAt, lastCompleteFy]);

  useEffect(() => {
    if (!available || isRetention) return;
    const id = ++reqId.current;
    loadGeo(builtAt, fy, mode, segment)
      .then((d) => { if (reqId.current === id) setLoaded({ key, data: d }); })
      .catch((e) => { if (reqId.current === id) setLoaded({ key, error: String(e) }); });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [available, key, isRetention, builtAt]);

  const fresh = loaded?.key === key;
  const freshData = fresh ? loaded?.data : undefined;
  const freshError = fresh ? loaded?.error : undefined;
  const cells = freshData?.available ? freshData.cells : [];
  const totalN = useMemo(() => cells.reduce((s, c) => s + c.n, 0), [cells]);

  if (!available) {
    return (
      <p style={{ margin: 0, color: "var(--text-secondary)" }}>
        {`Geographic membership insights are unavailable. ${capability?.unavailable_reason ?? "A usable billing-statement or Account postal source is not mirrored."} Other Insights views remain available.`}
      </p>
    );
  }

  const controlBtn = (activeC: boolean): React.CSSProperties => ({
    height: 32, padding: "0 var(--space-3)", display: "inline-flex", alignItems: "center",
    border: "1px solid var(--border-default)", borderRadius: "var(--radius-md)",
    font: "var(--font-semibold) var(--text-sm) var(--font-body)", cursor: "pointer",
    background: activeC ? POS : "var(--bg-primary)", color: activeC ? "#fff" : "var(--text-primary)",
  });

  return (
    <>
      {/* Standing header: where the members are, at a glance — constant across every mode. */}
      {densityData && <GeoHeader density={densityData} />}
      {/* questions, not jargon */}
      <div role="group" aria-label="Map mode" style={{ display: "inline-flex", gap: "var(--space-1)", marginBottom: "var(--space-3)", flexWrap: "wrap" }}>
        {MODE_ORDER.map((m) => (
          <button key={m} type="button" aria-pressed={mode === m} onClick={() => setMode(m)} style={controlBtn(mode === m)}>{MODES[m].tab}</button>
        ))}
      </div>
      {isRetention ? (
        <>
          <NeighborhoodRetentionMap currentFy={currentFy} segment={segment} builtAt={builtAt} />
          <details style={{ marginTop: "var(--space-4)" }}>
            <summary style={{ cursor: "pointer", fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>ZIP-level cohort detail</summary>
            <div style={{ marginTop: "var(--space-3)" }}>
              <RetentionTrend segment={segment} currentFy={currentFy} builtAt={builtAt} />
            </div>
          </details>
        </>
      ) : (
        <>
          <div data-testid="zip-geography-summary" role="region" aria-label={`${MODES[mode].tab} — ${fyLabel(fy)}`}>
            {!fresh ? (
              <p style={{ margin: 0, color: "var(--text-tertiary)" }}>{`Loading ${MODES[mode].measureLabel.toLowerCase()} by area for ${fyLabel(fy)}…`}</p>
            ) : freshError ? (
              <p style={{ margin: 0, color: "var(--color-error-600)" }}>The data could not load: {freshError}</p>
            ) : freshData && !freshData.available ? (
              <p style={{ margin: 0, color: "var(--text-secondary)" }}>No normalizable area data for this view.</p>
            ) : cells.length === 0 ? (
              <p style={{ margin: 0, color: "var(--text-secondary)" }}>No areas meet the reporting threshold for this view{segment ? " and segment" : ""}.</p>
            ) : mode === "net_change" ? (
              <NetView cells={cells} fy={fy} outOfArea={freshData?.out_of_area ?? 0} />
            ) : mode === "attrition" ? (
              <AttritionView cells={cells} fy={fy} totalN={totalN} />
            ) : (
              <WhereMembersAreView cells={cells} fy={fy} totalN={totalN} />
            )}
          </div>

          {fresh && freshData && freshData.suppressed_zips > 0 && (
            <p style={{ margin: "var(--space-3) 0 0", fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>
              {`${freshData.suppressed_zips} smaller ${freshData.suppressed_zips === 1 ? "area is" : "areas are"} hidden to protect households (fewer than ${mode === "attrition" ? 10 : 5} in this view).`}
            </p>
          )}

          {cells.length > 0 && <FullTable mode={mode} cells={cells} totalN={totalN} />}
        </>
      )}
    </>
  );
}

const StatRow = ({ children }: { children: React.ReactNode }) => (
  <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))", gap: "var(--space-3)", marginBottom: "var(--space-3)" }}>{children}</div>
);

/** The standing Geography header: the four density KPIs (where members are, at a glance), shown
 *  above the mode toggle and unchanged as the reader flips modes. Renders nothing until the
 *  density snapshot is available or when it holds no mapped areas. */
function GeoHeader({ density }: { density: ZipGeography }) {
  const cells = density.available ? density.cells : [];
  if (cells.length === 0) return null;
  const ranked = [...cells].sort((a, b) => b.n - a.n);
  const totalN = ranked.reduce((s, c) => s + c.n, 0);
  const top = ranked[0];
  const top5 = ranked.slice(0, 5).reduce((s, c) => s + c.n, 0);
  const outOfArea = density.out_of_area;
  return (
    <StatRow>
      <Stat label="Mapped households" value={fmt(totalN)} sub={outOfArea > 0 ? `${fmt(outOfArea)} outside NY` : "on the map"} icon="users" tone="primary" />
      <Stat label="Top area" value={top.zip} sub={`${fmt(top.n)} · ${pctOf(top.n, totalN)}% of total`} icon="map-pin" tone="accent" />
      <Stat label="Top 5 areas" value={`${pctOf(top5, totalN)}%`} sub="share of the total" icon="target" tone="primary" />
      <Stat label="Areas shown" value={fmt(ranked.length)} sub="above the threshold" icon="list" tone="neutral" />
    </StatRow>
  );
}

/** "Where members are" detail: the ranked neighborhood list, each row carrying its member
 *  households and the new members behind them (households that joined in `fy`) as a ratio, so the
 *  reader sees which areas are refreshing versus aging. The KPI tiles live in the header above. */
function WhereMembersAreView({ cells, fy, totalN }: { cells: ZipGeoCell[]; fy: number; totalN: number }) {
  const ranked = [...cells].sort((a, b) => b.n - a.n);
  const top = ranked[0];
  const top5 = ranked.slice(0, 5).reduce((s, c) => s + c.n, 0);
  const topNew = [...cells].sort((a, b) => b.joins - a.joins)[0];
  const maxN = top?.n ?? 1;
  return (
    <>
      <Headline>
        Your members are clustered in <strong>{top.zip}</strong> ({fmt(top.n)} households, {pctOf(top.n, totalN)}%), and the top 5 areas make up <strong>{pctOf(top5, totalN)}%</strong> of all members.{" "}
        {topNew && topNew.joins > 0 && <>Most new members in {fyLabel(fy)} {topNew.zip === top.zip ? "also " : ""}came from <strong>{topNew.zip}</strong> ({fmt(topNew.joins)}).</>}
      </Headline>
      <SectionTitle>Top areas by member households</SectionTitle>
      <div style={{ display: "flex", alignItems: "center", gap: 6, margin: "0 0 var(--space-2)", fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>
        <span style={{ display: "inline-block", width: 22, height: 8, borderRadius: 2, background: NEW }} />
        <span>Gold marks the share who joined in {fyLabel(fy)}</span>
      </div>
      {ranked.slice(0, 10).map((c) => (
        <BarRow key={c.zip} zip={c.zip} frac={c.n / maxN} color={POS} value={fmt(c.n)} sub="members"
          sub2={c.joins > 0 ? `${fmt(c.joins)} new · ${pctOf(c.joins, c.n)}%` : "no new members"}
          newFrac={c.n > 0 ? c.joins / c.n : 0} />
      ))}
    </>
  );
}

function NetView({ cells, fy, outOfArea }: { cells: ZipGeoCell[]; fy: number; outOfArea: number }) {
  const net = cells.reduce((s, c) => s + c.measure, 0);
  const losers = cells.filter((c) => c.measure < 0).sort((a, b) => a.measure - b.measure);
  const gainers = cells.filter((c) => c.measure > 0).sort((a, b) => b.measure - a.measure);
  const worst = losers[0];
  const best = gainers[0];
  const maxLoss = Math.max(1, ...losers.map((c) => -c.measure));
  const maxGain = Math.max(1, ...gainers.map((c) => c.measure));
  return (
    <>
      <StatRow>
        <Stat label={`Net change ${fyLabel(fy)}`} value={`${net > 0 ? "+" : ""}${fmt(net)}`} sub="joins − exits, mapped" icon={net >= 0 ? "trending-up" : "trending-down"} tone={net >= 0 ? "success" : "neutral"} />
        <Stat label="Areas shrinking" value={fmt(losers.length)} sub={worst ? `worst ${worst.zip} (${worst.measure})` : "none"} icon="trending-down" tone="neutral" />
        <Stat label="Areas growing" value={fmt(gainers.length)} sub={best ? `best ${best.zip} (+${best.measure})` : "none"} icon="trending-up" tone="success" />
        {outOfArea > 0 && <Stat label="Outside NY" value={fmt(outOfArea)} sub="not mapped" icon="globe" tone="neutral" />}
      </StatRow>
      <Headline>
        You are net <strong style={{ color: net >= 0 ? GAIN : NEG }}>{net > 0 ? "+" : ""}{fmt(net)}</strong> across mapped areas in {fyLabel(fy)}.{" "}
        {worst ? <>The steepest decline is <strong>{worst.zip}</strong> ({worst.measure}: {worst.joins} joined, {worst.exits} left).</> : <>No area is in net decline.</>}
      </Headline>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))", gap: "var(--space-4)" }}>
        <div>
          <SectionTitle tone={NEG}>Losing ground</SectionTitle>
          {losers.length === 0 ? <p style={{ margin: 0, fontSize: "var(--text-sm)", color: "var(--text-tertiary)" }}>No areas in decline.</p>
            : losers.slice(0, 8).map((c) => <BarRow key={c.zip} zip={c.zip} frac={-c.measure / maxLoss} color={NEG} value={`${c.measure}`} sub={`${c.exits} left`} />)}
        </div>
        <div>
          <SectionTitle tone={GAIN}>Gaining members</SectionTitle>
          {gainers.length === 0 ? <p style={{ margin: 0, fontSize: "var(--text-sm)", color: "var(--text-tertiary)" }}>No areas growing.</p>
            : gainers.slice(0, 8).map((c) => <BarRow key={c.zip} zip={c.zip} frac={c.measure / maxGain} color={GAIN} value={`+${c.measure}`} sub={`${c.joins} joined`} />)}
        </div>
      </div>
    </>
  );
}

function AttritionView({ cells, fy, totalN }: { cells: ZipGeoCell[]; fy: number; totalN: number }) {
  const ranked = [...cells].sort((a, b) => b.measure - a.measure);
  const totalExits = cells.reduce((s, c) => s + c.exits, 0);
  const avg = pctOf(totalExits, totalN);
  const worst = ranked[0];
  const maxRate = Math.max(1, ...ranked.map((c) => c.measure));
  return (
    <>
      <StatRow>
        <Stat label={`Avg attrition ${fyLabel(fy)}`} value={`${avg}%`} sub={`${fmt(totalExits)} of ${fmt(totalN)} left`} icon="activity" tone="neutral" />
        <Stat label="Highest-attrition area" value={worst.zip} sub={`${worst.measure}% · ${worst.exits} of ${worst.n}`} icon="alert-triangle" tone="accent" />
        <Stat label="Areas above average" value={fmt(ranked.filter((c) => c.measure > avg).length)} sub={`worse than ${avg}%`} icon="trending-down" tone="neutral" />
      </StatRow>
      <Headline>
        Average attrition across mapped areas is <strong>{avg}%</strong> in {fyLabel(fy)}, highest in <strong>{worst.zip}</strong> at <strong style={{ color: NEG }}>{worst.measure}%</strong> ({worst.exits} of {worst.n} households).
      </Headline>
      <SectionTitle tone={NEG}>Where attrition is worst</SectionTitle>
      {ranked.slice(0, 10).map((c) => (
        <BarRow key={c.zip} zip={c.zip} frac={c.measure / maxRate} color={NEG} value={`${c.measure}%`} sub={`${c.exits}/${c.n}`} />
      ))}
    </>
  );
}

// Retention heat: red (low) → amber → green (high). Fixed absolute stops so a color means
// the same retention level in every cohort and every area.
const retentionColor = (pct: number): string =>
  pct >= 80 ? "#047857" : pct >= 65 ? "#16a34a" : pct >= 50 ? "#ca8a04" : pct >= 35 ? "#ea580c" : "#b91c1c";

/** Cohort retention by ZIP as a heatmap across many join years at once — rows are areas,
 *  columns are join-year cohorts, each cell shaded by the share still members. Fetches
 *  every shown cohort automatically so the reader sees the trend without clicking through. */
function RetentionTrend({ segment, currentFy, builtAt }: { segment: Segment | null; currentFy: number; builtAt: string }) {
  // The last 8 completed join years, newest → oldest.
  const cohorts = useMemo(() => Array.from({ length: 8 }, (_, i) => currentFy - 1 - i), [currentFy]);
  const segKey = encodeSegment(segment);
  const [state, setState] = useState<{ loading: boolean; error?: string; byZip: Map<string, Record<number, ZipGeoCell>> }>({ loading: true, byZip: new Map() });

  useEffect(() => {
    let live = true;
    setState({ loading: true, byZip: new Map() });
    // All eight cohorts in one backend call: one store-lock hold instead of eight in a row.
    loadGeoYears(builtAt, "retention", segment, cohorts).then((views) => {
      if (!live) return;
      const byZip = new Map<string, Record<number, ZipGeoCell>>();
      for (const d of views) {
        if (!d.available) continue;
        for (const c of d.cells) {
          const row = byZip.get(c.zip) ?? {};
          row[d.fiscal_year] = c;
          byZip.set(c.zip, row);
        }
      }
      setState({ loading: false, byZip });
    }).catch((e) => { if (live) setState({ loading: false, error: String(e), byZip: new Map() }); });
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [segKey, currentFy, builtAt]);

  // The largest areas (by total cohort households across the shown years), top 14.
  const rows = useMemo(() => {
    const entries = [...state.byZip.entries()].map(([zip, byCohort]) => {
      const vals = Object.values(byCohort);
      const total = vals.reduce((s, c) => s + c.n, 0);
      const retd = vals.reduce((s, c) => s + c.retained, 0);
      return { zip, byCohort, total, avg: total > 0 ? Math.round((retd / total) * 100) : 0 };
    });
    entries.sort((a, b) => b.total - a.total);
    return entries.slice(0, 14);
  }, [state.byZip]);

  const region = (children: React.ReactNode) => (
    <div role="region" aria-label="Retention by area — cohort trend" data-testid="zip-retention-trend">{children}</div>
  );
  if (state.loading) return region(<p style={{ margin: 0, color: "var(--text-tertiary)" }}>{`Computing retention by ZIP for the ${fyLabel(cohorts[cohorts.length - 1])}–${fyLabel(cohorts[0])} cohorts…`}</p>);
  if (state.error) return region(<p style={{ margin: 0, color: "var(--color-error-600)" }}>The data could not load: {state.error}</p>);
  if (rows.length === 0) return region(<p style={{ margin: 0, color: "var(--text-secondary)" }}>No cohort × area cells meet the reporting threshold (fewer than 10 households each).</p>);

  const weakest = [...rows].sort((a, b) => a.avg - b.avg)[0];
  const strongest = [...rows].sort((a, b) => b.avg - a.avg)[0];
  const cellBox: React.CSSProperties = { display: "block", minWidth: 42, padding: "4px 6px", borderRadius: 4, fontVariantNumeric: "tabular-nums", fontSize: "var(--text-xs)" };

  return region(
    <>
      <Headline>
        Each column is a join-year cohort, each row an area; the cell is the share still members today.{" "}
        Across these cohorts, retention is weakest in <strong>{weakest.zip}</strong> (<strong style={{ color: NEG }}>{weakest.avg}%</strong>) and strongest in <strong>{strongest.zip}</strong> ({strongest.avg}%).
      </Headline>
      <div style={{ overflowX: "auto" }}>
        <table data-testid="zip-retention-table" style={{ borderCollapse: "collapse", fontSize: "var(--text-sm)" }}>
          <thead>
            <tr>
              <th style={{ textAlign: "left", padding: "var(--space-1) var(--space-2)", position: "sticky", left: 0, background: "var(--bg-primary)" }}>Area</th>
              {cohorts.map((cy) => <th key={cy} style={{ padding: "var(--space-1) var(--space-2)", textAlign: "center", color: "var(--text-tertiary)", fontSize: "var(--text-xs)" }}>{fyLabel(cy)}</th>)}
              <th style={{ padding: "var(--space-1) var(--space-2)", textAlign: "right", color: "var(--text-tertiary)", fontSize: "var(--text-xs)" }}>Avg</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.zip}>
                <td style={{ padding: "var(--space-1) var(--space-2)", fontVariantNumeric: "tabular-nums", position: "sticky", left: 0, background: "var(--bg-primary)" }}>{r.zip}</td>
                {cohorts.map((cy) => {
                  const c = r.byCohort[cy];
                  return (
                    <td key={cy} title={c ? `${fyLabel(cy)} cohort · ${c.retained} of ${c.n} still members` : "fewer than 10 households"} style={{ padding: 2, textAlign: "center" }}>
                      {c
                        ? <span style={{ ...cellBox, background: retentionColor(c.measure), color: "#fff" }}>{Math.round(c.measure)}%</span>
                        : <span style={{ ...cellBox, color: "var(--text-tertiary)" }}>—</span>}
                    </td>
                  );
                })}
                <td style={{ padding: "var(--space-1) var(--space-2)", textAlign: "right", fontVariantNumeric: "tabular-nums", fontWeight: "var(--font-semibold)" }}>{r.avg}%</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", marginTop: "var(--space-2)", fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}>
        <span>Lower</span>
        {["#b91c1c", "#ea580c", "#ca8a04", "#16a34a", "#047857"].map((c) => <span key={c} style={{ width: 18, height: 10, borderRadius: 2, background: c, display: "inline-block" }} />)}
        <span>Higher retention</span>
        <span style={{ marginLeft: "auto" }}>Blank = fewer than 10 households in that cohort × area</span>
      </div>
    </>,
  );
}

function FullTable({ mode, cells, totalN }: { mode: GeoMode; cells: ZipGeoCell[]; totalN: number }) {
  const ranked = mode === "retention"
    ? [...cells].sort((a, b) => a.measure - b.measure)
    : [...cells].sort((a, b) => Math.abs(b.measure) - Math.abs(a.measure));
  const flux = mode === "net_change" || mode === "attrition";
  const val = (c: ZipGeoCell) =>
    mode === "attrition" || mode === "retention" ? `${c.measure}%` : mode === "net_change" ? `${c.measure > 0 ? "+" : ""}${c.measure}` : fmt(c.n);
  return (
    <details style={{ marginTop: "var(--space-4)" }}>
      <summary style={{ cursor: "pointer", fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>All {ranked.length} areas</summary>
      <div style={{ marginTop: "var(--space-2)", overflowX: "auto" }}>
        <table data-testid="zip-geography-table" style={{ width: "100%", borderCollapse: "collapse", fontSize: "var(--text-sm)" }}>
          <thead>
            <tr>
              <th style={{ textAlign: "left" }}>Area</th>
              <th style={{ textAlign: "right" }}>{MODES[mode].measureLabel}</th>
              {mode !== "net_change" && mode !== "attrition" && mode !== "retention" && <th style={{ textAlign: "right" }}>Share</th>}
              {mode === "retention" && <th style={{ textAlign: "right" }}>Still members</th>}
              {flux && <th style={{ textAlign: "right" }}>Joins</th>}
              {flux && <th style={{ textAlign: "right" }}>Exits</th>}
              <th style={{ textAlign: "right" }}>{mode === "retention" ? "Cohort" : "Households"}</th>
            </tr>
          </thead>
          <tbody>
            {ranked.map((c) => (
              <tr key={c.zip} style={{ borderTop: "1px solid var(--border-subtle)" }}>
                <td style={{ fontVariantNumeric: "tabular-nums" }}>{c.zip}</td>
                <td style={{ textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{val(c)}</td>
                {mode !== "net_change" && mode !== "attrition" && mode !== "retention" && <td style={{ textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{pctOf(c.n, totalN)}%</td>}
                {mode === "retention" && <td style={{ textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{c.retained}</td>}
                {flux && <td style={{ textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{c.joins}</td>}
                {flux && <td style={{ textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{c.exits}</td>}
                <td style={{ textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{c.n}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </details>
  );
}
