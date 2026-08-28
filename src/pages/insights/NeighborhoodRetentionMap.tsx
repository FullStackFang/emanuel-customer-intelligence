import { useEffect, useMemo, useRef, useState } from "react";
import * as maplibregl from "maplibre-gl";
import type { ExpressionSpecification, GeoJSONSource, LayerSpecification, MapLayerMouseEvent } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import ntaMeta from "../../assets/nta-meta.json";
import { neighborhoodRetentionYears, type NeighborhoodCell, type Segment } from "../../api";
import { fyLabel } from "./format";

// Cohort retention rolled up to NYC neighborhoods, drawn as the packaged census-block mosaic
// (public/map/blocks.geojson; each block's `i` = its neighborhood index) over a light CARTO
// Positron basemap. Only member neighborhoods are painted — parks, water, and non-member
// blocks stay the plain basemap, so Central Park never lights up and the streets read through
// the tint (the mosaic is inserted beneath the basemap's road lines). Neighborhood identity is
// resolved here from the packaged `nta-meta.json`; the backend only ever sends an index.

type Meta = { name: string; boro: string; c: [number, number]; bb: [number, number, number, number] };
const META = ntaMeta as Meta[];

const BASEMAP = "https://basemaps.cartocdn.com/gl/positron-gl-style/style.json";
const BLOCKS_URL = "/map/blocks.geojson";
const NONE = "rgba(0,0,0,0)";

// Soft pastel retention ramp in the app's semantic hues (success → amber → error), muted so it
// sits calmly on the pale basemap. Same absolute stops as the ZIP heatmap, so a colour means
// the same retention level everywhere.
const color = (p: number | null): string =>
  p == null ? NONE
    : p >= 80 ? "#7fbfa2" : p >= 65 ? "#b6dca2" : p >= 50 ? "#f2d98f" : p >= 35 ? "#eeb389" : "#e29a95";
const RAMP = ["#e29a95", "#eeb389", "#f2d98f", "#b6dca2", "#7fbfa2"];
const tone = (p: number): string => (p >= 65 ? "var(--color-success-600)" : p >= 50 ? "var(--color-warning-600)" : "var(--color-error-600)");
const yr = (fy: number) => "’" + String(fy).slice(2);

type CohortMap = Map<number, NeighborhoodCell>; // nta index -> cell, for one cohort year
type CohortSel = number | "all"; // rail selection: a single join-year cohort, or every cohort blended

