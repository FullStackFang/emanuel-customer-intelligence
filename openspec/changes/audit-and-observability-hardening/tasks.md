## 1. Store foundations (audit detail + retention)

- [x] 1.1 Write a `store.rs` test asserting `audit` persists a `detail` JSON that includes an `account_ids` array and a `cid`, and that `list_audit` returns it intact (round-trip).
- [x] 1.2 Write a `store.rs` test for `prune_chat(before)`: seed messages older and newer than a cutoff across two conversations; assert only older messages are deleted, a conversation emptied by pruning is removed, and a partially pruned conversation survives with its newer messages.
- [x] 1.3 Write a `store.rs` test asserting `prune_chat` does not touch the synced mirror and that the existing `purge_mirror` still preserves unexpired chat history.
- [x] 1.4 Add `Store::prune_chat(before: &str) -> Result<usize>` (indexed `DELETE` on `_chat_messages.created_at`, then delete conversations with no remaining messages). No schema change.
- [x] 1.5 Confirm the existing `audit(detail: Option<Value>)` path carries the richer JSON unchanged; keep the insert-only contract (no update/delete). Make tests 1.1–1.3 pass.

## 2. Record-level PII access auditing (`pii-access-audit`)

- [x] 2.1 Write a `commands.rs` test that `get_at_risk` writes an audit row whose `detail` lists every returned household `account_id` plus the count, and contains no household `name`.
- [x] 2.2 Write a test that `get_watch_list` writes an audit row listing every listed household `account_id`, and that an empty result still writes a zero-count row with an empty id set.
- [x] 2.3 Write a test that reading the audit log (`get_audit`), `chat_list_conversations`, and `chat_list_messages` each write exactly one audit row of low-cardinality metadata (counts/offsets/conversation id), with no message content and no recursive re-audit.
- [x] 2.4 Extend the audit `detail` in `get_at_risk`, `get_watch_list`, and `export_watch_list_csv` to include `account_ids` (full, untruncated set) alongside the existing count. Do not add `name`.
- [x] 2.5 Add audit rows to `get_audit`, `chat_list_conversations`, and `chat_list_messages`; ensure `list_audit` stays a pure read so audit-log reads don't recurse. Make tests 2.1–2.3 pass.

## 3. Persistent logging + chat telemetry (`persistent-logging`)

- [x] 3.1 Add `tracing-appender` to `src-tauri/Cargo.toml`.
- [x] 3.2 Write a unit test asserting the chat-turn telemetry event carries backend + elapsed ms, includes token counts only when present, and contains neither prompt/reply text nor any household name/email/address/id (mirror the `chat_context.rs` leak-test pattern).
- [x] 3.3 In `lib.rs`, compose a daily-rotating file layer (`tracing-appender`, non-blocking) under `<app_data_dir>/logs/` with the existing stdout `fmt` layer via `registry().with(...)`; preserve the `EnvFilter`/`RUST_LOG` default and the `insights_timing` target. Hold the `WorkerGuard` for the process lifetime.
- [x] 3.4 Emit the per-turn telemetry event in `run_chat_stream` on a completed (non-cancelled) turn, target `chat_telemetry`, with `backend`, `ms`, optional `prompt_tokens`/`completion_tokens`, and `conversation_id`; make no event fire on cancel. Make test 3.2 pass.
- [ ] 3.5 Manually verify a `logs/` file is written on a real run and that log level still responds to `RUST_LOG`.

## 4. Chat retention wiring (`chat-retention`)

- [x] 4.1 Add a named `CHAT_RETENTION_DAYS` constant (default 365) and a helper computing the cutoff timestamp in the store's `now_iso` format.
- [x] 4.2 Call `prune_chat(cutoff)` best-effort at conversation open (`chat_list_messages`) and at turn start (`chat_send`); a prune error must be logged (warn) but must not fail the chat action.
- [x] 4.3 Write a test that a prune failure during a chat action does not surface as a chat error (the action still returns success).

## 5. Request correlation (`request-correlation`)

- [x] 5.1 Write a test that an audited command's audit row contains a `cid`, that two invocations produce different `cid`s, and that the `cid` contains no household identity.
- [x] 5.2 Generate a correlation id via the existing `new_id("act")` at the start of the audited read/write commands; include it in the audit `detail` (`cid`) and attach it to that command's log events (e.g. as a field on the emitted events). Make test 5.1 pass.

## 6. Frontend + verification

- [x] 6.1 Confirm `AuditPage.tsx` renders the richer `detail` (account id set, cid, read events) without code change; adjust rendering only if the raw JSON is unreadable. — Detail column renders `r.detail` verbatim; unknown actions fall back to a neutral badge. No change needed.
- [x] 6.2 Run the full Rust test suite (`cargo test` in `src-tauri`) — all new and existing tests green, including the store round-trip, PII-access, telemetry leak, retention, and correlation tests. — 185 passed, 0 failed.
- [ ] 6.3 Real-data aggregate check: on a synced mirror, load at-risk and Watch List, then open the Audit page and confirm the audit rows list the disclosed `account_id`s and a `cid`, with no household names present.
- [ ] 6.4 UI verification: exercise the chat (send a turn, open a saved conversation), confirm a `chat_telemetry` line appears in the log file with timing and no transcript text, and confirm opening the conversation wrote a read-audit row.
- [ ] 6.5 Confirm no PII leak: grep the written log file for a known household name/email from the test data and assert absence.
