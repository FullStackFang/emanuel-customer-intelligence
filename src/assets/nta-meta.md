# Neighborhood map assets

The Retention-by-area map (`src/pages/insights/NeighborhoodRetentionMap.tsx`) draws member
**neighborhoods** as a census-block mosaic. Three generated assets drive it, all produced by
`scripts/gen-map-assets.mjs` from a single source ordering so their indices align:

| Asset | Location | Purpose |
| --- | --- | --- |
| `nta-meta.json` | `src/assets/` | Per-neighborhood `{name, boro, centroid, bbox}`, indexed by NTA. The webview resolves a backend neighborhood index to a name / label position / fly-to bounds. |
| `zip_nta_crosswalk.json` | `src-tauri/src/` | `{ zip: nta_index }` for the ~214 NYC ZCTAs that overlap a neighborhood. Embedded in the backend (`insights::zip_nta`); the ZIP→neighborhood rollup. |
| `blocks.geojson` | `public/map/` | The 37k-block NYC census mosaic; each block's `i` = its neighborhood index. Fetched at runtime (not bundled into JS). |

## Source geometry

From the sibling workspace project `v0-rage-against-social-machine/public/map/`:

- `nta.geojson` — 195 NYC Neighborhood Tabulation Areas (`NTAName`, `BoroName`). Its feature
  order **is** the index used everywhere above. Central Park etc. live in the
  `park-cemetery-etc-Manhattan` NTA, which no member ZIP maps to — so parks never light up.
- `blocks.geojson` — the block mosaic, each block pre-tagged with `i` = its parent NTA index.

ZCTA boundaries come from this repo's `src/assets/ny-zcta-boundaries.json`.

## Regenerate

```
node scripts/gen-map-assets.mjs
```

The crosswalk uses **dominant overlap** (a 7×7 sample grid per ZCTA, majority neighborhood
wins); a ZCTA overlapping no NYC neighborhood is omitted (the backend counts it out-of-area
rather than misplacing it). Re-run whenever the neighborhood geometry changes, then rebuild —
`GEO`-style caches don't apply here (no persisted cache for the neighborhood view yet).
