## Context

Opening Insights triggers `InsightsPage.tsx:60`: `Promise.all([api.getInsights(force), api.getRiskSummary().catch(() => null)])`. Both Tauri commands acquire the same `Mutex<Option<Store>>` (`commands.rs:24`) through `with_store` (`commands.rs:37`), so despite `Promise.all` they run strictly one at a time. The page spinner clears only when `get_insights` resolves (gate at `InsightsPage.tsx:130`).

Measured behavior: one core pegged ~93% single-threaded for several seconds, then idle; the database file is byte-identical during steady state (no writes). So the cost is read + decrypt + in-memory compute, not I/O.

Verified cost split:
- The ~580 MB mirror decrypt happens only in `insights::rebuild` (`insights.rs:956`, `SELECT ... FROM "Account"`), which `ensure_fresh` (`commands.rs:433`) runs only when the mart is older than the newest sync. This is a post-sync cost, not a per-visit one.
- Per-visit reads (`insights::load`, `insights::load_household_years`) touch only the small derived mart tables `_m_household` / `_m_household_fy`.
- `risk::analyze` (`risk.rs:741`) recomputes on every visit — there is no `_meta` persistence anywhere in `risk.rs`. It builds features over ~11–12 cutoff years with per-household linear scans of the household-year set (`build_feature_row`, `risk.rs:118`), fits a smartcore `LogisticRegression` (`risk.rs:220`) once per test year, and reruns the whole rolling backtest up to 4× inside the coverage-driven family-removal loop (`evaluate`, `risk.rs:465`). `analyze_risk` (`commands.rs:485`) also re-reads both mart tables that `views` already read.

The recurring, felt slowness is therefore the risk recomputation, made worse by the shared lock: if `get_risk_summary` wins the lock, the spinner waits the entire risk compute before `get_insights` can start.

## Goals / Non-Goals

**Goals:**
- Warm revisits (no new sync) feel instant: no risk retrain, no duplicate mart read.
- Insights views paint without waiting on risk computation.
- A post-rebuild recompute is fast enough not to dominate the load.
- Identical analytical output: same scores, names, validation, and audit behavior.

**Non-Goals:**
- No change to the risk model, its features, its validation gates, or any displayed number.
- No true CPU-parallelism of the two commands (both are single-core CPU-bound; nothing to gain once risk is cached).
- No SQLCipher cipher-configuration changes (see the documented crash landmine at `store.rs:50-58`).
- No incremental/diff-based mart rebuild.

## Decisions

### Decision 1: Cache the fitted risk result in `_meta`, keyed on the build stamp
Persist `{built_at, RiskModel, WatchList}` as JSON under a `_meta` key (e.g. `risk_cache`). In `analyze_risk`, read `insights_built_at` (set atomically inside the rebuild transaction, `insights.rs:1090`) and the cached blob; if `cache.built_at == current built_at`, deserialize and return, skipping all compute. On any miss or deserialize error, compute as today and write the cache.

- **Why**: `insights_built_at` already advances on every rebuild, so the cache self-invalidates on sync/rebuild with no extra wiring. A miss falls back to the exact current path, so the cache can never change results — only skip repeats.
- **Alternatives considered**: (a) An in-memory cache in `AppState` — lost on restart and duplicates a persistence mechanism the DB already offers. (b) A separate cache table — heavier than one `_meta` row for a small blob. (c) Time-based invalidation — fragile vs. the exact build stamp.
- **Serialization**: add `Serialize`/`Deserialize` to `RiskModel`, `WatchList`, `Validation`, `YearResult`, `FamilyCoverage`, `WatchRow`, `Evidence` — all plain scalars/strings today.
- **Privacy**: the cache holds household names, but `_meta` is inside the SQLCipher-encrypted DB — the same at-rest boundary as the mart. Named Watch List access stays audited on read exactly as now; caching does not bypass the audit in `get_watch_list` (`commands.rs:508`).

### Decision 2: Render insights first; don't await risk behind the lock
Change `InsightsPage.tsx:60` from `Promise.all` to: `await api.getInsights(force)` → `setIns`, then fire-and-forget `api.getRiskSummary().then(setRisk).catch(() => {})`. The risk view already lives in its own tab (`InsightsPage.tsx:50`) and handles `risk === null`; add an explicit in-progress state there.

