# LLM Provider Settings Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Settings surface that lets staff configure and securely store LLM provider credentials (Anthropic, OpenAI, Google, Ollama, custom), with a connection test and a cloud-egress opt-in — the foundation the future tool-calling agent will build on.

**Architecture:** Non-secret provider config is a JSON blob in the existing encrypted `_meta` store (`store.rs`); API keys live in Windows Credential Manager via the existing `Secrets` wrapper (`secrets.rs`), one entry per provider. A new `llm.rs` module owns the provider types, defaults, (de)serialization, validation, and PII-free connection tests. New Tauri commands expose it; a new React `SettingsPage` drives it. The webview never receives a key — only a derived `has_key` flag, exactly like `get_status` never returns a token.

**Tech Stack:** Rust + Tauri 2, `reqwest 0.13`, `serde`/`serde_json`, `keyring 4`, `rusqlite`/SQLCipher; React 19 + TypeScript + Vite, Vitest, the in-repo design system.

**Spec:** `docs/superpowers/specs/2026-08-26-llm-provider-settings-design.md`

## Global Constraints

- No new crates or npm packages — use `reqwest`, `serde`, `serde_json`, `keyring`, `anyhow` (Rust) and the existing design system (frontend), all already in the manifests.
- Commands return `Result<T, String>` via the existing `err()` helper; no `panic!`/`unwrap` reaches the webview.
- Never hold the store `Mutex` across an `.await` (follow the `with_store` pattern in `commands.rs`).
- No command ever returns or logs an API key. The settings *view* carries only `has_key: bool`.
- Provider wire strings are exactly: `anthropic`, `openai`, `google`, `ollama`, `custom`.
- `is_cloud()` is false only for `ollama`. `requires_key()` is true only for `anthropic`, `openai`, `google` (Ollama needs none; custom's key is optional).
- Every settings mutation is audited with an action name only (`settings.llm.update` / `settings.llm.key_set` / `settings.llm.key_cleared`), never the value.
- Work happens on branch `feat/llm-provider-settings`. Commit after each task.

---

### Task 1: Provider types, defaults, and validation (`llm.rs`)

Pure data + logic, no I/O. Establishes every type later tasks consume.

**Files:**
- Create: `src-tauri/src/llm.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod llm;` next to the other `pub mod` lines, ~line 4)
- Test: inline `#[cfg(test)]` in `src-tauri/src/llm.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub enum Provider { Anthropic, OpenAi, Google, Ollama, Custom }` — `#[serde(rename_all = "lowercase")]`, `Clone, Copy, PartialEq, Eq, Debug`.
    - `pub fn all() -> [Provider; 5]`
    - `pub fn as_str(&self) -> &'static str`
    - `pub fn requires_key(&self) -> bool`
    - `pub fn is_cloud(&self) -> bool`
    - `pub fn key_name(&self) -> Option<&'static str>`
  - `pub struct ProviderConfig { pub model: String, pub base_url: String, pub timeout_secs: u64, pub headers: BTreeMap<String, String> }` — `Serialize, Deserialize, Clone, Debug, PartialEq`; `pub fn default_for(p: Provider) -> ProviderConfig`.
  - `pub struct LlmSettings { pub active_provider: Option<Provider>, pub cloud_egress_ack: bool, pub anthropic: ProviderConfig, pub openai: ProviderConfig, pub google: ProviderConfig, pub ollama: ProviderConfig, pub custom: ProviderConfig }` — `Serialize, Deserialize, Clone, Debug`, with per-field serde defaults and a manual `Default`.
    - `pub fn config(&self, p: Provider) -> &ProviderConfig`
    - `pub fn validate(&self) -> Result<(), String>`

- [ ] **Step 1: Write the failing tests**

Add to `src-tauri/src/llm.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_predicates_and_names() {
        assert!(Provider::Anthropic.requires_key());
        assert!(Provider::OpenAi.requires_key());
        assert!(Provider::Google.requires_key());
        assert!(!Provider::Ollama.requires_key());
        assert!(!Provider::Custom.requires_key());

        for p in Provider::all() {
            assert_eq!(p.is_cloud(), p != Provider::Ollama);
        }

        assert_eq!(Provider::Anthropic.key_name(), Some("llm_key_anthropic"));
        assert_eq!(Provider::Ollama.key_name(), None);
        assert_eq!(Provider::OpenAi.as_str(), "openai");
    }

    #[test]
    fn provider_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Provider::OpenAi).unwrap(), "\"openai\"");
        let p: Provider = serde_json::from_str("\"custom\"").unwrap();
        assert_eq!(p, Provider::Custom);
    }

    #[test]
    fn defaults_are_per_provider() {
        let s = LlmSettings::default();
        assert_eq!(s.active_provider, None);
        assert!(!s.cloud_egress_ack);
        assert_eq!(s.ollama.base_url, "http://localhost:11434");
        assert_eq!(s.anthropic.base_url, "https://api.anthropic.com");
        assert!(s.custom.base_url.is_empty());
        assert_eq!(s.config(Provider::Ollama).base_url, "http://localhost:11434");
    }

    #[test]
    fn validate_gates_cloud_on_ack() {
        let mut s = LlmSettings::default();
        s.active_provider = Some(Provider::Anthropic);
        assert!(s.validate().is_err(), "cloud provider without ack must fail");
        s.cloud_egress_ack = true;
        assert!(s.validate().is_ok());

        let mut o = LlmSettings::default();
        o.active_provider = Some(Provider::Ollama);
        assert!(o.validate().is_ok(), "ollama never needs ack");
    }

    #[test]
    fn partial_json_fills_missing_providers_with_defaults() {
        // Only anthropic present; others must come back as their defaults.
        let json = r#"{"active_provider":null,"cloud_egress_ack":false,
            "anthropic":{"model":"m","base_url":"https://x","timeout_secs":10,"headers":{}}}"#;
        let s: LlmSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.anthropic.model, "m");
        assert_eq!(s.ollama.base_url, "http://localhost:11434");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test llm::tests`
