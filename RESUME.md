# Resume this build

This repo is being built with subagent-driven execution of an implementation plan.

**To continue in a new Claude Code session rooted in THIS folder, paste:**

> Resume the subagent-driven execution of
> `docs/superpowers/plans/2026-08-25-customer-intelligence-v1.md`.
> Read the ledger at `.superpowers/sdd/2026-08-25-customer-intelligence-v1/progress.md`
> first, then continue from the first task without a `complete` line.
> Do NOT run `npm run tauri dev` (blocking GUI window in a headless agent) —
> verify Rust with `cd src-tauri && cargo build`, frontend with `npm run build`
> and `npx tsc --noEmit`.

Key facts the new session needs (also in the ledger):
- Branch: `feat/v1-implementation`
- Spec: `docs/superpowers/specs/2026-08-25-customer-intelligence-v1-design.md`
- `.env` holds the real Salesforce Consumer Key (gitignored); `.env.example` has blanks.
- Task 2 starts with a SQLCipher build probe. If `bundled-sqlcipher-vendored-openssl`
  won't compile on MSVC, fall back to DuckDB per the plan (do not improvise).
- Known pre-scan ruling: in Task 9 `commands.rs`, call the `with_store`/`who`/`client`
  helpers as `state.inner()`, not `&state` (State<AppState> won't coerce to &AppState).
