# LLM Provider Settings Foundation Design

Date: 2026-08-26

Status: approved in conversation; pending written review

Part of: **AI Agent** initiative (a tool-calling assistant over membership data). This is
sub-project 1 of 4. Later sub-projects — each with its own design — are:

2. LLM client + provider abstraction (unified Rust client speaking Anthropic / OpenAI / Google /
   Ollama / OpenAI-compatible custom, including each one's tool-calling and streaming format).
3. Agent tool layer + loop (expose safe app commands as agent tools; run the model↔tool loop).
4. Chat UI (the conversation panel where staff ask questions).

## 1. Purpose

The app has no LLM capability today. The eventual goal is a tool-calling agent that answers staff
questions about membership data by calling the app's own commands (`query_segment`, `get_insights`,
`get_at_risk`, …). Before any of that can exist, the app needs a place to **configure and securely
store LLM provider credentials** — modeled on the Settings panel in `llm_wiki`.

This sub-project delivers only that foundation: a Settings surface to choose a provider, save its
key/model/endpoint, prove the connection works, and record the user's acknowledgement that cloud
providers send data externally. Nothing consumes the configuration yet; that is sub-project 3.

## 2. Scope

**In scope**

- Five providers, matching `llm_wiki`: `anthropic`, `openai`, `google`, `ollama`, `custom`
  (OpenAI-compatible).
- Persist non-secret provider config in the existing encrypted `_meta` store.
- Persist API keys in Windows Credential Manager, one entry per provider.
- A `Settings` page (new NAV item) to edit config, set/clear keys, and test a connection.
- A cloud-egress warning + explicit opt-in that gates use of cloud providers.
- A `test_llm_connection` command performing one cheap, PII-free live call per provider.

**Out of scope** (deferred to later sub-projects)

- Any chat, prompting, streaming, or tool-calling.
- Sending membership data to any provider.
- Provider abstraction for actual completions (only the minimal test call exists here).

## 3. Success Criteria

- A user can select any of the five providers, enter its settings, save, and reload the app with
  the settings intact.
- API keys are written only to the keychain; the settings JSON never contains a key, and no command
  ever returns a key to the webview. The UI knows only whether a key is set (`has_key`).
- `test_llm_connection` returns a clear success/failure for each provider and sends no member data.
- Selecting a cloud provider (`anthropic`, `openai`, `google`, `custom`) and saving/testing it is
  blocked until the user checks the egress acknowledgement; `ollama` never requires it.
- Every settings mutation is audited (action name only — never the key value).
- All new Rust logic is covered by unit tests that run without network access.

## 4. Data Model

### 4.1 Non-secret config — `_meta['llm_settings']` (JSON)

```jsonc
{
  "active_provider": "anthropic",        // one of the five, or null
  "cloud_egress_ack": false,             // true once the user acknowledges cloud egress
  "providers": {
    "anthropic": { "model": "claude-sonnet-5", "base_url": "https://api.anthropic.com", "timeout_secs": 60, "headers": {} },
    "openai":    { "model": "gpt-4.1",           "base_url": "https://api.openai.com/v1", "timeout_secs": 60, "headers": {} },
    "google":    { "model": "gemini-2.5-pro",    "base_url": "https://generativelanguage.googleapis.com", "timeout_secs": 60, "headers": {} },
    "ollama":    { "model": "llama3.1",          "base_url": "http://localhost:11434", "timeout_secs": 120, "headers": {} },
    "custom":    { "model": "",                  "base_url": "", "timeout_secs": 60, "headers": {} }
  }
}
```

- Keys are **never** stored here.
- Defaults are seeded when a provider has no saved entry. Model default strings are starting points a
  user can overwrite; they are not validated against a live model list.
- `headers` is a free-form string→string map, used mainly by `custom` (e.g. an auth header for a
  gateway). It must never carry a value the UI treats as a secret; the API key is the only secret and
  lives in the keychain.

### 4.2 Secrets — keychain

One entry per keyed provider, following the existing `secrets.rs` naming convention:

- `llm_key_anthropic`, `llm_key_openai`, `llm_key_google`, `llm_key_custom`
- `ollama` has no key entry.

Constants live in `secrets.rs` alongside `TOKENS` and `DB_KEY`.

### 4.3 View returned to the webview — `LlmSettingsView`

The config above, but with each provider augmented by a derived `has_key: bool` (computed by probing
the keychain) and with no key material anywhere. This mirrors how `get_status` reports connection
state without ever exposing tokens.

## 5. Backend

New module `src-tauri/src/llm.rs` holding the provider enum, defaults, config (de)serialization, and
the test-connection calls. Commands are added to `commands.rs` and registered in `lib.rs`.

### 5.1 Types (`llm.rs`)

- `Provider` enum: `Anthropic | OpenAi | Google | Ollama | Custom`, with `as_str`/`from_str` and:
  - `requires_key()` — `true` for `Anthropic`/`OpenAi`/`Google`; `false` for `Ollama` and `Custom`
    (a custom OpenAI-compatible endpoint may be a keyless local gateway, e.g. LM Studio). When a key
    *is* set for `Custom`, it is sent; when absent, the request omits the auth header.
  - `is_cloud()` — `false` only for `Ollama`. `Custom` is treated as cloud (conservative: its
    locality cannot be proven), so the egress acknowledgement applies to it.
  - `key_name()` — the keychain entry name (`None` for `Ollama`).
- `ProviderConfig { model, base_url, timeout_secs, headers }` with per-provider `default()`.
- `LlmSettings { active_provider: Option<Provider>, cloud_egress_ack: bool, providers: Map<Provider, ProviderConfig> }`
  with `load(store)` / `save(store)` helpers over `_meta` and default-seeding on read.