- **Why**: the shared `Mutex` means the two calls cannot truly overlap, so `Promise.all` buys nothing and risks the spinner waiting behind the full risk compute. Insights-first guarantees the page paints after the cheap views body and releases the lock; risk then computes/loads separately. Directly satisfies the "views render independently of risk" requirement.
- **Alternatives considered**: dropping the `Mutex` to an `RwLock` or opening a second read connection for real parallelism — rejected as a non-goal (both bodies are CPU-bound; a second SQLCipher connection re-enters cipher init near the crash landmine).

### Decision 3: Index the per-household year lookups in risk
At the top of `risk::analyze`, build a `HashMap<(String, i32), &HhFy>` (or `HashMap<String, Vec<&HhFy>>` grouped by account) once and pass it into `build_feature_row` (`risk.rs:118`), `scoring_rows` (`risk.rs:89`), and `evidence_classes` (`risk.rs:541`), replacing every `years.iter().find/any`.

- **Why**: turns O(cutoffs × households × years) feature building and O(households × years) scoring into near-linear. Makes even a cache-miss (post-rebuild) recompute fast, so the cache is a safety net, not a crutch. Pure lookup refactor — deterministic, same outputs.
- **Left alone**: the ~40–48 solver fits are inherent to the coverage-removal design and smartcore has no warm-start; indexing addresses the larger, cheaper-to-fix term.

### Decision 4: Confirm the dominant cost with throwaway instrumentation first
Wrap `ensure_fresh`, `insights::views` (`commands.rs:464`), and `risk::analyze` (`commands.rs:491`) in `std::time::Instant` + the existing `tracing` (already imported, `commands.rs:32`). Three log lines split rebuild vs mart-read vs risk-compute, before and after. No new dependencies.

- **Why**: verify risk-compute dominance (currently high-confidence inference from the CPU profile) before investing in Decisions 1 and 3, and quantify the win afterward.

## Risks / Trade-offs

- **Stale cache returning outdated risk** → keyed strictly on `insights_built_at`, which changes only inside the rebuild transaction; a mismatch always recomputes. Never trust the blob if the stamp differs or it fails to parse.
- **Cache-serialization drift as `RiskModel` evolves** → on any deserialize error, fall back to compute (never surface a partial/garbled result); treat a schema change to these structs as a cache miss.
- **Named-Watch-List audit accidentally skipped when cached** → keep the audit write in `get_watch_list` on the read path, independent of whether the model came from cache or compute.
- **Frontend regression: risk tab stuck if the fire-and-forget promise is dropped** → give the risk view explicit loading / error / unavailable states; a rejected `getRiskSummary` must resolve to a visible state, not a permanent spinner.
- **Indexing refactor changing results** → guard with the existing risk tests plus a before/after equivalence check on a synthetic dataset; outputs must be identical.
- **`cache_size` PRAGMA (optional, deferred)** → only the non-cipher `cache_size` may be touched, and the 4-test suite that reproduced the `cipher_memory_security` crash must be re-run; cipher/mmap/page_size changes are out of scope.

## Migration Plan

1. Land instrumentation (Decision 4), capture baseline numbers on the real local DB.
2. Frontend insights-first render (Decision 2) — immediate perceived win, no backend risk.
3. Risk `_meta` cache (Decision 1) — the structural fix for the recurring cost.
4. Index risk lookups (Decision 3) — fast post-rebuild recompute.
5. Re-capture numbers; remove or gate the instrumentation.

Rollback: each step is independent and additive. Reverting the cache (Decision 1) restores compute-every-time; reverting the frontend change (Decision 2) restores `Promise.all`. No data migration — the `_meta` cache key is self-invalidating and safe to leave or delete.

## Open Questions

- Should the `_meta` risk cache be dropped proactively on app version change, or is build-stamp keying plus deserialize-fallback sufficient? (Leaning: fallback is sufficient; no proactive drop.)
- Is the optional `cache_size` PRAGMA worth including here, or deferred to a separate change given the crash landmine? (Leaning: defer.)
