## Why

The app now holds an encrypted mirror of Membership Household data plus a chat
surface and an action-audit trail, but three review-worthy gaps remain. First,
reads that surface **household identity** — the at-risk list and the Watch List —
are audited only as aggregate counts (`{"count": N}`), so there is no record of
*which* households a staff member viewed; for a customer-intelligence app touching
PII this is the highest-value gap. Second, telemetry is `tracing` to **stdout
only**, so once the desktop process exits there is no diagnostic trail and no record
of chat latency or token usage. Third, stored chat transcripts grow **unbounded**
with no retention policy, and reads of the audit log and chat history are themselves
unaudited. This change closes all three so the app can be reviewed after the fact.

## What Changes

- **Record-level access auditing.** Every read that returns Membership Household
  identity — `get_at_risk`, `get_watch_list`, and their CSV exports — records the
  set of household identifiers (`account_id`) surfaced to the viewer, alongside the
  existing count, so the audit answers "who saw which household, when". Names are
  **not** written to the audit; the household identifier is the key.
- **Audit the reads of sensitive stores.** Reading the audit log (`get_audit`),
  listing conversations, and opening a chat transcript (`chat_list_messages`) each
  write an audit row. These are reads of governance-sensitive material and are
  currently invisible.
- **Persistent, rotating logs.** Add a daily-rotating file log sink under the app
  data directory in addition to stdout, at a controlled level, so diagnostics
  survive process exit. Log lines **never** carry household PII or chat transcript
  content.
- **Chat-turn telemetry.** Emit one structured log event per completed chat turn
  recording backend, elapsed milliseconds, and token counts when the backend
  reports them — never the prompt or reply text.
- **Chat retention policy.** Add a configurable maximum age for stored chat messages
  with a documented default; expired messages (and emptied conversations) are pruned
  so `_chat_messages` no longer grows without bound. A purge action already exists;
  this adds *automatic* age-based expiry.
- **Request correlation.** Thread a per-action correlation id through a user action's
  log events (and, where a record-level audit row is written, onto that row), so one
  action can be reconstructed across the command → store → agent chain.

## Capabilities

### New Capabilities
- `pii-access-audit`: Record-level auditing of every read that surfaces Membership
  Household identity — the audit row names the household identifiers disclosed, the
  viewer, and the time — plus auditing of reads of the audit log and chat transcripts.
- `persistent-logging`: Durable, rotating file logging in addition to stdout, and a
  structured per-chat-turn telemetry event (backend, latency, tokens), with a hard
  guarantee that no household PII or transcript content is ever logged.
- `chat-retention`: An age-based retention policy for stored chat transcripts — a
  documented default maximum age, automatic pruning of expired messages, and removal
  of conversations left empty.
- `request-correlation`: A per-action correlation id threaded through that action's
  log events and onto any record-level audit row it produces, so a single user action
  is reconstructable end to end.

### Modified Capabilities
<!-- None. Record-level auditing, retention, logging, and correlation are additive,
     cross-cutting concerns captured as new capabilities; no existing capability's
     requirements change. The audited read commands keep their current outputs. -->

## Impact

- **Rust — store (`store.rs`):** `Store::audit` gains a way to carry a household
  identifier list and a correlation id in the audit `detail` (append-only insert path
  is preserved; no update/delete added). New retention method to delete
  `_chat_messages` older than a cutoff and remove now-empty conversations. Audit rows
  stay in the existing `_audit` table; no schema-breaking migration.
- **Rust — commands (`commands.rs`):** `get_at_risk`, `get_watch_list`,
  `export_watch_list_csv` extend their audit `detail` with the `account_id` set;
  `get_audit`, `chat_list_conversations`, `chat_list_messages` gain audit rows.
  `chat_send` / `run_chat_stream` emit the per-turn telemetry event and prune expired
  chat history on a schedule (e.g. at conversation open or turn start).
- **Rust — startup (`lib.rs`):** `tracing_subscriber` gains a rotating
  `tracing-appender` file layer alongside the existing stdout layer, keeping the
  `insights_timing` target behavior intact.
- **Frontend:** none required for correctness; `AuditPage.tsx` continues to render
  the richer `detail`. Optionally surface a retention setting later (out of scope).
- **Dependencies:** add `tracing-appender` (Rust). No new API keys, no cloud egress,
  no change to the chat governance boundary or the SQLCipher key handling.
- **Privacy/audit:** strengthens the existing privacy posture — record-level PII
  access becomes accountable, sensitive reads become visible, and logs are constrained
  to carry no identifying data. Retention reduces the standing volume of stored
  transcripts.
