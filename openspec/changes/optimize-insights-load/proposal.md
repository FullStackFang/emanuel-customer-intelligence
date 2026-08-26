## Why

Opening the Insights page pegs one CPU core for several seconds behind a "Loading insights / Reading the local mirror" spinner, and can feel like it never loads. All data is local, so this is pure recomputation waste: the validated churn-risk analysis retrains from scratch on every visit, and it runs serialized ahead of the cheaper membership views because both backend commands contend for a single store lock. This makes a local-data analytics page feel broken when it is only slow.

## What Changes

- Reuse the validated risk analysis across visits: persist the fitted model, its validation, and the Watch List, keyed to the analytical dataset's build stamp, and recompute only after a rebuild. Today it is recomputed on every Insights load.
- Render the membership views without waiting for the risk analysis: `get_insights` returns and the page paints first; the risk view populates independently and shows its own loading state.
- Reduce the risk computation's own cost so a post-rebuild recompute is fast: replace per-household linear scans of the household-year dataset with an indexed lookup, and stop reading the analytical dataset twice per load.
- Add lightweight, throwaway timing instrumentation around rebuild, views, and risk analysis to confirm the dominant cost before and after the change.
- No change to any analytical result, validation gate, privacy boundary, or audit behavior — same numbers, same names, computed once instead of every time.

## Capabilities

### New Capabilities
<!-- none -->

### Modified Capabilities
- `validated-membership-risk`: add a requirement that a validated risk result is reused across reads while the analytical dataset is unchanged and is recomputed when the dataset is rebuilt, without altering the model, its gates, or its outputs.
- `membership-lifecycle-insights`: add a requirement that membership lifecycle views become visible independently of the risk analysis, so Insights does not block on risk computation.

## Impact

- **Code (Rust)**: `src-tauri/src/commands.rs` (`get_insights`, `get_risk_summary`, `analyze_risk`, `ensure_fresh`, `with_store`), `src-tauri/src/risk.rs` (`analyze`, `feature_rows`/`build_feature_row`, scoring/evidence lookups, serde on `RiskModel`/`WatchList`/`Validation`), `src-tauri/src/insights.rs` (build-stamp reuse), `src-tauri/src/store.rs` (`_meta` read/write for the cache).
- **Code (Frontend)**: `src/pages/InsightsPage.tsx` load flow (`:55-65`) and the risk section's loading state.
- **Privacy/Audit**: the risk cache holds household names but is stored only in `_meta` inside the existing SQLCipher-encrypted database — same at-rest boundary as the analytical mart; no new exposure. Named Watch List access remains audited exactly as today.
- **Dependencies**: none added. Reuses `serde_json` and the existing `tracing` setup.
- **Data**: no schema change to source or mart tables; adds one `_meta` cache key that self-invalidates on rebuild.
