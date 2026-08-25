//! Salesforce REST client. Every call goes through `get_json`, which refreshes
//! the token once on 401. Only reads; there is deliberately no POST/PATCH here.

use crate::auth::{self, TokenSet};
use crate::config::Config;
use crate::secrets::Secrets;
use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;

pub const API_VERSION: &str = "v62.0";
pub type Row = serde_json::Map<String, serde_json::Value>;

#[derive(Deserialize, Clone, Debug)]
pub struct SObjectMeta {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub queryable: bool,
    #[serde(rename = "customSetting", default)]
    pub custom_setting: bool,
    #[serde(rename = "deprecatedAndHidden", default)]
    pub deprecated_and_hidden: bool,
}

#[derive(Deserialize)]
pub struct GlobalDescribe {
    pub sobjects: Vec<SObjectMeta>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct FieldMeta {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Deserialize)]
struct ObjectDescribe {
    fields: Vec<FieldMeta>,
}

#[derive(Deserialize)]
struct QueryPage {
    #[serde(default)]
    records: Vec<Row>,
    #[serde(rename = "nextRecordsUrl")]
    next: Option<String>,
    #[serde(default)]
    done: bool,
    #[serde(rename = "totalSize", default)]
    total_size: i64,
}

pub fn mirrorable(o: &SObjectMeta) -> bool {
    o.queryable && !o.custom_setting && !o.deprecated_and_hidden
}

pub fn selectable(f: &FieldMeta) -> bool {
    !matches!(f.field_type.as_str(), "address" | "location" | "base64")
}

pub struct SfClient {
    http: reqwest::Client,
    cfg: Config,
    secrets: Secrets,
    tokens: TokenSet,
}

impl SfClient {
    pub fn new(cfg: Config, secrets: Secrets, tokens: TokenSet) -> SfClient {
        SfClient {
            http: reqwest::Client::new(),
            cfg,
            secrets,
            tokens,
        }
    }

    pub fn tokens(&self) -> &TokenSet {
        &self.tokens
    }

    fn api(&self, path: &str) -> String {
        format!(
            "{}/services/data/{API_VERSION}{path}",
            self.tokens.instance_url
        )
    }

    async fn get_json<T: DeserializeOwned>(&mut self, url: &str) -> Result<T> {
        for attempt in 0..2 {
            let resp = self
                .http
                .get(url)
                .bearer_auth(&self.tokens.access_token)
                .send()
                .await
                .context("salesforce request")?;
            if resp.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.tokens = auth::refresh(&self.cfg, &self.secrets, &self.tokens).await?;
                continue;
            }
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return Err(anyhow!(
                    "salesforce {status}: {}",
                    body.chars().take(300).collect::<String>()
                ));
            }
            return serde_json::from_str(&body).context("salesforce json");
        }
        Err(anyhow!("unauthorized after refresh; please reconnect"))
    }

    pub async fn describe_global(&mut self) -> Result<Vec<SObjectMeta>> {
        let g: GlobalDescribe = self.get_json(&self.api("/sobjects/")).await?;
        Ok(g.sobjects.into_iter().filter(mirrorable).collect())
    }

    pub async fn describe_object(&mut self, object: &str) -> Result<Vec<FieldMeta>> {
        let d: ObjectDescribe = self
            .get_json(&self.api(&format!("/sobjects/{object}/describe")))
            .await?;
        Ok(d.fields.into_iter().filter(selectable).collect())
    }

    pub async fn count(&mut self, object: &str) -> Result<i64> {
        let q = format!("SELECT COUNT() FROM {object}");
        let url = format!("{}?q={}", self.api("/query"), urlencoded(&q));
        let p: QueryPage = self.get_json(&url).await?;
        Ok(p.total_size)
    }

    /// Follow nextRecordsUrl until done. `on_page` receives the running row count.
    pub async fn query_all(
        &mut self,
        soql: &str,
        on_page: &mut (dyn FnMut(usize) + Send),
    ) -> Result<Vec<Row>> {
        let mut url = format!("{}?q={}", self.api("/query"), urlencoded(soql));
        let mut out: Vec<Row> = Vec::new();
        loop {
            let page: QueryPage = self.get_json(&url).await?;
            out.extend(page.records);
            on_page(out.len());
            match page.next {
                Some(n) if !page.done => url = format!("{}{}", self.tokens.instance_url, n),
                _ => break,
            }
        }
        Ok(out)
    }
}

fn urlencoded(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(name: &str, queryable: bool, cs: bool, dep: bool) -> SObjectMeta {
        SObjectMeta {
            name: name.into(),
            label: name.into(),
            queryable,
            custom_setting: cs,
            deprecated_and_hidden: dep,
        }
    }

    #[test]
    fn mirrorable_requires_queryable_and_excludes_settings_and_deprecated() {
        assert!(mirrorable(&obj("Account", true, false, false)));
        assert!(!mirrorable(&obj("Setting__c", true, true, false)));
        assert!(!mirrorable(&obj("Old__c", true, false, true)));
        assert!(!mirrorable(&obj("Feed", false, false, false)));
    }

    #[test]
    fn selectable_skips_compound_and_binary_fields() {
        let f = |t: &str| FieldMeta {
            name: "x".into(),
            field_type: t.into(),
            label: "x".into(),
        };
        assert!(selectable(&f("string")));
        assert!(selectable(&f("textarea")));
        assert!(!selectable(&f("address")));
        assert!(!selectable(&f("location")));
        assert!(!selectable(&f("base64")));
    }

    #[test]
    fn describe_global_json_deserializes_with_defaults() {
        let g: GlobalDescribe = serde_json::from_str(
            r#"{"sobjects":[{"name":"Account","label":"Account","queryable":true}]}"#,
        )
        .unwrap();
        assert_eq!(g.sobjects.len(), 1);
        assert!(!g.sobjects[0].custom_setting);
        assert!(!g.sobjects[0].deprecated_and_hidden);
    }
}