export function NeighborhoodRetentionMap({ currentFy, segment, builtAt }: { currentFy: number; segment: Segment | null; builtAt: string }) {
  // The last 8 completed join-year cohorts, newest → oldest (retention needs a past cohort).
  const cohorts = useMemo(() => Array.from({ length: 8 }, (_, i) => currentFy - 1 - i), [currentFy]);
  const [cohortFy, setCohortFy] = useState<CohortSel>("all"); // land on the all-members blend, not one cohort
  const [byCohort, setByCohort] = useState<Map<number, CohortMap> | null>(null);
  const [available, setAvailable] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selNta, setSelNta] = useState<number | null>(null);

  const containerRef = useRef<HTMLDivElement>(null);
  const tipRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<maplibregl.Map | null>(null);
  const readyRef = useRef(false);
  const firstFitRef = useRef(true);
  // Live values the once-bound map handlers read (avoids stale closures).
  const hoverRef = useRef<number | null>(null);
  const selRef = useRef<number | null>(null);
  const cohortMapRef = useRef<CohortMap | null>(null);
  const cohortFyRef = useRef<CohortSel>(cohortFy);

  const segKey = segment ? `${segment.kind}:${(segment as { value: unknown }).value}` : "";

  // ── data: every shown cohort in one backend call ──────────────────────────
  useEffect(() => {
    let live = true;
    setByCohort(null); setError(null);
    neighborhoodRetentionYears(segment, cohorts)
      .then((views) => {
        if (!live) return;
        setAvailable(views.some((v) => v.available));
        const m = new Map<number, CohortMap>();
        for (const v of views) {
          const cm: CohortMap = new Map();
          for (const c of v.cells) cm.set(c.nta, c);
          m.set(v.cohort_fy, cm);
        }
        setByCohort(m);
      })
      .catch((e) => { if (live) setError(String(e)); });
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [segKey, currentFy, builtAt]);

  // Neighborhoods with data in ANY cohort — the stable set the mosaic filters to, so only
  // member-neighborhood blocks render (not all 37k NYC blocks).
  const litIdx = useMemo(() => {
    const s = new Set<number>();
    byCohort?.forEach((cm) => cm.forEach((_, nta) => s.add(nta)));
    return [...s];
  }, [byCohort]);

  // "All": every cohort blended per neighborhood (Σ retained / Σ n), so the map can show where
  // ALL current members stay, not one join year. Same NeighborhoodCell shape as a single cohort.
  const allMap = useMemo<CohortMap | null>(() => {
    if (!byCohort || byCohort.size === 0) return null;
    const acc = new Map<number, { n: number; retained: number }>();
    byCohort.forEach((cm) => cm.forEach((c, nta) => {
      const a = acc.get(nta) ?? { n: 0, retained: 0 };
      a.n += c.n; a.retained += c.retained; acc.set(nta, a);
    }));
    if (acc.size === 0) return null;
    const m: CohortMap = new Map();
    acc.forEach((a, nta) => m.set(nta, { nta, n: a.n, retained: a.retained, measure: a.n ? (a.retained / a.n) * 100 : 0 }));
    return m;
  }, [byCohort]);

  const cohortMap = cohortFy === "all" ? allMap : (byCohort?.get(cohortFy) ?? null);
  cohortMapRef.current = cohortMap;
  cohortFyRef.current = cohortFy;
  selRef.current = selNta;

  const opacityExpr = (): ExpressionSpecification => {
    const sel = selRef.current;
    const hov = hoverRef.current ?? -1;
    // With a neighborhood selected, spotlight it: keep it vivid and fade every other
    // neighborhood well back, so the choice reads even against same-coloured neighbours.
    // With nothing selected, a gentle hover lift over the calm default.
    const expr = sel != null
      ? ["case", ["==", ["get", "i"], sel], 0.95, ["==", ["get", "i"], hov], 0.5, 0.18]
      : ["case", ["==", ["get", "i"], hov], 0.7, 0.52];
    return expr as unknown as ExpressionSpecification;
  };

  const repaintOpacity = () => {
    const map = mapRef.current;
    if (map?.getLayer("mosaic")) map.setPaintProperty("mosaic", "fill-opacity", opacityExpr());
  };
  const hoverSet = (nta: number | null) => { if (hoverRef.current === nta) return; hoverRef.current = nta; repaintOpacity(); };

  const fitToIdx = (idx: number[], animate: boolean) => {
    const map = mapRef.current; if (!map || !idx.length) return;
    let a = Infinity, b = Infinity, c = -Infinity, d = -Infinity;
    for (const i of idx) { const bb = META[i].bb; a = Math.min(a, bb[0]); b = Math.min(b, bb[1]); c = Math.max(c, bb[2]); d = Math.max(d, bb[3]); }
    map.fitBounds([[a, b], [c, d]], { padding: { top: 40, left: 330, right: 60, bottom: 60 }, duration: animate ? 700 : 0, maxZoom: 14 });
  };

  const repaint = () => {
    const map = mapRef.current;
    if (!map || !readyRef.current || !map.getLayer("mosaic")) return;
    const cm = cohortMapRef.current ?? new Map<number, NeighborhoodCell>();
    map.setFilter("mosaic", ["in", ["get", "i"], ["literal", litIdx]] as unknown as ExpressionSpecification);
    const m: (string | number | unknown[])[] = ["match", ["get", "i"]];
    cm.forEach((cell, nta) => { m.push(nta, color(cell.measure)); });
    map.setPaintProperty("mosaic", "fill-color", (cm.size ? [...m, NONE] : NONE) as unknown as ExpressionSpecification);
    repaintOpacity();
    const feats = [...cm.keys()].map((nta) => ({ type: "Feature" as const, geometry: { type: "Point" as const, coordinates: META[nta].c }, properties: { name: META[nta].name } }));
    (map.getSource("nlabels") as GeoJSONSource | undefined)?.setData({ type: "FeatureCollection", features: feats });
    if (firstFitRef.current && litIdx.length) { firstFitRef.current = false; fitToIdx(litIdx, false); }
  };

  // ── map: create once, add the mosaic beneath the streets ──────────────────
  useEffect(() => {
    if (!containerRef.current || mapRef.current) return;
    const map = new maplibregl.Map({
      container: containerRef.current, style: BASEMAP, center: [-73.97, 40.75], zoom: 10.5,
      minZoom: 9, maxZoom: 16, dragRotate: false, pitchWithRotate: false, attributionControl: { compact: true },
    });
    mapRef.current = map;
    map.addControl(new maplibregl.NavigationControl({ showCompass: false }), "bottom-right");
    map.touchZoomRotate.disableRotation();

    map.on("load", async () => {
      let blocks: GeoJSON.FeatureCollection;
      try { blocks = await (await fetch(BLOCKS_URL)).json(); }
      catch (e) { setError(`Could not load the neighborhood map geometry: ${e}`); return; }
      if (!mapRef.current) return;
      const beforeRoads = (map.getStyle().layers ?? []).find((l: LayerSpecification) => l.type === "line" && /road|street|bridge|tunnel/i.test(l.id))?.id;
      map.addSource("blocks", { type: "geojson", data: blocks });
      map.addSource("nlabels", { type: "geojson", data: { type: "FeatureCollection", features: [] } });
      map.addLayer({ id: "mosaic", type: "fill", source: "blocks", filter: ["in", ["get", "i"], ["literal", []]] as unknown as ExpressionSpecification, paint: { "fill-color": NONE, "fill-opacity": 0.52 } }, beforeRoads);
      map.addLayer({ id: "nname", type: "symbol", source: "nlabels", layout: { "text-field": ["get", "name"], "text-font": ["Open Sans Semibold", "Arial Unicode MS Bold"], "text-size": 12, "text-max-width": 8, "text-allow-overlap": false }, paint: { "text-color": "#1c1917", "text-halo-color": "#ffffff", "text-halo-width": 1.8 } });

      map.on("mousemove", "mosaic", (e: MapLayerMouseEvent) => {
        const nta = e.features?.[0]?.properties?.i as number | undefined;
        const cell = nta != null ? cohortMapRef.current?.get(nta) : undefined;
        if (nta == null || !cell) { map.getCanvas().style.cursor = ""; hoverSet(null); if (tipRef.current) tipRef.current.style.opacity = "0"; return; }
        map.getCanvas().style.cursor = "pointer";
        hoverSet(nta);
        const tip = tipRef.current;
        if (tip) {
          tip.style.opacity = "1"; tip.style.left = `${e.point.x}px`; tip.style.top = `${e.point.y}px`;
          tip.innerHTML = `<div class="nm">${META[nta]?.name ?? nta}</div><div class="pc" style="color:${tone(cell.measure)}">${Math.round(cell.measure)}% still members</div><div class="sub">${cell.retained} of ${cell.n} · ${cohortFyRef.current === "all" ? "all cohorts" : `${fyLabel(cohortFyRef.current)} cohort`}</div>`;
        }
      });
      map.on("mouseleave", "mosaic", () => { map.getCanvas().style.cursor = ""; hoverSet(null); if (tipRef.current) tipRef.current.style.opacity = "0"; });
      map.on("click", "mosaic", (e: MapLayerMouseEvent) => {
        const nta = e.features?.[0]?.properties?.i as number | undefined;
        if (nta != null && cohortMapRef.current?.has(nta)) setSelNta(nta);
      });

      readyRef.current = true;
      repaint();
    });

    return () => { map.remove(); mapRef.current = null; readyRef.current = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // repaint the mosaic whenever the cohort data or selected year changes
  useEffect(() => { repaint(); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [byCohort, cohortFy, litIdx]);
  // selecting a neighborhood flies to it; clearing returns to the member cluster
  useEffect(() => {
    repaintOpacity();
    if (selNta != null) { const bb = META[selNta].bb; mapRef.current?.fitBounds([[bb[0], bb[1]], [bb[2], bb[3]]], { padding: { top: 60, left: 340, right: 80, bottom: 80 }, duration: 700, maxZoom: 14 }); }
    else if (litIdx.length) fitToIdx(litIdx, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selNta]);

  // ── panel data ────────────────────────────────────────────────────────────
  const ranked = useMemo(() => cohortMap ? [...cohortMap.entries()].sort((a, b) => b[1].measure - a[1].measure) : [], [cohortMap]);
  const series = useMemo(() => selNta == null || !byCohort ? [] : cohorts.map((cy) => byCohort.get(cy)?.get(selNta)?.measure ?? null), [selNta, byCohort, cohorts]);
  const selAvg = useMemo(() => { const v = series.filter((x): x is number => x != null); return v.length ? Math.round(v.reduce((a, b) => a + b, 0) / v.length) : 0; }, [series]);
  const loading = byCohort == null && !error;

  // Household-weighted retention per cohort (Σ retained / Σ n across that cohort's mapped
  // neighborhoods) — the single number the data-aware cohort rail plots. Same absolute ramp as
  // the map, so a bar's colour means the same retention level as the shading. null = no mapped data.
  const cohortAgg = useMemo(() => {
    const m = new Map<number, number | null>();
    for (const cy of cohorts) {
      const cm = byCohort?.get(cy);
      if (!cm || cm.size === 0) { m.set(cy, null); continue; }
      let ret = 0, n = 0;
      cm.forEach((c) => { ret += c.retained; n += c.n; });
      m.set(cy, n ? (ret / n) * 100 : null);
    }
    return m;
  }, [byCohort, cohorts]);
  // Blended retention across every cohort — powers the "All" tick and, when it's selected, the map.
  const allPct = useMemo(() => {
    if (!allMap) return null;
    let ret = 0, n = 0;
    allMap.forEach((c) => { ret += c.retained; n += c.n; });
    return n ? (ret / n) * 100 : null;
  }, [allMap]);
  const cohortsAsc = useMemo(() => [...cohorts].reverse(), [cohorts]); // oldest → newest, so the rail reads left-to-right as a timeline
  const railItems = useMemo<CohortSel[]>(() => ["all", ...cohortsAsc], [cohortsAsc]); // "All" leads the rail
  const selPct = cohortFy === "all" ? allPct : (cohortAgg.get(cohortFy) ?? null);

  // Arrow / Home / End move the selection within the cohort rail (roving-tabindex radiogroup).
  const onRailKey = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const idx = railItems.indexOf(cohortFy);
    let ni = idx;
    if (e.key === "ArrowRight" || e.key === "ArrowUp") ni = Math.min(railItems.length - 1, idx + 1);
    else if (e.key === "ArrowLeft" || e.key === "ArrowDown") ni = Math.max(0, idx - 1);
    else if (e.key === "Home") ni = 0;
    else if (e.key === "End") ni = railItems.length - 1;
    else return;
    e.preventDefault();
    setSelNta(null); setCohortFy(railItems[ni]);
    e.currentTarget.querySelectorAll<HTMLButtonElement>("button")[ni]?.focus();
  };

  const tag: React.CSSProperties = { fontFamily: "var(--font-mono)", fontSize: 10, textTransform: "uppercase", letterSpacing: "var(--tracking-wide)", color: "var(--text-tertiary)" };

  return (
    <div style={{ position: "relative", border: "1px solid var(--border-default)", borderRadius: "var(--radius-xl)", overflow: "hidden", boxShadow: "var(--shadow-sm)" }}>
      <div ref={containerRef} style={{ width: "100%", height: 620 }} role="application" aria-label="Retention by neighborhood map" />

      <div className="nrt-panel" style={{ position: "absolute", top: 16, left: 16, width: 300, maxWidth: "calc(100% - 32px)", maxHeight: "calc(100% - 32px)", overflow: "auto", zIndex: 2, background: "var(--bg-primary)", border: "1px solid var(--border-default)", borderRadius: "var(--radius-xl)", boxShadow: "var(--shadow-lg)", padding: "var(--space-4)" }}>
        <p style={{ margin: "0 0 var(--space-1)", fontWeight: "var(--font-bold)", fontSize: "var(--text-lg)", lineHeight: 1.15, color: "var(--text-primary)" }}>Where members stay</p>
        <p style={{ margin: "0 0 var(--space-3)", fontSize: "var(--text-sm)", color: "var(--text-secondary)", lineHeight: 1.4 }}>Member neighborhoods shaded by the share still enrolled — all members, or a single join cohort.</p>

        <style>{`
.nrt-cohort{display:grid;grid-template-columns:repeat(${railItems.length},1fr);gap:3px;align-items:end}
.nrt-cohort button{all:unset;cursor:pointer;display:flex;flex-direction:column;align-items:center;gap:5px;padding:4px 0 3px;border-radius:6px;transition:background .16s}
.nrt-cohort button:hover{background:var(--bg-secondary)}
.nrt-cohort button:focus-visible{outline:2px solid var(--border-focus);outline-offset:2px}
.nrt-cohort .cw{display:flex;align-items:flex-end;height:42px;width:100%;justify-content:center}
.nrt-cohort .cb{width:72%;max-width:20px;border-radius:4px 4px 2px 2px;transition:height .42s cubic-bezier(.22,1,.36,1),box-shadow .2s}
.nrt-cohort button[aria-checked="true"] .cb{box-shadow:0 0 0 2px var(--bg-primary),0 0 0 4px var(--color-primary-600)}
.nrt-cohort .cy{font-family:var(--font-mono);font-size:10px;color:var(--text-tertiary);font-variant-numeric:tabular-nums;transition:color .2s}
.nrt-cohort button[aria-checked="true"] .cy{color:var(--text-primary);font-weight:700}
@media (prefers-reduced-motion:reduce){.nrt-cohort .cb{transition:box-shadow .2s}}
.nrt-panel{scrollbar-width:thin;scrollbar-color:var(--border-default) transparent}
.nrt-panel::-webkit-scrollbar{width:10px}
.nrt-panel::-webkit-scrollbar-track{background:transparent}
.nrt-panel::-webkit-scrollbar-thumb{background:var(--border-default);border-radius:999px;border:3px solid var(--bg-primary);background-clip:padding-box}
.nrt-panel:hover{scrollbar-color:var(--border-strong) transparent}
.nrt-panel:hover::-webkit-scrollbar-thumb{background:var(--border-strong);background-clip:padding-box}
.nrt-panel::-webkit-scrollbar-thumb:hover{background:var(--text-tertiary);background-clip:padding-box}
`}</style>
        <div style={{ padding: "var(--space-2) 0 var(--space-1)", borderTop: "1px solid var(--border-subtle)", borderBottom: "1px solid var(--border-subtle)", marginBottom: "var(--space-3)" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", gap: "var(--space-2)", marginBottom: "var(--space-2)", ...tag }}>
            <span>Join cohort</span>
            <span style={{ fontFamily: "var(--font-mono)", fontSize: 11, fontWeight: "var(--font-bold)", color: "var(--text-brand)", fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap", textTransform: "none", letterSpacing: 0 }}>{cohortFy === "all" ? "All cohorts" : fyLabel(cohortFy)}{selPct != null ? ` · ${Math.round(selPct)}%` : ""}</span>
          </div>
          <div className="nrt-cohort" role="radiogroup" aria-label="Join cohort" onKeyDown={onRailKey}>
            {railItems.map((cy) => {
              const p = cy === "all" ? allPct : (cohortAgg.get(cy) ?? null);
              const h = p == null ? 7 : Math.round(12 + Math.max(0, Math.min(100, p)) / 100 * 30);
              const sel = cy === cohortFy;
              const name = cy === "all" ? "All cohorts" : fyLabel(cy);
              return (
                <button key={cy} type="button" role="radio" aria-checked={sel} tabIndex={sel ? 0 : -1}
                  aria-label={`${name}${p != null ? `, ${Math.round(p)}% still enrolled` : ", no mapped data"}`}
                  onClick={() => { setSelNta(null); setCohortFy(cy); }}>
                  <span className="cw"><span className="cb" style={{ height: h, background: p == null ? "var(--bg-tertiary)" : color(p) }} /></span>
                  <span className="cy">{cy === "all" ? "All" : yr(cy)}</span>
                </button>
              );
            })}
          </div>
        </div>

        {error ? (
          <p style={{ margin: 0, color: "var(--color-error-600)", fontSize: "var(--text-sm)" }}>{error}</p>
        ) : !available ? (
          <p style={{ margin: 0, color: "var(--text-secondary)", fontSize: "var(--text-sm)" }}>Geographic membership insights are unavailable for this source.</p>
        ) : loading ? (
          <p style={{ margin: 0, color: "var(--text-tertiary)", fontSize: "var(--text-sm)" }}>Loading…</p>
        ) : selNta != null ? (
          <>
            <button type="button" onClick={() => setSelNta(null)} style={{ all: "unset", cursor: "pointer", ...tag, color: "var(--text-brand)", marginBottom: "var(--space-2)", display: "inline-block" }}>‹ all neighborhoods</button>
            <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between", gap: "var(--space-2)" }}>
              <span style={{ fontWeight: "var(--font-bold)", fontSize: "var(--text-base)", color: "var(--text-primary)", lineHeight: 1.15 }}>{META[selNta].name}</span>
              <span style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)", color: "var(--text-brand)", whiteSpace: "nowrap" }}>{selAvg}% avg</span>
            </div>
            <div style={{ ...tag, margin: "var(--space-2) 0 var(--space-1)" }}>Retention across join cohorts</div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 6 }}>
              {cohorts.map((cy, i) => {
                const v = series[i];
                return (
                  <div key={cy} style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 3 }}>
                    <span style={{ width: "100%", textAlign: "center", padding: "7px 0", borderRadius: 6, fontWeight: "var(--font-bold)", fontSize: "var(--text-sm)", fontVariantNumeric: "tabular-nums", background: v == null ? "var(--bg-tertiary)" : color(v), color: v == null ? "var(--text-tertiary)" : "#1c1917" }}>{v == null ? "—" : Math.round(v)}</span>
                    <span style={{ fontFamily: "var(--font-mono)", fontSize: 9, color: "var(--text-tertiary)" }}>{yr(cy)}</span>
                  </div>
                );
              })}
            </div>
          </>
        ) : ranked.length === 0 ? (
          <p style={{ margin: 0, color: "var(--text-secondary)", fontSize: "var(--text-sm)" }}>No neighborhoods clear the 10-household floor {cohortFy === "all" ? "across the join cohorts" : `for ${fyLabel(cohortFy)}`}.</p>
        ) : (
          <>
            <div style={{ ...tag, marginBottom: "var(--space-2)" }}>Ranked · strongest first</div>
            <div className="nrt-rank" style={{ display: "flex", flexDirection: "column", gap: 2 }}>
              {ranked.map(([nta, cell]) => (
                <button key={nta} type="button" onClick={() => setSelNta(nta)} onMouseEnter={() => hoverSet(nta)} onMouseLeave={() => hoverSet(null)} style={{ all: "unset", cursor: "pointer", display: "block", padding: "7px 8px", borderRadius: 8 }}>
                  <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between", gap: "var(--space-2)" }}>
                    <span title={META[nta].name} style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)", lineHeight: 1.25, display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical", overflow: "hidden" }}>{META[nta].name}</span>
                    <span style={{ flexShrink: 0, fontWeight: "var(--font-bold)", fontSize: "var(--text-sm)", color: "var(--text-primary)", fontVariantNumeric: "tabular-nums" }}>{Math.round(cell.measure)}%</span>
                  </div>
                  <span style={{ display: "block", height: 6, marginTop: 6, background: "var(--bg-tertiary)", borderRadius: 999, overflow: "hidden" }}>
                    <span style={{ display: "block", height: "100%", width: `${Math.max(3, cell.measure)}%`, background: color(cell.measure), borderRadius: 999 }} />
                  </span>
                </button>
              ))}
            </div>
            <style>{`.nrt-rank button:hover{background:var(--bg-secondary)}`}</style>
          </>
        )}
      </div>

      <div style={{ position: "absolute", bottom: 14, left: 16, zIndex: 2, display: "flex", alignItems: "center", gap: 7, ...tag, background: "rgba(255,255,255,.92)", border: "1px solid var(--border-default)", borderRadius: "var(--radius-md)", padding: "6px 10px" }}>
        <span>Low</span>
        <span style={{ display: "flex", height: 8, width: 96, borderRadius: 2, overflow: "hidden" }}>{RAMP.map((c) => <span key={c} style={{ flex: 1, background: c }} />)}</span>
        <span>High</span>
      </div>

      <div id="nrt-tip" ref={tipRef} style={{ position: "absolute", pointerEvents: "none", zIndex: 5, opacity: 0, transform: "translate(-50%, -120%)", background: "var(--bg-primary)", border: "1px solid var(--border-default)", borderRadius: 9, padding: "8px 11px", minWidth: 150, maxWidth: 230, boxShadow: "var(--shadow-md)", transition: "opacity .1s" }}>
        <style>{`#nrt-tip .nm{font-weight:700;font-size:1rem;line-height:1.1;color:var(--text-primary)} #nrt-tip .pc{font-family:var(--font-mono);font-size:11px;font-weight:600;margin-top:4px} #nrt-tip .sub{font-family:var(--font-mono);font-size:10px;color:var(--text-tertiary);margin-top:3px}`}</style>
      </div>
    </div>
  );
}
