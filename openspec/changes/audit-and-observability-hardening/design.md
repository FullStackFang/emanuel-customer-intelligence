## Context

The app persists an encrypted (SQLCipher) mirror of Salesforce data at
`<app_data_dir>/mirror.db`, keyed from the OS keychain. It already has an
append-only `_audit` table (`Store::audit`, insert-only by design), a chat feature
persisting transcripts in `_chat_conversations` / `_chat_messages`, and `tracing`
logging initialised in `lib.rs`. Three gaps motivate this change (see proposal):

- **Aggregate-only PII access audit.** `get_at_risk` audits `{"count": rows.len()}`
  (`commands.rs:715-720`) and `get_watch_list` audits count + availability
  (`commands.rs:867-872`) while both return rows carrying `account_id` **and** `name`
  (`insights::AtRiskRow`, `risk::WatchRowView`). There is no record of which
  households were disclosed.
- **Ephemeral telemetry.** `tracing_subscriber::fmt()` writes to stdout only
  (`lib.rs:24-28`); nothing survives process exit, and chat turns emit no telemetry.
- **Unbounded transcripts, unaudited reads.** `_chat_messages` has no expiry;
  `get_audit`, `chat_list_conversations`, `chat_list_messages` write no audit row.

Constraints: the webview is untrusted and reaches data only through fixed Rust
commands; the store is opened under a `Mutex` via `with_store` (never across an
await); the audit table must remain append-only (no update/delete path); and no
change may weaken the chat governance boundary or expose the SQLCipher key.

## Goals / Non-Goals

**Goals:**
- Make every disclosure of Membership Household identity accountable at the record
  level — the audit answers "who saw which household, when".
- Make reads of governance-sensitive stores (the audit log, chat transcripts) visible
  in the audit.
- Give the app a durable, rotating log trail and a per-chat-turn telemetry event —
  with a hard guarantee that neither carries PII or transcript content.
- Bound stored chat history with an age-based retention policy.
- Let a single user action be reconstructed across log + audit via a correlation id.

**Non-Goals:**
- No storage-layer tamper-evidence (hash chaining / signing) for `_audit` — remains
  out of scope; append-only-by-construction is retained.
- No per-field mutation history of member data (the mirror stays a full-replace
  snapshot).
- No remote log shipping, metrics backend, or crash-reporting service (e.g. Sentry).
- No new frontend surface beyond `AuditPage.tsx` rendering the richer `detail`
  (a retention **setting** UI is explicitly deferred).
- No change to what data the chat governance boundary exposes.

## Decisions

### 1. Record household identity in the audit `detail` JSON, keyed on `account_id`

Extend the audit `detail` object written by the identity-bearing reads to include the
disclosed household identifiers, e.g. `{"count": N, "account_ids": ["001…", …]}`.

- **Why `account_id`, not `name`:** `account_id` is the stable Membership Household
  identifier and is sufficient to answer "who saw whom" without copying household
  **names** into a second table. This keeps the audit itself free of the most
  sensitive display field, consistent with the app's privacy posture.
- **Why `detail` JSON, not new columns:** `_audit(detail TEXT)` already carries
  free-form JSON; reusing it needs **no schema migration** and no change to
  `list_audit` / `AuditPage.tsx` (which render `detail` verbatim). Adding columns
  would touch the schema and the read path for no functional gain.
- **Alternative considered — a dedicated `_pii_access` table:** rejected as
  over-engineering for the current need; it would duplicate the who/when/what that
  `_audit` already models and fragment the single reviewable trail.
- **Bounding size:** for large result sets the row count can be large. The audit
  stores the full `account_id` set (the point is completeness); this is bounded by the
  Watch List / at-risk sizes, which are small by construction (the Watch List is
  deliberately a short queue). No truncation is applied so the record stays authoritative.

### 2. Audit sensitive reads with a low-cardinality shape

`get_audit`, `chat_list_conversations`, `chat_list_messages` each call `Store::audit`
with a count/id summary (e.g. audit read → `{"limit":…,"offset":…}`; transcript open →
`{"conversation_id":…,"messages":N}`), never message content. Reading the audit log
writes an audit row (a read of the audit log is itself an event); to avoid an infinite
regress, **reading the audit log is audited but the audit-read rows are not recursively
re-audited** — `list_audit` stays a pure read and the row is written by the command.

### 3. Add a rotating file layer to `tracing`, keep stdout

