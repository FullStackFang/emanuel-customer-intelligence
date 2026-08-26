## 1. Baseline instrumentation

- [x] 1.1 Add `std::time::Instant` timing + `tracing` log lines around `ensure_fresh`, `insights::views`, and `risk::analyze` in `get_insights`/`get_risk_summary`/`analyze_risk` (`commands.rs:433,464,491`)
- [x] 1.2 Run a cold load (post-sync) and a warm revisit against the real local DB; record rebuild vs mart-read vs risk-compute timings to confirm risk-compute dominates the warm path
  - Measured 2026-08-26 (release, `examples/time_insights.rs` on a copy of the real mirror): rebuild 218,066 ms of which the `dues_evidence` loop 204,930 ms (94%); `insights::views` 33,634–49,659 ms on EVERY load (`cohort_matrix` 24–34 s, `year1` ×2 ≈ 6–9 s, `channels` 3.3 s); risk mart read ≈ 0.8 s; `risk::analyze` 6.4–7.7 s (not 50 s — the earlier figure was a debug build). Conclusion: risk does NOT dominate the warm path; `views` does, and the day-to-day app is a debug binary (`npm run tauri dev`).
- [x] 1.3 Keep the timing lines behind `tracing` (or mark for removal) so they can be re-run after the change

## 2. Render insights first (frontend)

- [x] 2.1 Change `InsightsPage.tsx:60` from `Promise.all([...])` to `await api.getInsights(force)` → `setIns`, then fire-and-forget `api.getRiskSummary().then(setRisk).catch(...)`
- [x] 2.2 Ensure `getRiskSummary` rejection resolves the risk view to a visible error/unavailable state, never a permanent spinner
- [x] 2.3 Add an explicit in-progress ("analyzing churn risk…") state to the risk section driven by `risk === null`
- [x] 2.4 Verify the membership views paint before risk completes and that a risk failure does not blank the lifecycle views (covers `membership-lifecycle-insights` scenarios)

## 3. Cache the validated risk result in `_meta`

- [x] 3.1 Derive `Serialize`/`Deserialize` on `RiskModel`, `WatchList`, `Validation`, `YearResult`, `FamilyCoverage`, `WatchRow`, `Evidence` in `risk.rs`
- [x] 3.2 Define a `_meta` cache blob `{built_at, RiskModel, WatchList}` (key e.g. `risk_cache`) with serialize/deserialize helpers using `serde_json`
- [x] 3.3 In `analyze_risk` (`commands.rs:485`), read `insights_built_at` and the cache; return the cached result when `built_at` matches; otherwise compute and write the cache
- [x] 3.4 Fall back to full computation on any cache miss or deserialize error (never surface a partial result)
- [x] 3.5 Keep the named Watch List audit write on the read path (`get_watch_list`, `commands.rs:508`) independent of cache hit vs. compute
- [x] 3.6 Add tests: second read with unchanged dataset reuses the result; a rebuild invalidates it and forces recompute; an unreadable cache falls back to compute (covers `validated-membership-risk` scenarios)

## 4. Index the risk per-household lookups

- [x] 4.1 Build a `HashMap<(String, i32), &HhFy>` (or account-grouped map) once at the top of `risk::analyze` (`risk.rs:741`)
- [x] 4.2 Replace `years.iter().find/any` in `build_feature_row` (`risk.rs:118`), `scoring_rows` (`risk.rs:89`), and `evidence_classes` (`risk.rs:541`) with map lookups
- [x] 4.3 Add a before/after equivalence check on a synthetic dataset proving identical model output, and confirm existing risk tests stay green

## 5. Verification

- [x] 5.1 Re-run the instrumentation: warm revisit shows no risk retrain and no duplicate mart read; capture the improvement vs. the 1.2 baseline
  - After section 6: rebuild 13,331 ms (was 218,066), `views` 1,065 / 1,025 ms cold/warm (was 33,634–49,659), risk mart read 808 ms, `risk::analyze` 7,285 ms (cached after the first run).
- [x] 5.2 Run `cargo test` (Rust) and the frontend `vitest` suite; confirm no analytical number, name, validation gate, or audit behavior changed
- [ ] 5.3 Manually verify in the running app: warm Insights load feels instant, risk tab fills in independently, post-sync load recomputes correctly
- [x] 5.4 Remove or finalize the instrumentation added in task 1

## 6. Index the rebuild and view hot paths (added after the 1.2 measurement)

- [x] 6.1 `apply_dues`: build a `DuesIndex` once (statement id → household-years; membership lines grouped per `(hh, fy)` in original order); `dues_evidence` becomes a thin wrapper; equivalence test vs. the naive scan
- [x] 6.2 `views`: one `HouseholdYearIndex` over `_m_household_fy`; `year1_indexed` / `cohort_matrix_indexed` / `channels_indexed`; compute `year1` once and pass it to `kpis`; equivalence test vs. the reference scans
- [x] 6.3 Staleness: `Store::newest_mart_source_sync_at()` restricted to `insights::MART_SOURCE_OBJECTS`; used by `ensure_fresh_with` and `views`, so syncing an unrelated object no longer forces a rebuild; test
- [x] 6.4 `profile_selected` calls `ensure_fresh` instead of an unconditional `insights::rebuild`
- [x] 6.5 Remove the temporary sub-phase timers; keep `examples/time_insights.rs` as the re-runnable harness

## 7. Visible progress (added — the user could not tell "working" from "hung")

- [x] 7.1 Frontend: structured `insights:progress` payload `{job, phase, step, steps, done, total, elapsed_ms}`; `JobStatus` phase checklist with counters, determinate bar and elapsed timer; inline rebuild banner on revisit/force; risk-analysis step status in the Risk tab and header; error state with retry; `get_insights_job` resume on mount; 5 new vitest cases
- [x] 7.2 Backend: `progress::Reporter` (throttled, monotonic) threaded through `rebuild_with` (5 phases with row counters) and `risk::analyze_with` (4 phases, one tick per validation year); `AppState.job` + `get_insights_job` command that never takes the store lock; store work of `get_insights`/`get_risk_summary`/`get_watch_list` moved onto `spawn_blocking`
- [x] 7.3 Debug builds: `[profile.dev] opt-level = 1` + `[profile.dev.package."*"] opt-level = 3` so `npm run tauri dev` is not several× slower than release on the compute phases (one-time full dependency recompile ≈ 30 min on first build; incremental after)
- [ ] 7.4 Manually verify in the running app: post-sync rebuild shows phases + counts + elapsed; warm load ≈ 1 s; revisiting during a rebuild shows the banner
