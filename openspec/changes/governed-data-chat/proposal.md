## Why

Staff can read the Insights dashboards but cannot ask the data a question in their
own words ("who is our most profitable cohort?", "which entry jobs retain best?").
A chat surface makes the membership data conversational — but the data includes
household PII, and any cloud model that reads a household row transmits it off the
machine. This change adds the chat *and* a local governance boundary that guarantees
no raw PII ever reaches any model, so the convenience never costs privacy.

## What Changes

- Add a floating, toggleable **chat launcher** (bottom-right, customer-service style)
  available on every page of the app, outside the page router.
- Add a **model selector** with three backends, **none of which use an API key**:
  - **Ollama** — local HTTP (`localhost:11434`); nothing leaves the machine.
  - **Claude** — the **Claude Code CLI** (`claude -p`) as a locked-down subprocess, using its own CLI login.
  - **ChatGPT** — the **Codex CLI** (`codex exec`) as a locked-down subprocess, using its own CLI login.
- Add a **governed aggregate snapshot** built in Rust from the existing PII-free
  Insights aggregates plus the `CONTEXT.md` data dictionary. This snapshot is the
  **only** data any backend ever receives. Raw households, names, emails, addresses,
  at-risk rows, watch lists, and segment member lists are **excluded by construction**,
  and any group smaller than a k-anonymity floor (**N = 5**) is dropped.
- **Lock down both CLI agents** so they cannot reach raw data by any other route
  (no Bash/Read/Write, no MCP, no data working directory) — their sole input is the
  snapshot piped over stdin.
- Add **saved conversations**: new encrypted-store tables, with list / open / rename /
  delete and a "clear chat history" action. Chat history survives aggregate rebuilds
  and is left untouched by the data-mirror purge.
- Add **streaming** replies (token-by-token via Tauri events) and per-message **cancel**.
- Reuse the existing `agent.rs` CLI machinery (already supports Claude Code and Codex)
  and the existing design system; the chat does **not** touch the `llm.rs` API-key /
  keychain path at all.

## Capabilities

### New Capabilities
- `data-chat`: A governed chat surface for asking natural-language questions about the
  membership data — floating launcher, three keyless backends (Ollama / Claude Code /
  Codex), streaming replies, multi-turn conversations, and saved conversation history.
- `chat-data-governance`: The data-egress boundary for chat — a de-identified aggregate
  snapshot as the sole model input, mandatory exclusion of all PII-bearing sources, a
  k-anonymity floor, CLI-agent lockdown, and an automated leak test that proves the
  snapshot carries no identifying data.

### Modified Capabilities
<!-- None. The chat consumes existing Insights aggregates read-only; no existing
     capability's requirements change. -->

## Impact

- **New Rust**: a governed snapshot builder + leak test; Tauri commands to run a chat
  turn (CLI-subprocess and local-HTTP transports) with streaming events and cancel;
  conversation persistence commands. Extends `agent.rs` wiring (currently unwired) and
  adds a keyless Ollama streaming client (net-new; `llm.rs` today only tests connectivity).
- **New store schema**: `_chat_conversations` and `_chat_messages` tables in the existing
  SQLCipher store; `purge_mirror` continues to leave chat history intact.
- **New frontend**: a global chat overlay (launcher + panel + backend selector +
  conversation drawer + streaming render) built from the existing design system, mounted
  in `App.tsx`.
- **Privacy/audit**: introduces a new, auditable egress boundary; raw PII never enters a
  prompt. Cloud backends (Claude Code, Codex) receive de-identified aggregates only;
  Ollama receives the same snapshot but stays fully local.
- **Dependencies**: requires the `claude` and/or `codex` CLIs on PATH for those backends
  (already detected by `agent.rs`); Ollama backend requires a local Ollama server. No new
  API keys or cloud credentials are introduced.
