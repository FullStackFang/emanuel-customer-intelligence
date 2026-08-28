## Context

The app already mirrors Salesforce into an encrypted (SQLCipher) local store and
derives PII-free aggregates in `insights.rs` (trends, cohorts, financials by band/age,
KPIs, retention, channels, schools, outcomes). Two LLM subsystems were partly built on
the `feat/llm-provider-settings` branch:

- `llm.rs` + `SettingsPage.tsx` — API-key providers (Anthropic/OpenAI/Google/Ollama/
  Custom), keys in the OS keychain. This path **only tests connectivity today**; it has
  no chat-completion client. The chat feature will **not** use this path.
- `agent.rs` — drives the **Claude Code** and **Codex** CLIs as subprocesses (headless,
  streaming stdout, timeout, cancel registry, PATH resolution incl. Windows `.cmd`
  shims). Auth is each CLI's own login; **no API key**. Fully unit-tested but **not
  wired** to any Tauri command, UI, or persistence.

The webview is untrusted and reaches data only through fixed Rust commands. Household
names/exports sit behind existing audit + privacy boundaries; the user has repeatedly
required that Financials and similar surfaces stay strictly aggregate (see repo memory
`financials-aggregate-only`, `insights-exec-framing`).

This change finishes and surfaces the `agent.rs` path as a governed chat, adds a keyless
local Ollama backend, and — critically — inserts a data-egress boundary so no raw PII can
reach any model.

## Goals / Non-Goals

**Goals:**
- Let anyone ask natural-language questions about the membership data from a floating,
  always-available chat, with saved conversations.
- Offer three backends, **all keyless**: Ollama (local HTTP), Claude (Claude Code CLI),
  ChatGPT (Codex CLI).
- **Guarantee, by construction and by an automated test, that no raw PII leaves the
  machine.** The only data any backend receives is a de-identified aggregate snapshot.
- Reuse `agent.rs`, the encrypted store, and the existing design system; keep the change
  surgical.

**Non-Goals:**
- No governed tool-calling / live drill-down (a fixed snapshot only, this version).
- No vector/semantic RAG or embeddings.
- No new API-key providers; no use of `llm.rs`'s keychain path for chat.
- No Codex-as-third-coding-agent beyond acting as the "ChatGPT" chat backend.
- No change to how Insights aggregates themselves are computed.

## Decisions

### D1. One governance boundary: a de-identified snapshot is the sole model input
A single Rust builder (`chat_context.rs`) assembles a **GovernedSnapshot** from the
existing PII-free Insights aggregates plus the `CONTEXT.md` data dictionary. This string
is the only data placed in any prompt.

- **Allow-list, not deny-list.** The builder reads only from an explicit set of aggregate
  sources. PII-bearing types (`Hh`, `HhFy`, `AtRiskRow`, watch list, `query_segment`
  member rows) are never referenced by the builder at all.
- **k-anonymity floor N = 5.** Any aggregate cell/group representing fewer than 5
  households is dropped before it enters the snapshot.
- **Automated leak test.** A unit test builds a snapshot over representative data and
  asserts it contains no email pattern, no household-id column value, no personal-name
  field, and no group below the floor. This test *is* the guarantee; it must pass in CI.
- *Alternative considered:* governed tool-calling via a local MCP server (model queries
  aggregates on demand). Rejected for v1 — the guarantee would then require proving a
  negative over the agent's entire tool surface, versus "read the exact prompt string and
  run the leak test." Kept as a documented future extension.
- *Alternative considered:* let the local Ollama backend see raw data (it never egresses).
  Rejected — uniform governance means one code path and one leak test regardless of
  backend, and switching model never changes exposure. A raw-local mode can be a future
  opt-in toggle.

### D2. Two keyless transports behind one `ChatBackend` trait
`trait ChatBackend { async fn stream(snapshot, history, user_msg, on_token, cancel) }`,
with two implementations:

- **CliAgentBackend** (Claude Code, Codex) — reuses `agent.rs` `run_streaming`. Builds
  the prompt = snapshot + history + message, pipes it to stdin, parses the agent's JSON
  event stream for assistant text, emits tokens. Claude emits `stream-json`; Codex emits
  its own `--json` events — a small per-agent parser extracts assistant text from each.
- **OllamaBackend** — net-new keyless HTTP client to `POST /api/chat` (native Ollama
  streaming) on the configured `localhost` base URL. No key, no keychain.

- *Alternative considered:* route ChatGPT through the OpenAI API. Rejected by explicit
  user requirement ("no api keys"); Codex CLI uses ChatGPT login instead.
- *Alternative considered:* one OpenAI-compatible HTTP client covering both Ollama and a
  cloud model. Moot once cloud backends are CLI-only; Ollama uses its native endpoint.