Expected: FAIL to compile (`llm` module / types not defined).

- [ ] **Step 3: Write the implementation**

At the top of `src-tauri/src/llm.rs`:

```rust
//! LLM provider settings: types, defaults, validation. No secrets live here —
//! API keys are stored in the keychain, never in this config.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    OpenAi,
    Google,
    Ollama,
    Custom,
}

impl Provider {
    pub fn all() -> [Provider; 5] {
        [
            Provider::Anthropic,
            Provider::OpenAi,
            Provider::Google,
            Provider::Ollama,
            Provider::Custom,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::OpenAi => "openai",
            Provider::Google => "google",
            Provider::Ollama => "ollama",
            Provider::Custom => "custom",
        }
    }

    /// Only cloud providers that authenticate with a key strictly require one.
    pub fn requires_key(&self) -> bool {
        matches!(self, Provider::Anthropic | Provider::OpenAi | Provider::Google)
    }

    /// Conservative: everything except a local Ollama is treated as cloud, so the
    /// egress acknowledgement applies (a custom endpoint's locality can't be proven).
    pub fn is_cloud(&self) -> bool {
        !matches!(self, Provider::Ollama)
    }

    pub fn key_name(&self) -> Option<&'static str> {
        match self {
            Provider::Anthropic => Some("llm_key_anthropic"),
            Provider::OpenAi => Some("llm_key_openai"),
            Provider::Google => Some("llm_key_google"),
            Provider::Custom => Some("llm_key_custom"),
            Provider::Ollama => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProviderConfig {
    pub model: String,
    pub base_url: String,
    pub timeout_secs: u64,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

impl ProviderConfig {
    pub fn default_for(p: Provider) -> ProviderConfig {
        let (model, base_url, timeout_secs) = match p {
            Provider::Anthropic => ("claude-sonnet-5", "https://api.anthropic.com", 60),
            Provider::OpenAi => ("gpt-4.1", "https://api.openai.com/v1", 60),
            Provider::Google => (
                "gemini-2.5-pro",
                "https://generativelanguage.googleapis.com",
                60,
            ),
            Provider::Ollama => ("llama3.1", "http://localhost:11434", 120),
            Provider::Custom => ("", "", 60),
        };
        ProviderConfig {
            model: model.to_string(),
            base_url: base_url.to_string(),
            timeout_secs,
            headers: BTreeMap::new(),
        }
    }
}

fn dflt_anthropic() -> ProviderConfig { ProviderConfig::default_for(Provider::Anthropic) }
fn dflt_openai() -> ProviderConfig { ProviderConfig::default_for(Provider::OpenAi) }
fn dflt_google() -> ProviderConfig { ProviderConfig::default_for(Provider::Google) }
fn dflt_ollama() -> ProviderConfig { ProviderConfig::default_for(Provider::Ollama) }
fn dflt_custom() -> ProviderConfig { ProviderConfig::default_for(Provider::Custom) }

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LlmSettings {
    #[serde(default)]
    pub active_provider: Option<Provider>,
    #[serde(default)]
    pub cloud_egress_ack: bool,
    #[serde(default = "dflt_anthropic")]
    pub anthropic: ProviderConfig,
    #[serde(default = "dflt_openai")]
    pub openai: ProviderConfig,
    #[serde(default = "dflt_google")]
    pub google: ProviderConfig,
    #[serde(default = "dflt_ollama")]
    pub ollama: ProviderConfig,
    #[serde(default = "dflt_custom")]
    pub custom: ProviderConfig,
}

impl Default for LlmSettings {
    fn default() -> Self {
        LlmSettings {
            active_provider: None,
            cloud_egress_ack: false,
            anthropic: dflt_anthropic(),
            openai: dflt_openai(),
            google: dflt_google(),
            ollama: dflt_ollama(),
            custom: dflt_custom(),
        }
    }
}

impl LlmSettings {
    pub fn config(&self, p: Provider) -> &ProviderConfig {
        match p {
            Provider::Anthropic => &self.anthropic,
            Provider::OpenAi => &self.openai,
            Provider::Google => &self.google,
            Provider::Ollama => &self.ollama,
            Provider::Custom => &self.custom,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(p) = self.active_provider {
            if p.is_cloud() && !self.cloud_egress_ack {
                return Err(
                    "This provider sends data to an external service. Acknowledge that before enabling it."
                        .to_string(),
                );
            }
        }
        Ok(())
    }
}
```

