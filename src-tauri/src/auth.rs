//! Salesforce OAuth 2.0 authorization-code + PKCE for a public desktop client.
//! Tokens live only here and in the keychain — never in the webview.

use crate::config::Config;
use crate::secrets::{Secrets, TOKENS};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub const REDIRECT: &str = "http://localhost:1717/callback";
const LISTEN: &str = "127.0.0.1:1717";
const SCOPES: &str = "api refresh_token openid id profile email";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TokenSet {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub instance_url: String,
    /// Identity URL, e.g. https://login.salesforce.com/id/00D.../005...
    pub id: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Identity {
    pub user_id: String,
    pub organization_id: String,
    pub username: String,
    pub display_name: String,
}

#[derive(Debug)]
pub enum AuthError {
    StateMismatch,
    MissingCode,
    Provider(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::StateMismatch => write!(
                f,
                "login response did not match this app's request (state mismatch)"
            ),
            AuthError::MissingCode => write!(f, "login response had no authorization code"),
            AuthError::Provider(e) => write!(f, "Salesforce declined the login: {e}"),
        }
    }
}
impl std::error::Error for AuthError {}

// ── pure helpers (unit-tested) ──────────────────────────────────────────────

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn random_b64(n: usize) -> Result<String> {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).context("os random")?;
    Ok(URL_SAFE_NO_PAD.encode(b))
}

pub fn authorize_url(cfg: &Config, challenge: &str, state: &str) -> String {
    let mut u = url::Url::parse(&format!("{}/services/oauth2/authorize", cfg.login_url))
        .expect("static url");
    u.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &cfg.client_id)
        .append_pair("redirect_uri", REDIRECT)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("scope", SCOPES);
    u.to_string()
}

/// Parse the loopback request. Ok(None) means "not the callback path" (e.g. /favicon.ico).
pub fn parse_callback(
    path_and_query: &str,
    expected_state: &str,
) -> Result<Option<String>, AuthError> {
    let u = url::Url::parse(&format!("http://localhost{path_and_query}"))
        .map_err(|_| AuthError::MissingCode)?;
    if u.path() != "/callback" {
        return Ok(None);
    }
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for (k, v) in u.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            _ => {}
        }
    }
    if state.as_deref() != Some(expected_state) {
        return Err(AuthError::StateMismatch);
    }
    if let Some(e) = error {
        return Err(AuthError::Provider(e));
    }
    code.map(Some).ok_or(AuthError::MissingCode)
}

// ── token persistence ───────────────────────────────────────────────────────

pub fn load_tokens(secrets: &Secrets) -> Result<Option<TokenSet>> {
    Ok(match secrets.get(TOKENS)? {
        Some(json) => Some(serde_json::from_str(&json).context("stored tokens unreadable")?),
        None => None,
    })
}

pub fn save_tokens(secrets: &Secrets, t: &TokenSet) -> Result<()> {
    secrets.set(TOKENS, &serde_json::to_string(t)?)
}

// ── network flows ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    instance_url: String,
    id: String,
}

/// Full login: bind loopback FIRST, open browser, wait for one callback, exchange code.
pub async fn login(cfg: &Config, secrets: &Secrets) -> Result<(TokenSet, Identity)> {
    let verifier = random_b64(64)?;
    let challenge = pkce_challenge(&verifier);
    let state = random_b64(32)?;

    let server = tiny_http::Server::http(LISTEN).map_err(|e| {
        anyhow!("could not listen on {LISTEN} (is another copy of the app running?): {e}")
    })?;

    let auth_url = authorize_url(cfg, &challenge, &state);
    tauri_plugin_opener::open_url(&auth_url, None::<&str>).context("open browser")?;

    let code = tokio::task::spawn_blocking(move || wait_for_code(server, &state))
        .await
        .context("listener task")??;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/services/oauth2/token", cfg.login_url))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("client_id", cfg.client_id.as_str()),
            ("redirect_uri", REDIRECT),
            ("code_verifier", verifier.as_str()),
        ])
        .send()
        .await
        .context("token request")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("token exchange failed ({status}): {body}"));
    }
    let tr: TokenResponse = serde_json::from_str(&body).context("token response")?;
    let tokens = TokenSet {
        access_token: tr.access_token,
        refresh_token: tr.refresh_token,
        instance_url: tr.instance_url,
        id: tr.id,
    };
    let identity = fetch_identity(&tokens).await?;
    save_tokens(secrets, &tokens)?;
    Ok((tokens, identity))
}

