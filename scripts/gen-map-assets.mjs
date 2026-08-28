// Regenerate the neighborhood-map assets. Deterministic; run once and commit the outputs.
//   node scripts/gen-map-assets.mjs
// Source neighborhood geometry is the sibling workspace project's map assets (see
// src/assets/nta-meta.md). All three outputs derive from the SAME nta.geojson ordering, so
// block.i, the crosswalk index, and nta-meta index all line up.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const APP = path.resolve(HERE, "..");
const RAGE = path.resolve(APP, "..", "v0-rage-against-social-machine", "public", "map");

const nta = JSON.parse(fs.readFileSync(path.join(RAGE, "nta.geojson"), "utf8"));
const zcta = JSON.parse(fs.readFileSync(path.join(APP, "src", "assets", "ny-zcta-boundaries.json"), "utf8"));

const polysOf = (g) => (g.type === "Polygon" ? [g.coordinates] : g.coordinates); // [outer, ...holes] per polygon
const bboxOf = (g) => { let a = Infinity, b = Infinity, c = -Infinity, d = -Infinity; for (const poly of polysOf(g)) for (const ring of poly) for (const [x, y] of ring) { if (x < a) a = x; if (y < b) b = y; if (x > c) c = x; if (y > d) d = y; } return [+a.toFixed(5), +b.toFixed(5), +c.toFixed(5), +d.toFixed(5)]; };
const centroid = (g) => { let best = null, bn = 0; for (const poly of polysOf(g)) for (const ring of poly) if (ring.length > bn) { bn = ring.length; best = ring; } let sx = 0, sy = 0; for (const [x, y] of best) { sx += x; sy += y; } return [+(sx / best.length).toFixed(5), +(sy / best.length).toFixed(5)]; };
const inRing = (x, y, ring) => { let ins = false; for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) { const [xi, yi] = ring[i], [xj, yj] = ring[j]; if ((yi > y) !== (yj > y) && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) ins = !ins; } return ins; };
const inGeom = (x, y, g) => { for (const poly of polysOf(g)) if (inRing(x, y, poly[0]) && !poly.slice(1).some((h) => inRing(x, y, h))) return true; return false; };

// nta-meta.json — index -> {name, boro, c:[lon,lat], bb:[minx,miny,maxx,maxy]}
const ntaMeta = nta.features.map((f) => ({ name: f.properties.NTAName, boro: f.properties.BoroName, c: centroid(f.geometry), bb: bboxOf(f.geometry) }));
const metaOut = path.join(APP, "src", "assets", "nta-meta.json");
fs.writeFileSync(metaOut, JSON.stringify(ntaMeta));

// zip_nta_crosswalk.json — {zip: nta_index} by dominant overlap (7x7 sample grid, majority wins)
const ntaGeo = nta.features.map((f, i) => ({ i, g: f.geometry, bb: bboxOf(f.geometry) }));
const ntaAt = (x, y) => { for (const n of ntaGeo) { const b = n.bb; if (x < b[0] || x > b[2] || y < b[1] || y > b[3]) continue; if (inGeom(x, y, n.g)) return n.i; } return -1; };
const N = 7; const cross = {}; let mapped = 0;
for (const f of zcta.features) {
  const zip = f.properties.ZCTA5CE20; const g = f.geometry; const [x0, y0, x1, y1] = bboxOf(g);
  const tally = {};
  for (let a = 0; a < N; a++) for (let b = 0; b < N; b++) {
    const x = x0 + ((a + 0.5) / N) * (x1 - x0), y = y0 + ((b + 0.5) / N) * (y1 - y0);
    if (!inGeom(x, y, g)) continue; const ni = ntaAt(x, y); if (ni < 0) continue; tally[ni] = (tally[ni] || 0) + 1;
  }
  const best = Object.entries(tally).sort((p, q) => q[1] - p[1])[0];
  if (best) { cross[zip] = +best[0]; mapped++; }
}
const crossOut = path.join(APP, "src-tauri", "src", "zip_nta_crosswalk.json");
fs.writeFileSync(crossOut, JSON.stringify(cross));

// blocks.geojson — the census-block mosaic, copied verbatim into public/ for runtime fetch
const pubMap = path.join(APP, "public", "map");
fs.mkdirSync(pubMap, { recursive: true });
fs.copyFileSync(path.join(RAGE, "blocks.geojson"), path.join(pubMap, "blocks.geojson"));

console.log(`nta-meta.json: ${ntaMeta.length} neighborhoods`);
console.log(`zip_nta_crosswalk.json: ${mapped} ZIPs mapped`);
console.log(`public/map/blocks.geojson: ${(fs.statSync(path.join(pubMap, "blocks.geojson")).size / 1e6).toFixed(1)} MB`);