### D3. CLI lockdown — the agents get no route to raw data but the snapshot
Both CLI backends run with data access closed off:
- **Claude Code:** `-p --output-format stream-json --verbose`, `--strict-mcp-config
  --mcp-config {}` (the existing `isolate` flag), a permission mode that forbids
  Bash/Read/Write, and **no `--add-dir`** pointing at anything data-bearing.
- **Codex:** `exec --json --skip-git-repo-check -s read-only`, run with `cwd` set to a
  fresh empty temp directory — never the repo or the store directory.
- The store file and its SQLCipher key are never passed to a subprocess; the encrypted DB
  is unreadable even if a sandbox were escaped.

### D4. Persistence — two tables in the existing encrypted store
```sql
_chat_conversations(id TEXT PRIMARY KEY, backend TEXT, title TEXT,
                    created_at TEXT, updated_at TEXT)
_chat_messages(id TEXT PRIMARY KEY, conversation_id TEXT, role TEXT,
               content TEXT, created_at TEXT)
```
Follows the store's existing `_objects`/`_fields` table convention and lives inside the
same SQLCipher encryption. `purge_mirror` (data wipe) does **not** touch these tables; a
separate "clear chat history" command deletes them. Chat history is independent of the
Insights cache read-model revision, so aggregate rebuilds never invalidate it.

- *Alternative considered:* store threads as JSON blobs in `_meta`. Rejected — real tables
  give clean list/rename/delete and per-message rows without hand-rolled JSON surgery.

### D5. Multi-turn composition
Each send composes `[snapshot] + [prior turns] + [new message]`.
- **Ollama:** stateless — replay full history each turn; snapshot as the system message.
- **Claude Code / Codex:** use the CLI's session continuity (`--resume <session-id>` for
  Claude; keep/replay session for Codex); snapshot prepended on turn one. Session id is
  stored on the conversation row's backend metadata.

### D6. Streaming + cancel wiring
A `chat_send` Tauri command spawns the backend, emits `chat:token` / `chat:done` /
`chat:error` events (same `app.emit` pattern `insights:progress` uses), and registers the
run in the existing `AgentRegistry` so a `chat_cancel` command can abort it. The webview
subscribes and renders tokens incrementally.

### D7. Frontend — a global overlay, not a page
A `ChatWidget` mounted in `App.tsx` outside the page router: a bottom-right FAB toggles a
~380×560 panel (header with backend selector + conversation menu, scrollable streaming
message list, input, new-chat, saved-conversation drawer). Built from existing
design-system components (`Card`, `IconButton`, `Textarea`, `Menu`, `Select`).

## Risks / Trade-offs

- **The guarantee is only as good as the allow-list + leak test.** A future dev adding a
  new aggregate source could accidentally introduce PII. → The leak test runs in CI and
  the builder is the single choke point; the test asserts on patterns (emails, id
  columns, name fields) so new leaks are caught, and the allow-list is documented in the
  spec as the only sanctioned sources.
- **Aggregates still egress to Anthropic/OpenAI for the cloud backends.** This is the
  accepted egress line (raw PII never leaves; de-identified aggregates may). → Documented
  plainly; users who want zero egress select Ollama, which stays fully local.
- **CLI dependency & environment drift.** `claude`/`codex` may be absent, logged out, or
  a version whose flags/JSON shape changed. → Reuse `agent.rs::detect`; surface a clear
  "backend unavailable / not logged in" state in the panel; Ollama likewise reports if the
  local server is down. Per-backend event parsing is isolated so a format change touches
  one function.
- **k-anon floor vs. small congregation.** N = 5 may blank out thin slices (rare cohorts).
  → Acceptable; the snapshot simply omits sub-floor groups, and the model is told totals
  may exclude small groups so it never fabricates them.
- **Latency of cold CLI spawn.** First token can lag while the CLI starts. → Streaming +
  a visible "thinking" state; timeouts already enforced by `run_streaming`.
- **Prompt size.** A large snapshot inflates every turn. → Snapshot is bounded aggregate
  summaries, not row dumps; send once as system context, not per user turn where the
  transport allows.

## Migration Plan

- Additive only: new Rust modules/commands, two new store tables (created idempotently via
  `CREATE TABLE IF NOT EXISTS`, matching the existing `SCHEMA` batch), one new frontend
  overlay. No changes to existing commands, specs, or the Insights computation.
- No data migration; new tables start empty. Rollback = remove the overlay mount, the new
  commands from the invoke handler, and the two tables; nothing else depends on them.
- Feature is inert until a backend is selected and available; absent `claude`/`codex`/
  Ollama, the panel shows an unavailable state and no data is ever assembled or sent.

## Open Questions

- None blocking. Deferred by decision: governed tool-calling drill-down (D1 alternative),
  a raw-local Ollama mode (D1 alternative), and conversation export — all out of scope for
  this change and revisitable later.