Add to `src-tauri/src/lib.rs` with the other module declarations (after `pub mod insights;`, keeping alphabetical order):

```rust
pub mod llm;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test llm::tests`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/llm.rs src-tauri/src/lib.rs
git commit -m "feat(llm): provider types, per-provider defaults, egress validation"
```

---

### Task 2: Persistence + redacted view (`llm.rs`)

Load/save config through `_meta`, and build the key-free view for the webview.

**Files:**
- Modify: `src-tauri/src/llm.rs`
- Test: inline `#[cfg(test)]` in `src-tauri/src/llm.rs`

**Interfaces:**
- Consumes: `Provider`, `ProviderConfig`, `LlmSettings` (Task 1); `crate::store::{Store, open}`, `Store::{get_meta, set_meta}`; `crate::secrets::Secrets` with `get(name) -> Result<Option<String>>`.
- Produces:
  - `pub const META_KEY: &str = "llm_settings";`
  - `LlmSettings::load(store: &Store) -> anyhow::Result<LlmSettings>`
  - `LlmSettings::save(&self, store: &Store) -> anyhow::Result<()>`
  - `pub struct ProviderView { pub provider: Provider, pub config: ProviderConfig, pub has_key: bool }` — `Serialize`.
  - `pub struct LlmSettingsView { pub active_provider: Option<Provider>, pub cloud_egress_ack: bool, pub providers: Vec<ProviderView> }` — `Serialize`.
  - `LlmSettings::to_view(&self, secrets: &Secrets) -> anyhow::Result<LlmSettingsView>`

- [ ] **Step 1: Write the failing tests**

Add these tests inside the existing `mod tests` in `llm.rs`:

```rust
    use crate::secrets::Secrets;
    use crate::store::{self, Store};

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn mem_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = store::open(&dir.path().join("t.db"), KEY).unwrap();
        (dir, s)
    }

    fn test_secrets() -> Secrets {
        Secrets::new("emanuel-customer-intelligence-llm-test")
    }

    #[test]
    fn load_returns_defaults_when_absent_then_round_trips() {
        let (_d, s) = mem_store();
        let loaded = LlmSettings::load(&s).unwrap();
        assert_eq!(loaded.active_provider, None);

        let mut settings = LlmSettings::default();
        settings.active_provider = Some(Provider::Ollama);
        settings.ollama.model = "mistral".into();
        settings.save(&s).unwrap();

        let back = LlmSettings::load(&s).unwrap();
        assert_eq!(back.active_provider, Some(Provider::Ollama));
        assert_eq!(back.ollama.model, "mistral");
    }

    #[test]
    fn view_reports_has_key_and_never_leaks_the_key() {
        let secrets = test_secrets();
        secrets.set("llm_key_openai", "sk-secret-123").unwrap();
        secrets.delete("llm_key_anthropic").unwrap();

        let settings = LlmSettings::default();
        let view = settings.to_view(&secrets).unwrap();

        let openai = view.providers.iter().find(|p| p.provider == Provider::OpenAi).unwrap();
        let anthropic = view.providers.iter().find(|p| p.provider == Provider::Anthropic).unwrap();
        let ollama = view.providers.iter().find(|p| p.provider == Provider::Ollama).unwrap();
        assert!(openai.has_key);
        assert!(!anthropic.has_key);
        assert!(!ollama.has_key, "keyless provider is always has_key=false");

        let serialized = serde_json::to_string(&view).unwrap();
        assert!(!serialized.contains("sk-secret-123"), "view must not contain key material");

        secrets.delete("llm_key_openai").unwrap();
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test llm::tests`
Expected: FAIL to compile (`load`, `save`, `to_view`, `META_KEY`, `ProviderView` undefined).

- [ ] **Step 3: Write the implementation**

Add to `llm.rs` (after the `impl LlmSettings` from Task 1, plus new imports at the top of the file):

```rust
use crate::secrets::Secrets;
use crate::store::Store;

pub const META_KEY: &str = "llm_settings";

#[derive(Serialize, Debug)]
pub struct ProviderView {
    pub provider: Provider,
    pub config: ProviderConfig,
    pub has_key: bool,
}

#[derive(Serialize, Debug)]
pub struct LlmSettingsView {
    pub active_provider: Option<Provider>,
    pub cloud_egress_ack: bool,
    pub providers: Vec<ProviderView>,
}

impl LlmSettings {
    pub fn load(store: &Store) -> anyhow::Result<LlmSettings> {
        match store.get_meta(META_KEY)? {
            Some(json) => Ok(serde_json::from_str(&json)?),
            None => Ok(LlmSettings::default()),
        }
    }

    pub fn save(&self, store: &Store) -> anyhow::Result<()> {
        store.set_meta(META_KEY, &serde_json::to_string(self)?)
    }

    pub fn to_view(&self, secrets: &Secrets) -> anyhow::Result<LlmSettingsView> {
        let mut providers = Vec::with_capacity(5);
        for p in Provider::all() {
            let has_key = match p.key_name() {
                Some(name) => secrets.get(name)?.is_some(),
                None => false,
            };
            providers.push(ProviderView {
                provider: p,
                config: self.config(p).clone(),
                has_key,
            });
        }
        Ok(LlmSettingsView {
            active_provider: self.active_provider,
            cloud_egress_ack: self.cloud_egress_ack,
            providers,
        })
    }
}
```

