//! LLM provider settings: types, defaults, validation. No secrets live here —
//! API keys are stored in the keychain, never in this config.

use crate::secrets::Secrets;
use crate::store::Store;
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

#[cfg(test)]
mod tests {
    use super::*;
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