### 5.2 Commands (`commands.rs`), registered in `lib.rs`

| Command | Behaviour |
|---|---|
| `get_llm_settings() -> LlmSettingsView` | Load config from `_meta`, seed defaults, attach `has_key` per provider by probing the keychain. Never returns a key. |
| `set_llm_settings(settings: LlmSettings)` | Validate and persist config to `_meta`. Reject enabling/saving a cloud provider as `active_provider` when `cloud_egress_ack == false`. Audit `settings.llm.update`. |
| `set_llm_key(provider, key)` | Write `llm_key_<provider>` to the keychain. Reject for `ollama`. Audit `settings.llm.key_set` (no value). |
| `clear_llm_key(provider)` | Delete the keychain entry. Audit `settings.llm.key_cleared`. |
| `test_llm_connection(provider) -> TestResult { ok, detail }` | Perform one cheap, PII-free live call (see 5.3). Load the key from the keychain as needed. For a cloud provider, require `cloud_egress_ack`. |

`TestResult` carries a boolean and a short human string (status code / model count / error message).

### 5.3 Test-connection calls (no member data)

- **anthropic**: `POST {base}/v1/messages` with the configured model, `max_tokens: 1`, and a fixed
  one-word prompt; success on 2xx. Header `x-api-key` + `anthropic-version`.
- **openai** / **custom**: `GET {base}/models`, with `Authorization: Bearer <key>` only when a key is
  set (custom may be keyless); success on 2xx.
- **google**: `GET {base}/v1beta/models?key=<key>`; success on 2xx.
- **ollama**: `GET {base}/api/tags`; success on 2xx (validates the local server is reachable).

HTTP uses the same client stack already used by `salesforce.rs` (`reqwest`). Timeouts come from the
provider config. Failures return `ok: false` with the status or error text — never a panic.

## 6. Frontend

### 6.1 Shell wiring (`App.tsx`)

- Add `"settings"` to `PageKey` and a `{ key: "settings", icon: "settings", label: "Settings" }`
  entry to `NAV`.
- Render `<SettingsPage />` for that key. It receives the standard `PageProps` but ignores Salesforce
  status; it works whenever the app shell is shown.

**Reachability:** the page is signed-in only, matching the current shell (the app is unusable without
Salesforce anyway, and the agent's future tools operate on Salesforce-derived data). Pre-auth
configuration is intentionally deferred.

### 6.2 `SettingsPage.tsx`

An **AI Agent** section (leaving room for future settings groups):

- Provider `<select>` bound to `active_provider`.
- For the selected provider: `model`, `base_url` (prefilled with the default), `timeout_secs`, and —
  for `custom` — an editable headers list.
- API key field: password-style input. When `has_key` is true, show "•••• set" with a **Clear**
  button instead of the value. Hidden for `ollama`.
- **Cloud egress gate:** when the selected provider `is_cloud`, show a warning banner and an
  "I understand this sends congregation data to an external service" checkbox bound to
  `cloud_egress_ack`. Save and Test for a cloud provider are disabled until it is checked.
- **Test connection** button → calls `test_llm_connection`, shows the `TestResult`.
- **Save** button → `set_llm_settings` (and `set_llm_key` when a new key was entered).

Uses existing design-system components (`Button`, `Alert`, inputs) for visual consistency.

### 6.3 `api.ts`

Add typed bindings 1:1 with the new commands: `getLlmSettings`, `setLlmSettings`, `setLlmKey`,
`clearLlmKey`, `testLlmConnection`, plus the `LlmSettingsView` / `ProviderConfig` / `TestResult`
interfaces and a `PROVIDERS` constant.

## 7. Error Handling

- All commands return `Result<_, String>` via the existing `err()` helper; no panics reach the
  webview.
- `test_llm_connection` converts transport/HTTP errors into `TestResult { ok: false, detail }`.
- Saving a cloud provider without acknowledgement returns a specific, user-readable error so the UI
  can surface it even if the client-side gate is bypassed.
- Missing key on a keyed provider surfaces as a `has_key: false` in the view and a clear failure from
  `test_llm_connection`.

## 8. Testing

**Rust (no network):**

- `LlmSettings` round-trips through `_meta` (serialize → `set_meta` → `get_meta` → deserialize) and
  seeds defaults for absent providers.
- `Provider` predicates: `is_cloud` false only for `ollama`; `requires_key` false for `ollama` and
  `custom`; `key_name` mapping.
- Key set/clear against a **test keychain service** (as in the existing `secrets` tests), and
  `has_key` derivation reflecting set/clear.
- The `LlmSettingsView` produced from a config with saved keys contains **no key material**
  (redaction guarantee).
- Cloud-egress guard: `set_llm_settings` rejects a cloud `active_provider` when `cloud_egress_ack`
  is false and accepts it when true.
- Request-builder unit tests where practical (correct URL/headers per provider) without issuing a
  live request.

**Frontend (Vitest, `api.test.ts`):**

- New command bindings invoke the correct Tauri command names with the expected arguments (mocked
  `invoke`), consistent with the existing tests in that file.

Live `test_llm_connection` calls are exercised manually against real providers, not in CI.

## 9. Non-goals / Deviations

- No Tauri Store plugin: config reuses the encrypted `_meta` table rather than adding llm_wiki's
  separate (unencrypted) store, keeping a single encrypted persistence layer.
- No model-list fetching or validation beyond the connection test.
- No consumption of the configuration by any feature; that begins in sub-project 2.
