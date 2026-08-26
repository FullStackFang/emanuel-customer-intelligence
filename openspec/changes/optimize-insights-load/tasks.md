## 1. Baseline instrumentation

- [ ] 1.1 Add `std::time::Instant` timing + `tracing` log lines around `ensure_fresh`, `insights::views`, and `risk::analyze` in `get_insights`/`get_risk_summary`/`analyze_risk` (`commands.rs:433,464,491`)
- [ ] 1.2 Run a cold load (post-sync) and a warm revisit against the real local DB; record rebuild vs mart-read vs risk-compute timings to confirm risk-compute dominates the warm path
- [ ] 1.3 Keep the timing lines behind `tracing` (or mark for removal) so they can be re-run after the change

## 2. Render insights first (frontend)

- [ ] 2.1 Change `InsightsPage.tsx:60` from `Promise.all([...])` to `await api.getInsights(force)` → `setIns`, then fire-and-forget `api.getRiskSummary().then(setRisk).catch(...)`
- [ ] 2.2 Ensure `getRiskSummary` rejection resolves the risk view to a visible error/unavailable state, never a permanent spinner
- [ ] 2.3 Add an explicit in-progress ("analyzing churn risk…") state to the risk section driven by `risk === null`
- [ ] 2.4 Verify the membership views paint before risk completes and that a risk failure does not blank the lifecycle views (covers `membership-lifecycle-insights` scenarios)

## 3. Cache the validated risk result in `_meta`

- [ ] 3.1 Derive `Serialize`/`Deserialize` on `RiskModel`, `WatchList`, `Validation`, `YearResult`, `FamilyCoverage`, `WatchRow`, `Evidence` in `risk.rs`
- [ ] 3.2 Define a `_meta` cache blob `{built_at, RiskModel, WatchList}` (key e.g. `risk_cache`) with serialize/deserialize helpers using `serde_json`
- [ ] 3.3 In `analyze_risk` (`commands.rs:485`), read `insights_built_at` and the cache; return the cached result when `built_at` matches; otherwise compute and write the cache
- [ ] 3.4 Fall back to full computation on any cache miss or deserialize error (never surface a partial result)
- [ ] 3.5 Keep the named Watch List audit write on the read path (`get_watch_list`, `commands.rs:508`) independent of cache hit vs. compute
- [ ] 3.6 Add tests: second read with unchanged dataset reuses the result; a rebuild invalidates it and forces recompute; an unreadable cache falls back to compute (covers `validated-membership-risk` scenarios)

## 4. Index the risk per-household lookups

- [ ] 4.1 Build a `HashMap<(String, i32), &HhFy>` (or account-grouped map) once at the top of `risk::analyze` (`risk.rs:741`)
- [ ] 4.2 Replace `years.iter().find/any` in `build_feature_row` (`risk.rs:118`), `scoring_rows` (`risk.rs:89`), and `evidence_classes` (`risk.rs:541`) with map lookups
- [ ] 4.3 Add a before/after equivalence check on a synthetic dataset proving identical model output, and confirm existing risk tests stay green

## 5. Verification

- [ ] 5.1 Re-run the instrumentation: warm revisit shows no risk retrain and no duplicate mart read; capture the improvement vs. the 1.2 baseline
- [ ] 5.2 Run `cargo test` (Rust) and the frontend `vitest` suite; confirm no analytical number, name, validation gate, or audit behavior changed
- [ ] 5.3 Manually verify in the running app: warm Insights load feels instant, risk tab fills in independently, post-sync load recomputes correctly
- [ ] 5.4 Remove or finalize the instrumentation added in task 1