fn wait_for_code(server: tiny_http::Server, state: &str) -> Result<String> {
    let deadline = std::time::Instant::now() + LOGIN_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(anyhow!(
                "timed out waiting for the browser login (5 minutes)"
            ));
        }
        let Some(req) = server.recv_timeout(remaining).context("listener")? else {
            return Err(anyhow!(
                "timed out waiting for the browser login (5 minutes)"
            ));
        };
        match parse_callback(req.url(), state) {
            Ok(None) => {
                let _ = req.respond(tiny_http::Response::empty(404));
            }
            Ok(Some(code)) => {
                let _ = req.respond(html(
                    200,
                    "Connected. You can close this tab and return to the app.",
                ));
                return Ok(code);
            }
            Err(e) => {
                let _ = req.respond(html(
                    400,
                    "Login was not accepted. Return to the app and try again.",
                ));
                return Err(e.into());
            }
        }
    }
}

fn html(code: u16, body: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let page = format!(
        "<!doctype html><meta charset=utf-8><title>Emanuel Customer Intelligence</title>\
         <body style=\"font:16px system-ui;padding:48px;color:#1c1917\">{body}</body>"
    );
    tiny_http::Response::from_string(page)
        .with_status_code(code)
        .with_header(
            tiny_http::Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap(),
        )
}

pub async fn refresh(cfg: &Config, secrets: &Secrets, current: &TokenSet) -> Result<TokenSet> {
    let rt = current
        .refresh_token
        .as_deref()
        .ok_or_else(|| anyhow!("no refresh token; please reconnect"))?;
    let resp = reqwest::Client::new()
        .post(format!("{}/services/oauth2/token", cfg.login_url))
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", rt),
            ("client_id", cfg.client_id.as_str()),
        ])
        .send()
        .await
        .context("refresh request")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!(
            "session refresh failed ({status}); please reconnect"
        ));
    }
    let tr: TokenResponse = serde_json::from_str(&body).context("refresh response")?;
    let tokens = TokenSet {
        access_token: tr.access_token,
        refresh_token: tr.refresh_token.or_else(|| current.refresh_token.clone()),
        instance_url: tr.instance_url,
        id: tr.id,
    };
    save_tokens(secrets, &tokens)?;
    Ok(tokens)
}

/// Best-effort revoke at Salesforce; the caller clears the keychain regardless.
pub async fn revoke(cfg: &Config, tokens: &TokenSet) {
    let token = tokens
        .refresh_token
        .clone()
        .unwrap_or_else(|| tokens.access_token.clone());
    let _ = reqwest::Client::new()
        .post(format!("{}/services/oauth2/revoke", cfg.login_url))
        .form(&[("token", token.as_str())])
        .send()
        .await;
}

pub async fn fetch_identity(tokens: &TokenSet) -> Result<Identity> {
    let resp = reqwest::Client::new()
        .get(&tokens.id)
        .bearer_auth(&tokens.access_token)
        .send()
        .await
        .context("identity request")?
        .error_for_status()
        .context("identity response")?;
    resp.json::<Identity>().await.context("identity json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_appendix_b_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            pkce_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn authorize_url_contains_required_params() {
        let cfg = crate::config::Config::new("CID", "https://x.my.salesforce.com");
        let u = authorize_url(&cfg, "CHAL", "STATE");
        assert!(u.starts_with("https://x.my.salesforce.com/services/oauth2/authorize?"));
        for needle in [
            "response_type=code",
            "client_id=CID",
            "code_challenge=CHAL",
            "code_challenge_method=S256",
            "state=STATE",
            "redirect_uri=http%3A%2F%2Flocalhost%3A1717%2Fcallback",
            "scope=api+refresh_token+openid+id+profile+email",
        ] {
            assert!(u.contains(needle), "missing {needle} in {u}");
        }
    }

    #[test]
    fn parse_callback_returns_code_when_state_matches() {
        let r = parse_callback("/callback?code=abc.def&state=S1", "S1").unwrap();
        assert_eq!(r.as_deref(), Some("abc.def"));
    }

    #[test]
    fn parse_callback_rejects_wrong_or_missing_state() {
        assert!(matches!(
            parse_callback("/callback?code=x&state=BAD", "S1"),
            Err(AuthError::StateMismatch)
        ));
        assert!(matches!(
            parse_callback("/callback?code=x", "S1"),
            Err(AuthError::StateMismatch)
        ));
    }

    #[test]
    fn parse_callback_reports_provider_error_and_missing_code() {
        assert!(matches!(
            parse_callback("/callback?error=access_denied&state=S1", "S1"),
            Err(AuthError::Provider(_))
        ));
        assert!(matches!(
            parse_callback("/callback?state=S1", "S1"),
            Err(AuthError::MissingCode)
        ));
    }

    #[test]
    fn parse_callback_ignores_other_paths() {
        assert_eq!(parse_callback("/favicon.ico", "S1").unwrap(), None);
    }
}