Note: the two `use` lines above may duplicate imports if you place them mid-file — put all `use` statements together at the top of `llm.rs` and keep only one of each.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test llm::tests`
Expected: PASS (7 tests total).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/llm.rs
git commit -m "feat(llm): persist settings in _meta and build key-free view"
```

---

### Task 3: Test-request builder + runner (`llm.rs`)

The PII-free connection test. The URL/method/header logic is pure and unit-tested; the network call is a thin async wrapper.

**Files:**
- Modify: `src-tauri/src/llm.rs`
- Test: inline `#[cfg(test)]` in `src-tauri/src/llm.rs`

**Interfaces:**
- Consumes: `Provider`, `ProviderConfig` (Task 1).
- Produces:
  - `pub struct TestRequest { pub method: reqwest::Method, pub url: String, pub headers: Vec<(String, String)>, pub body: Option<serde_json::Value> }`
  - `pub fn build_test_request(p: Provider, c: &ProviderConfig, key: Option<&str>) -> Result<TestRequest, String>`
  - `pub struct TestResult { pub ok: bool, pub detail: String }` — `Serialize`.
  - `pub async fn run_test(p: Provider, c: &ProviderConfig, key: Option<&str>) -> TestResult`

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests` in `llm.rs`:

```rust
    #[test]
    fn build_test_request_per_provider() {
        // Anthropic: POST messages, keyed, 1-token body.
        let c = ProviderConfig::default_for(Provider::Anthropic);
        let r = build_test_request(Provider::Anthropic, &c, Some("k")).unwrap();
        assert_eq!(r.method, reqwest::Method::POST);
        assert_eq!(r.url, "https://api.anthropic.com/v1/messages");
        assert!(r.headers.iter().any(|(k, v)| k == "x-api-key" && v == "k"));
        assert!(r.headers.iter().any(|(k, _)| k == "anthropic-version"));
        assert_eq!(r.body.as_ref().unwrap()["max_tokens"], serde_json::json!(1));

        // OpenAI: GET models with bearer.
        let c = ProviderConfig::default_for(Provider::OpenAi);
        let r = build_test_request(Provider::OpenAi, &c, Some("k")).unwrap();
        assert_eq!(r.method, reqwest::Method::GET);
        assert_eq!(r.url, "https://api.openai.com/v1/models");
        assert!(r.headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer k"));

        // Google: key in query string.
        let c = ProviderConfig::default_for(Provider::Google);
        let r = build_test_request(Provider::Google, &c, Some("k")).unwrap();
        assert_eq!(r.url, "https://generativelanguage.googleapis.com/v1beta/models?key=k");

        // Ollama: GET tags, no key, no auth header.
        let c = ProviderConfig::default_for(Provider::Ollama);
        let r = build_test_request(Provider::Ollama, &c, None).unwrap();
        assert_eq!(r.url, "http://localhost:11434/api/tags");
        assert!(r.headers.iter().all(|(k, _)| k != "Authorization"));
    }

    #[test]
    fn build_test_request_custom_key_optional_and_base_trimmed() {
        let mut c = ProviderConfig::default_for(Provider::Custom);
        c.base_url = "http://localhost:1234/v1/".into(); // trailing slash
        let r = build_test_request(Provider::Custom, &c, None).unwrap();
        assert_eq!(r.url, "http://localhost:1234/v1/models");
        assert!(r.headers.iter().all(|(k, _)| k != "Authorization"), "no key -> no auth");

        let r2 = build_test_request(Provider::Custom, &c, Some("k")).unwrap();
        assert!(r2.headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer k"));
    }

    #[test]
    fn build_test_request_requires_key_for_keyed_cloud() {
        let c = ProviderConfig::default_for(Provider::OpenAi);
        assert!(build_test_request(Provider::OpenAi, &c, None).is_err());
        let c = ProviderConfig::default_for(Provider::Anthropic);
        assert!(build_test_request(Provider::Anthropic, &c, None).is_err());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test llm::tests`
Expected: FAIL to compile (`build_test_request`, `TestRequest` undefined).

- [ ] **Step 3: Write the implementation**

Add to `llm.rs`:

```rust
pub struct TestRequest {
    pub method: reqwest::Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<serde_json::Value>,
}

#[derive(Serialize, Debug)]
pub struct TestResult {
    pub ok: bool,
    pub detail: String,
}

fn base(c: &ProviderConfig) -> &str {
    c.base_url.trim().trim_end_matches('/')
}

pub fn build_test_request(
    p: Provider,
    c: &ProviderConfig,
    key: Option<&str>,
) -> Result<TestRequest, String> {
    if p.requires_key() && key.map(|k| k.trim().is_empty()).unwrap_or(true) {
        return Err("No API key is set for this provider.".to_string());
    }
    let b = base(c);
    if b.is_empty() {
        return Err("Base URL is empty.".to_string());
    }
    Ok(match p {
        Provider::Anthropic => TestRequest {
            method: reqwest::Method::POST,
            url: format!("{b}/v1/messages"),
            headers: vec![
                ("x-api-key".into(), key.unwrap_or_default().to_string()),
                ("anthropic-version".into(), "2023-06-01".into()),
                ("content-type".into(), "application/json".into()),
            ],
            body: Some(serde_json::json!({
                "model": c.model,
                "max_tokens": 1,
                "messages": [{ "role": "user", "content": "ping" }],
            })),
        },
        Provider::OpenAi | Provider::Custom => {
            let mut headers = Vec::new();
            if let Some(k) = key.filter(|k| !k.trim().is_empty()) {
                headers.push(("Authorization".into(), format!("Bearer {k}")));
            }
            TestRequest {
                method: reqwest::Method::GET,
                url: format!("{b}/models"),
                headers,
                body: None,
            }
        }
        Provider::Google => TestRequest {
            method: reqwest::Method::GET,
            url: format!("{b}/v1beta/models?key={}", key.unwrap_or_default()),
            headers: vec![],
            body: None,
        },
        Provider::Ollama => TestRequest {
            method: reqwest::Method::GET,
            url: format!("{b}/api/tags"),
            headers: vec![],
            body: None,
        },
    })
}

pub async fn run_test(p: Provider, c: &ProviderConfig, key: Option<&str>) -> TestResult {
    let req = match build_test_request(p, c, key) {
        Ok(r) => r,
        Err(e) => return TestResult { ok: false, detail: e },
    };
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(c.timeout_secs.max(1)))
        .build()
    {
        Ok(c) => c,
        Err(e) => return TestResult { ok: false, detail: e.to_string() },
    };
    let mut rb = client.request(req.method, &req.url);
    for (k, v) in req.headers {
        rb = rb.header(k, v);
    }
    if let Some(body) = req.body {
        rb = rb.json(&body);
    }
    match rb.send().await {
        Ok(resp) if resp.status().is_success() => TestResult {
            ok: true,
            detail: format!("OK ({})", resp.status().as_u16()),
        },
        Ok(resp) => TestResult {
            ok: false,
            detail: format!("HTTP {}", resp.status().as_u16()),
        },
        Err(e) => TestResult { ok: false, detail: e.to_string() },
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test llm::tests`
Expected: PASS (10 tests total). `run_test` is exercised manually later, not in CI.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/llm.rs
git commit -m "feat(llm): PII-free per-provider connection test builder and runner"
```

---

### Task 4: Tauri commands + registration (`commands.rs`, `lib.rs`)

Expose the module through the command boundary, with auditing and the egress guard.

**Files:**
- Modify: `src-tauri/src/commands.rs` (add commands near the other command fns; add `use crate::llm;`)
- Modify: `src-tauri/src/lib.rs` (register the 5 commands in `generate_handler!`)
- Test: covered by the pure `llm.rs` tests (validate/build); command wiring verified by `cargo build` + the manual pass in Task 6.

**Interfaces:**
- Consumes: `with_store`, `who`, `err`, `AppState`, `CmdResult` (existing in `commands.rs`); everything from `llm` (Tasks 1–3); `Secrets::{set, delete, get}`.
- Produces (Tauri commands): `get_llm_settings`, `set_llm_settings`, `set_llm_key`, `clear_llm_key`, `test_llm_connection`.

- [ ] **Step 1: Add `use` and the commands to `commands.rs`**

Add near the top with the other `use crate::…` lines:

```rust
use crate::llm;
```

Add at the end of `commands.rs` (before any trailing test module, if present):

```rust
// ── LLM provider settings ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_llm_settings(state: State<'_, AppState>) -> CmdResult<llm::LlmSettingsView> {
    let settings = with_store(state.inner(), |s| llm::LlmSettings::load(s))?;
    settings.to_view(&state.secrets).map_err(err)
}

#[tauri::command]
pub async fn set_llm_settings(
    state: State<'_, AppState>,
    settings: llm::LlmSettings,
) -> CmdResult<()> {
    settings.validate().map_err(err)?;
    with_store(state.inner(), |s| {
        settings.save(s)?;
        s.audit(&who(state.inner()), "settings.llm.update", None, None)?;
        Ok(())
    })
}

#[tauri::command]
pub async fn set_llm_key(
    state: State<'_, AppState>,
    provider: llm::Provider,
    key: String,
) -> CmdResult<()> {
    let name = provider
        .key_name()
        .ok_or_else(|| "This provider does not use an API key.".to_string())?;
    if key.trim().is_empty() {
        return Err(err("API key is empty."));
    }
    state.secrets.set(name, &key).map_err(err)?;
    with_store(state.inner(), |s| {
        s.audit(
            &who(state.inner()),
            "settings.llm.key_set",
            Some(provider.as_str()),
            None,
        )
    })
}

#[tauri::command]
pub async fn clear_llm_key(
    state: State<'_, AppState>,
    provider: llm::Provider,
) -> CmdResult<()> {
    if let Some(name) = provider.key_name() {
        state.secrets.delete(name).map_err(err)?;
    }
    with_store(state.inner(), |s| {
        s.audit(
            &who(state.inner()),
            "settings.llm.key_cleared",
            Some(provider.as_str()),
            None,
        )
    })
}

#[tauri::command]
pub async fn test_llm_connection(
    state: State<'_, AppState>,
    provider: llm::Provider,
) -> CmdResult<llm::TestResult> {
    // Read config + key without holding the store lock across the network await.
    let settings = with_store(state.inner(), |s| llm::LlmSettings::load(s))?;
    if provider.is_cloud() && !settings.cloud_egress_ack {
        return Err("Acknowledge external data egress before testing this provider.".to_string());
    }
    let config = settings.config(provider).clone();
    let key = match provider.key_name() {
        Some(name) => state.secrets.get(name).map_err(err)?,
        None => None,
    };
    Ok(llm::run_test(provider, &config, key.as_deref()).await)
}
```

- [ ] **Step 2: Register the commands in `lib.rs`**

In the `tauri::generate_handler![ … ]` list (ends ~line 57), add these entries after `commands::export_insights_pdf,`:

```rust
            commands::get_llm_settings,
            commands::set_llm_settings,
            commands::set_llm_key,
            commands::clear_llm_key,
            commands::test_llm_connection,
```

- [ ] **Step 3: Build to verify the command wiring compiles**

Run: `cd src-tauri && cargo build`
Expected: builds clean. Fix any type/borrow errors (common: pass `state.inner()` to `with_store`/`who`, matching the existing commands).

- [ ] **Step 4: Run the full Rust test suite**

Run: `cd src-tauri && cargo test`
Expected: PASS, including all `llm::tests` and the pre-existing suites.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(llm): expose provider settings commands with audit and egress guard"
```

---

### Task 5: Frontend API bindings + tests (`api.ts`, `api.test.ts`)

Typed wrappers 1:1 with the new commands.

**Files:**
- Modify: `src/api.ts`
- Test: `src/api.test.ts`

**Interfaces:**
- Consumes: `invoke` (already imported in `api.ts`); command names from Task 4.
- Produces: types `LlmProvider`, `ProviderConfig`, `ProviderView`, `LlmSettingsView`, `LlmSettings`, `TestResult`; const `PROVIDERS`; functions `getLlmSettings`, `setLlmSettings`, `setLlmKey`, `clearLlmKey`, `testLlmConnection`.

- [ ] **Step 1: Write the failing test**

Add a new `it` block to the existing `describe` in `src/api.test.ts`:

```ts
  it("llm settings wrappers use the exact command names", async () => {
    invoke.mockResolvedValue(undefined);
    const cfg = { model: "m", base_url: "u", timeout_secs: 60, headers: {} };
    const settings = {
      active_provider: "anthropic" as const, cloud_egress_ack: true,
      anthropic: cfg, openai: cfg, google: cfg, ollama: cfg, custom: cfg,
    };
    await api.getLlmSettings();
    await api.setLlmSettings(settings);
    await api.setLlmKey("openai", "sk-x");
    await api.clearLlmKey("openai");
    await api.testLlmConnection("ollama");
    expect(invoke.mock.calls).toEqual([
      ["get_llm_settings"],
      ["set_llm_settings", { settings }],
      ["set_llm_key", { provider: "openai", key: "sk-x" }],
      ["clear_llm_key", { provider: "openai" }],
      ["test_llm_connection", { provider: "ollama" }],
    ]);
    expect([...api.PROVIDERS]).toEqual(["anthropic", "openai", "google", "ollama", "custom"]);
  });
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm test -- api.test`
Expected: FAIL (`api.getLlmSettings` is not a function).

- [ ] **Step 3: Add the bindings to `api.ts`**

Append to `src/api.ts`:

```ts
export type LlmProvider = "anthropic" | "openai" | "google" | "ollama" | "custom";
export const PROVIDERS: LlmProvider[] = ["anthropic", "openai", "google", "ollama", "custom"];

export interface ProviderConfig {
  model: string; base_url: string; timeout_secs: number; headers: Record<string, string>;
}
export interface ProviderView { provider: LlmProvider; config: ProviderConfig; has_key: boolean }
export interface LlmSettingsView {
  active_provider: LlmProvider | null; cloud_egress_ack: boolean; providers: ProviderView[];
}
export interface LlmSettings {
  active_provider: LlmProvider | null; cloud_egress_ack: boolean;
  anthropic: ProviderConfig; openai: ProviderConfig; google: ProviderConfig;
  ollama: ProviderConfig; custom: ProviderConfig;
}
export interface TestResult { ok: boolean; detail: string }

export const getLlmSettings = () => invoke<LlmSettingsView>("get_llm_settings");
export const setLlmSettings = (settings: LlmSettings) => invoke<void>("set_llm_settings", { settings });
export const setLlmKey = (provider: LlmProvider, key: string) =>
  invoke<void>("set_llm_key", { provider, key });
export const clearLlmKey = (provider: LlmProvider) => invoke<void>("clear_llm_key", { provider });
export const testLlmConnection = (provider: LlmProvider) =>
  invoke<TestResult>("test_llm_connection", { provider });
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npm test -- api.test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/api.ts src/api.test.ts
git commit -m "feat(llm): typed api bindings for provider settings commands"
```

---

### Task 6: Settings page + NAV wiring (`SettingsPage.tsx`, `App.tsx`)

The UI. Verified by typecheck + a manual run (no meaningful unit surface).

**Files:**
- Create: `src/pages/SettingsPage.tsx`
- Modify: `src/App.tsx` (extend `PageKey`, add a `NAV` entry, render the page)

**Interfaces:**
- Consumes: everything from Task 5; design-system `Button, Alert, Card, Field, Input, Select`; `PageTitle` from `chrome.jsx`; `PageProps` from `App`.
- Produces: default-exported `SettingsPage` component; `"settings"` member of `PageKey`.

- [ ] **Step 1: Wire the page into the shell (`App.tsx`)**

- Change the `PageKey` type (line 13) to include `"settings"`:

```tsx
export type PageKey = "overview" | "data" | "segments" | "insights" | "audit" | "settings";
```

- Add the import with the other page imports (after the `AuditPage` import):

```tsx
import SettingsPage from "./pages/SettingsPage";
```

- Add a `NAV` entry (after the `audit` entry):

```tsx
  { key: "settings", icon: "settings", label: "Settings" },
```

- Add the render line (after the `audit` line inside `AppFrame`):

```tsx
      {page === "settings" && <SettingsPage {...props} />}
```

- [ ] **Step 2: Create `src/pages/SettingsPage.tsx`**

```tsx
import { useEffect, useState } from "react";
import type { PageProps } from "../App";
import * as api from "../api";
import { Alert, Button, Card, Field, Input, Select } from "../design-system";
import { PageTitle } from "../design-system/ui-kits/grant-management/chrome.jsx";

const CLOUD: Record<api.LlmProvider, boolean> = {
  anthropic: true, openai: true, google: true, ollama: false, custom: true,
};
const USES_KEY: Record<api.LlmProvider, boolean> = {
  anthropic: true, openai: true, google: true, ollama: false, custom: true,
};
const LABEL: Record<api.LlmProvider, string> = {
  anthropic: "Anthropic (Claude)", openai: "OpenAI", google: "Google (Gemini)",
  ollama: "Ollama (local)", custom: "Custom (OpenAI-compatible)",
};

// Rebuild the full LlmSettings the backend expects from the array-shaped view.
function toSettings(view: api.LlmSettingsView): api.LlmSettings {
  const byProvider = (p: api.LlmProvider) =>
    view.providers.find((x) => x.provider === p)!.config;
  return {
    active_provider: view.active_provider,
    cloud_egress_ack: view.cloud_egress_ack,
    anthropic: byProvider("anthropic"), openai: byProvider("openai"),
    google: byProvider("google"), ollama: byProvider("ollama"), custom: byProvider("custom"),
  };
}

export default function SettingsPage(_props: PageProps) {
  const [view, setView] = useState<api.LlmSettingsView | null>(null);
  const [selected, setSelected] = useState<api.LlmProvider>("anthropic");
  const [keyInput, setKeyInput] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [test, setTest] = useState<api.TestResult | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () =>
    api.getLlmSettings()
      .then((v) => { setView(v); if (v.active_provider) setSelected(v.active_provider); })
      .catch((e) => setErr(String(e)));

  useEffect(() => { void load(); }, []);
  if (!view) return null;

  const current = view.providers.find((p) => p.provider === selected)!;
  const patchConfig = (p: Partial<api.ProviderConfig>) =>
    setView({
      ...view,
      providers: view.providers.map((x) =>
        x.provider === selected ? { ...x, config: { ...x.config, ...p } } : x),
    });

  const cloudBlocked = CLOUD[selected] && !view.cloud_egress_ack;

  const save = async () => {
    setErr(null); setMsg(null); setBusy(true);
    try {
      await api.setLlmSettings({ ...toSettings(view), active_provider: selected });
      if (USES_KEY[selected] && keyInput.trim()) {
        await api.setLlmKey(selected, keyInput.trim());
        setKeyInput("");
      }
      await load();
      setMsg("Saved.");
    } catch (e) { setErr(String(e)); }
    finally { setBusy(false); }
  };

  const runTest = async () => {
    setErr(null); setMsg(null); setTest(null); setBusy(true);
    try { setTest(await api.testLlmConnection(selected)); }
    catch (e) { setErr(String(e)); }
    finally { setBusy(false); }
  };

  const clearKey = async () => {
    setBusy(true);
    try { await api.clearLlmKey(selected); await load(); }
    catch (e) { setErr(String(e)); }
    finally { setBusy(false); }
  };

  return (
    <div style={{ width: "100%", maxWidth: 720, margin: "0 auto" }}>
      <PageTitle eyebrow="Customer Intelligence" title="Settings" actions={undefined} />
      {err && <Alert tone="error" style={{ marginBottom: "var(--space-4)" }}>{err}</Alert>}
      {msg && <Alert tone="success" style={{ marginBottom: "var(--space-4)" }}>{msg}</Alert>}

      <Card>
        <h2 style={{ fontFamily: "var(--font-display)", fontSize: "var(--text-lg)", margin: "0 0 var(--space-4)" }}>
          AI Agent
        </h2>

        <Field label="Provider">
          <Select
            value={selected}
            options={api.PROVIDERS.map((p) => ({ value: p, label: LABEL[p] }))}
            children={undefined}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
              setSelected(e.target.value as api.LlmProvider); setTest(null); setKeyInput("");
            }}
          />
        </Field>

        <Field label="Model">
          <Input value={current.config.model}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => patchConfig({ model: e.target.value })} />
        </Field>

        <Field label="Base URL">
          <Input value={current.config.base_url}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => patchConfig({ base_url: e.target.value })} />
        </Field>

        <Field label="Timeout (seconds)">
          <Input type="number" value={String(current.config.timeout_secs)}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
              patchConfig({ timeout_secs: Number(e.target.value) || 0 })} />
        </Field>

        {USES_KEY[selected] && (
          <Field label="API key">
            {current.has_key
              ? (<div style={{ display: "flex", gap: "var(--space-3)", alignItems: "center" }}>
                  <span style={{ color: "var(--text-secondary)", fontSize: "var(--text-sm)" }}>•••• set</span>
                  <Button variant="secondary" size="sm" disabled={busy} onClick={clearKey}>Clear</Button>
                </div>)
              : (<Input type="password" value={keyInput} placeholder="Paste key"
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) => setKeyInput(e.target.value)} />)}
          </Field>
        )}

        {CLOUD[selected] && (
          <Alert tone="warning" style={{ margin: "var(--space-4) 0" }}>
            <label style={{ display: "flex", gap: "var(--space-2)", alignItems: "flex-start" }}>
              <input type="checkbox" checked={view.cloud_egress_ack}
                onChange={(e) => setView({ ...view, cloud_egress_ack: e.target.checked })} />
              <span>I understand this provider sends congregation data to an external service.</span>
            </label>
          </Alert>
        )}

        <div style={{ display: "flex", gap: "var(--space-3)", marginTop: "var(--space-4)" }}>
          <Button disabled={busy || cloudBlocked} onClick={save}>Save</Button>
          <Button variant="secondary" disabled={busy || cloudBlocked} onClick={runTest}>Test connection</Button>
        </div>

        {test && (
          <Alert tone={test.ok ? "success" : "error"} style={{ marginTop: "var(--space-4)" }}>
            {test.ok ? "Connection OK" : "Connection failed"} — {test.detail}
          </Alert>
        )}
      </Card>
    </div>
  );
}
```

- [ ] **Step 3: Typecheck**

Run: `npm run typecheck`
Expected: no errors. If `Field`'s prop is named differently than `label`, or `Alert` lacks a `warning`/`success` tone, adjust to the design system's actual API (open `src/design-system/components/forms/Field.jsx` and `.../feedback/Alert.jsx` to confirm prop names and available tones).

- [ ] **Step 4: Manual verification (real app)**

Run: `npm run tauri dev`
Confirm:
1. A **Settings** item appears in the sidebar with a gear icon; clicking it opens the page. (If the `settings` icon doesn't render, pick another valid lucide name like `sliders-horizontal` in the `NAV` entry.)
2. Select **Ollama**, set base URL to a running instance (or expect a failure), click **Test connection** → a result Alert appears; no egress checkbox is shown.
3. Select **Anthropic** → the egress warning + checkbox appears; **Save**/**Test** are disabled until it's checked.
4. Check the box, paste a key, **Save** → reload the app; the key field shows "•••• set" and the checkbox stays checked (persistence works).
5. **Clear** removes the key (field returns to an input on reload).
6. Open the **Audit** page → `settings.llm.update` / `settings.llm.key_set` rows are present with no key value in the detail.

- [ ] **Step 5: Commit**

```bash
git add src/pages/SettingsPage.tsx src/App.tsx
git commit -m "feat(llm): Settings page for provider config, keys, and connection test"
```

---

## Final verification

- [ ] Run the full gate: `npm run verify` (typecheck + vitest + `cargo test`). Expected: all green.
- [ ] Confirm `git status` shows only the intended files changed on `feat/llm-provider-settings`.

## Self-Review notes (author)

- **Spec coverage:** §4.1 config → Task 1/2; §4.2 keychain → Task 2/4; §4.3 view/redaction → Task 2 (+ redaction test); §5.2 commands → Task 4; §5.3 test calls → Task 3; §6 frontend → Task 5/6; §7 error handling → `Result<_,String>` + `run_test` never errs (Tasks 3/4); §8 testing → per-task tests; §2 egress opt-in → `validate()` (Task 1) + command guard (Task 4) + UI gate (Task 6).
- **Deferred, by design:** no completions/streaming/tool-calling; that is sub-project 2+.
- **Naming consistency:** `LlmSettings`/`LlmSettingsView`/`ProviderView`/`ProviderConfig`/`TestResult`/`build_test_request`/`run_test` are used identically across backend tasks; frontend `getLlmSettings`/`setLlmSettings`/`setLlmKey`/`clearLlmKey`/`testLlmConnection` match the command names registered in Task 4.