Use `tracing-appender` (`rolling::daily`) writing to `<app_data_dir>/logs/`, composed
with the existing stdout `fmt` layer via `tracing_subscriber::registry().with(...)`.
The `insights_timing` debug target and the `EnvFilter`/`RUST_LOG` default (`info`)
behavior are preserved.

- **Why `tracing-appender`:** it is the first-party companion to the `tracing` stack
  already in `Cargo.toml`; no new logging framework is introduced.
- **Non-blocking:** use its `non_blocking` writer so file I/O never stalls a command;
  hold the returned `WorkerGuard` for the process lifetime (store it in managed state /
  a `static`, since `run()` never returns before exit).
- **PII discipline:** file logging is a **sink** change only. The guarantee that logs
  carry no PII is a property of the **call sites** (Decision 4), not the sink.

### 4. One structured chat-turn telemetry event; content-free by construction

On a completed (non-cancelled) turn, `run_chat_stream` emits
`tracing::info!(target: "chat_telemetry", backend, ms, prompt_tokens?, completion_tokens?, conversation_id)`.
Token counts are included only when the backend reports them (Ollama's response
carries counts; the CLI agents may not — fields are `Option`). The event contains
**no** prompt or reply text. A leak-style unit test asserts the emitted event carries
no message content, mirroring the existing `chat_context.rs` leak-test pattern.

### 5. Age-based chat retention, applied lazily

Add `Store::prune_chat(before: &str) -> Result<usize>` deleting `_chat_messages` with
`created_at < before` and then removing conversations left with zero messages. A
default max age (proposed **365 days**, a named constant) is applied opportunistically
— at conversation open and at turn start — so there is no background timer and no new
lifecycle. `purge_mirror` continues to leave chat tables untouched; this is a separate,
deliberate age policy.

- **Why lazy, not a timer:** the app is a desktop process with no scheduler; pruning
  on the existing chat entry points is simplest and needs no new task. Cost is one
  indexed `DELETE` on paths the user already triggers.
- **Alternative — prune only on explicit action:** rejected; "unbounded until the user
  remembers to clear" is the gap being closed.

### 6. Correlation id per action, propagated to logs and audit

Generate a short id (reuse `new_id("act")`, the existing OS-randomness helper) at the
start of an audited command, attach it to that command's log events, and include it in
the audit `detail` (`{"cid":"act-…", …}`). This ties a log line to its audit row
without a schema change. Full `#[instrument]` span propagation is a possible later
refinement but is not required to reconstruct an action.

## Risks / Trade-offs

- **[Storing `account_id` in the audit widens what the audit holds.]** → The audit is
  inside the same SQLCipher DB as the data it references, so no new plaintext exposure
  is created; `name` is deliberately excluded so the audit never carries the most
  sensitive field. The trade is intentional: accountability requires recording the
  identifier disclosed.
- **[File logs could accidentally capture PII if a future call site logs a row.]** →
  The PII-free property is asserted by a test for the chat-telemetry event and enforced
  by review; the log **level/target** conventions are documented. Logs live under the
  app data dir (same trust zone as the DB), not a shared location.
- **[`tracing-appender` non-blocking guard dropped early loses logs.]** → Hold the
  `WorkerGuard` for the whole process; document that `run()` owns it until exit.
- **[Lazy retention only runs when the user opens chat.]** → Acceptable: retention is a
  volume-control, not a hard legal deadline; if a hard deadline is ever needed, a
  startup prune can be added. Default age is a named constant, easy to change.
- **[Auditing reads increases audit volume.]** → Rows are small and the audit already
  grows unbounded; retention/rotation for `_audit` is noted as a possible follow-up but
  is out of scope here to keep the change focused.

## Migration Plan

- **Schema:** none. All four capabilities reuse existing tables (`_audit.detail` JSON,
  `_chat_messages`). No migration, no data backfill; existing audit rows remain valid
  (they simply lack the new `detail` keys).
- **Dependency:** add `tracing-appender` to `src-tauri/Cargo.toml`.
- **Rollout:** additive and behind no flag; first run after the change starts writing
  the richer audit detail, the file log, and applying retention.
- **Rollback:** revert the change. Existing `mirror.db` files are unaffected — the new
  `detail` keys are ignored by the prior code, the `logs/` directory is inert, and
  pruned rows are already gone (retention is not reversible, which matches its intent).

## Open Questions

- **Default retention age.** Design proposes 365 days as a named constant. Confirm the
  institution's preferred default (or whether retention should be opt-in initially).
- **Correlation-id depth.** This change threads the id through log events + audit
  `detail`. Whether to adopt full `#[instrument]` spans across `command → store → agent`
  is deferred unless reconstruction proves insufficient.
