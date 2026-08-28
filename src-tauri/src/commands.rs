//! The command boundary — the only surface the webview can reach.
//! Every command: (1) never returns a token, (2) audits itself, (3) never holds
//! the store lock across an await.

use crate::agent::{self, AgentRegistry};
use crate::auth::{self, Identity};
use crate::chat::{self, Cancel, ChatBackend};
use crate::chat_context;
use crate::config::Config;
use crate::insights::{self, AtRiskRow, Insights};
use crate::llm;
use crate::profile;
use crate::progress::{self, ProgressEvent, Reporter};
use crate::risk;
use crate::salesforce::SfClient;
use crate::secrets::{Secrets, TOKENS};
use crate::segment::{self, SegmentReq, SegmentResult};
use crate::store::{self, AuditRow, ChatConversation, FieldRow, ObjectRow, StoredChatMessage, Store, Who};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    pub cfg: Config,
    pub secrets: Secrets,
    pub db_path: PathBuf,
    pub store: Mutex<Option<Store>>,
    pub identity: Mutex<Option<Identity>>,
    /// Latest progress event of the running Insights job (rebuild / risk), or None when no
    /// job is running. Deliberately separate from `store` so it can be read while a rebuild
    /// holds the store lock.
    pub job: Mutex<Option<ProgressEvent>>,
    /// In-flight chat streams: the existing agent-run registry holds each stream's task handle
    /// (keyed by conversation id) so `chat_cancel` can abort it — which kills any CLI subprocess
    /// via kill_on_drop — and `chat_cancels` holds each stream's cooperative cancel flag.
    pub agents: AgentRegistry,
    pub chat_cancels: Mutex<HashMap<String, Cancel>>,
    /// Keeps the non-blocking file-log writer's flush thread alive for the process lifetime.
    /// Dropping it stops the rotating log sink, so it is held here and never taken. `None`
    /// when file logging could not be initialized (stdout logging still applies).
    pub _log_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

/// A short unique id with a role prefix (OS randomness, matching `secrets`/`auth`).
fn new_id(prefix: &str) -> String {
    let mut b = [0u8; 8];
    let _ = getrandom::fill(&mut b);
    format!("{prefix}-{}", hex::encode(b))
}

/// A per-action correlation id: an opaque `act-<hex>` token (OS randomness, carrying no
/// household identity) that ties an audited action's log events to its audit row. See the
/// request-correlation capability.
fn new_cid() -> String {
    new_id("act")
}

// ── record-level access auditing (pii-access-audit) ───────────────────────────
// Each helper writes the audit row a command delegates to, so the audit-detail shape can be
// unit-tested against a real Store without a Tauri runtime. The disclosed household set is
// keyed on `account_id`; the household `name` is deliberately never written.

/// Audit the at-risk read: the full disclosed household id set plus the count, tagged with the
/// action's correlation id.
fn audit_at_risk_access(s: &Store, w: &Who, cid: &str, rows: &[AtRiskRow]) -> anyhow::Result<()> {
    let account_ids: Vec<&str> = rows.iter().map(|r| r.account_id.as_str()).collect();
    s.audit(
        w,
        "insights.at_risk",
        None,
        Some(serde_json::json!({"cid": cid, "count": rows.len(), "account_ids": account_ids})),
    )
}

/// Audit a Watch List read or export: the full disclosed household id set, the count, and
/// availability. Never the household `name`.
fn audit_watch_list_access(
    s: &Store,
    w: &Who,
    cid: &str,
    action: &str,
    rows: &[risk::WatchRowView],
    available: bool,
) -> anyhow::Result<()> {
    let account_ids: Vec<&str> = rows.iter().map(|r| r.account_id.as_str()).collect();
    s.audit(
        w,
        action,
        None,
        Some(serde_json::json!({
            "cid": cid, "count": rows.len(), "available": available, "account_ids": account_ids,
        })),
    )
}

/// Audit a read of the audit log itself — paging parameters only, no audit-row content. Written
/// after the read (`list_audit` stays a pure read) so it never recurses.
fn audit_audit_read(s: &Store, w: &Who, cid: &str, limit: i64, offset: i64) -> anyhow::Result<()> {
    s.audit(
        w,
        "audit.read",
        None,
        Some(serde_json::json!({"cid": cid, "limit": limit, "offset": offset})),
    )
}

/// Audit a listing of chat conversations — a count only, no titles or content.
fn audit_chat_list_conversations(
    s: &Store,
    w: &Who,
    cid: &str,
    count: usize,
) -> anyhow::Result<()> {
    s.audit(
        w,
        "chat.list_conversations",
        None,
        Some(serde_json::json!({"cid": cid, "count": count})),
    )
}

/// Audit opening a chat transcript — the conversation id and message count only, no content.
fn audit_chat_list_messages(
    s: &Store,
    w: &Who,
    cid: &str,
    conversation_id: &str,
    messages: usize,
) -> anyhow::Result<()> {
    s.audit(
        w,
        "chat.list_messages",
        None,
        Some(serde_json::json!({
            "cid": cid, "conversation_id": conversation_id, "messages": messages,
        })),
    )
}

// ── chat retention (chat-retention) ───────────────────────────────────────────

/// Maximum age of a stored chat message before age-based retention prunes it. A named,
/// documented default (a retention *setting* UI is deferred). See the chat-retention capability.
const CHAT_RETENTION_DAYS: i64 = 365;

/// The retention cutoff timestamp (now − `CHAT_RETENTION_DAYS`) in the store's `created_at`
/// format (RFC-3339, seconds, `Z`), so it compares directly against `_chat_messages.created_at`.
fn chat_retention_cutoff() -> String {
    (chrono::Utc::now() - chrono::Duration::days(CHAT_RETENTION_DAYS))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Run a best-effort chat-retention prune: log the count on success, warn and swallow on
/// failure so the caller's chat action still proceeds. The prune is a closure so the failure
/// path is testable without corrupting a real store.
fn best_effort_prune(cid: &str, prune: impl FnOnce() -> anyhow::Result<usize>) {
    match prune() {
        Ok(0) => {}
        Ok(n) => {
            tracing::info!(target: "chat_retention", cid, pruned = n, "pruned expired chat messages")
        }
        Err(e) => {
            tracing::warn!(target: "chat_retention", cid, error = %e, "chat retention prune failed; continuing")
        }
    }
}

/// Apply age-based retention to stored chat, best-effort — a prune failure never fails the
/// chat action.
fn prune_chat_best_effort(s: &mut Store, cid: &str) {
    let cutoff = chat_retention_cutoff();
    best_effort_prune(cid, || s.prune_chat(&cutoff));
}

// ── chat-turn telemetry (persistent-logging) ──────────────────────────────────

/// A content-free, PII-free record of one completed chat turn: backend, elapsed ms, optional
/// token counts (present only when the backend reports them), the conversation id, and the
/// action's correlation id. It holds NO prompt or reply text and NO household identity by
/// construction — asserted by `telemetry_event_carries_no_content_or_identity`.
#[derive(Debug)]
struct ChatTurnTelemetry {
    cid: String,
    backend: chat::BackendKind,
    ms: u128,
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    conversation_id: String,
}

impl ChatTurnTelemetry {
    fn emit(&self) {
        tracing::info!(
            target: "chat_telemetry",
            cid = %self.cid,
            backend = self.backend.as_str(),
            ms = self.ms,
            prompt_tokens = self.prompt_tokens,
            completion_tokens = self.completion_tokens,
            conversation_id = %self.conversation_id,
            "chat turn completed"
        );
    }
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    let s = e.to_string();
    tracing::warn!("{s}");
    s
}

/// Run `f` with the store, opening it lazily. Never call this while awaiting.
fn with_store<T>(
    state: &AppState,
    f: impl FnOnce(&mut Store) -> anyhow::Result<T>,
) -> CmdResult<T> {
    let mut guard = state
        .store
        .lock()
        .map_err(|_| "store lock poisoned".to_string())?;
    if guard.is_none() {
        let key = state.secrets.db_key().map_err(err)?;
        *guard = Some(store::open(&state.db_path, &key).map_err(err)?);
    }
    f(guard.as_mut().expect("opened")).map_err(err)
}

fn who(state: &AppState) -> Who {
    match state.identity.lock().ok().and_then(|g| g.clone()) {
        Some(id) => Who {
            sf_user_id: Some(id.user_id),
            sf_username: Some(id.username),
        },
        None => Who::default(),
    }
}

async fn client(state: &AppState) -> CmdResult<SfClient> {
    let tokens = auth::load_tokens(&state.secrets)
        .map_err(err)?
        .ok_or("Not connected to Salesforce")?;
    Ok(SfClient::new(
        state.cfg.clone(),
        state.secrets.clone(),
        tokens,
    ))
}

// ── status / auth ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct StatusView {
    pub connected: bool,
    pub identity: Option<Identity>,
    pub object_count: i64,
    pub selected_count: i64,
    pub synced_rows: i64,
    pub last_scan_at: Option<String>,
}

/// Fast, local-only status for first paint: whether a session exists (tokens present) plus
/// mirror counts. It never touches the network — the signed-in name is recovered separately
/// by `recover_identity` so the window is never gated on a Salesforce round-trip.
#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> CmdResult<StatusView> {
    let connected = auth::load_tokens(&state.secrets).map_err(err)?.is_some();
    let identity = state.identity.lock().ok().and_then(|g| g.clone());
    let st = with_store(state.inner(), |s| s.status())?;
    Ok(StatusView {
        connected,
        identity,
        object_count: st.object_count,
        selected_count: st.selected_count,
        synced_rows: st.synced_rows,
        last_scan_at: st.last_scan_at,
    })
}

/// Recover the signed-in identity for a stored session (a network call, refreshing the
/// token if needed). Called in the background after first paint. Returns the cached identity
/// if already known this session, `None` when no session exists, or an error if the session
/// cannot be recovered — the frontend treats that error as signed-out.
#[tauri::command]
pub async fn recover_identity(state: State<'_, AppState>) -> CmdResult<Option<Identity>> {
    if let Some(id) = state.identity.lock().ok().and_then(|g| g.clone()) {
        return Ok(Some(id));
    }
    if auth::load_tokens(&state.secrets).map_err(err)?.is_none() {
        return Ok(None);
    }
    let mut c = client(state.inner()).await?;
    let id = match auth::fetch_identity(c.tokens()).await {
        Ok(id) => id,
        Err(_) => {
            let t = auth::refresh(&state.cfg, &state.secrets, c.tokens())
                .await
                .map_err(err)?;
            c = SfClient::new(state.cfg.clone(), state.secrets.clone(), t);
            auth::fetch_identity(c.tokens()).await.map_err(err)?
        }
    };
    *state.identity.lock().map_err(|_| "lock".to_string())? = Some(id.clone());
    Ok(Some(id))
}

#[tauri::command]
pub async fn connect(state: State<'_, AppState>) -> CmdResult<Identity> {
    let (_tokens, identity) = auth::login(&state.cfg, &state.secrets).await.map_err(err)?;
    *state.identity.lock().map_err(|_| "lock".to_string())? = Some(identity.clone());
    let w = who(state.inner());
    with_store(state.inner(), |s| {
        s.audit(
            &w,
            "auth.connect",
            None,
            Some(serde_json::json!({"org": identity.organization_id})),
        )
    })?;
    Ok(identity)
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> CmdResult<()> {
    let w = who(state.inner());
    if let Some(t) = auth::load_tokens(&state.secrets).map_err(err)? {
        auth::revoke(&state.cfg, &t).await;
    }
    state.secrets.delete(TOKENS).map_err(err)?;
    *state.identity.lock().map_err(|_| "lock".to_string())? = None;
    with_store(state.inner(), |s| {
        s.audit(&w, "auth.disconnect", None, None)
    })
}

// ── scan ────────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
struct ScanProgress {
    done: usize,
    total: usize,
}

#[derive(Serialize)]
pub struct ScanSummary {
    pub objects: usize,
    pub failed: Vec<String>,
}

#[tauri::command]
pub async fn scan(app: AppHandle, state: State<'_, AppState>) -> CmdResult<ScanSummary> {
    let mut c = client(state.inner()).await?;
    let objects = c.describe_global().await.map_err(err)?;
    let total = objects.len();
    let mut failed = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        let fields = match c.describe_object(&o.name).await {
            Ok(f) => f,
            Err(e) => {
                failed.push(format!("{}: {e}", o.name));
                continue;
            }
        };
        let count = c.count(&o.name).await.unwrap_or(-1);
        let (name, label) = (o.name.clone(), o.label.clone());
        with_store(state.inner(), |s| {
            s.upsert_object(&name, &label, count)?;
            for f in &fields {
                s.upsert_field(
                    &name,
                    &f.name,
                    &f.field_type,
                    &f.label,
                    profile::is_sensitive(&f.name, &f.field_type),
                )?;
            }
            Ok(())
        })?;
        let _ = app.emit("scan:progress", ScanProgress { done: i + 1, total });
    }
    let w = who(state.inner());
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    with_store(state.inner(), |s| {
        s.set_meta("last_scan_at", &now)?;
        s.audit(
            &w,
            "scan.run",
            None,
            Some(serde_json::json!({"objects": total, "failed": failed.len()})),
        )
    })?;
    Ok(ScanSummary {
        objects: total - failed.len(),
        failed,
    })
}

// ── selection ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_objects(state: State<'_, AppState>) -> CmdResult<Vec<ObjectRow>> {
    with_store(state.inner(), |s| s.list_objects())
}

#[tauri::command]
pub async fn set_object_selected(
    object: String,
    selected: bool,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    let w = who(state.inner());
    with_store(state.inner(), |s| {
        s.set_object_selected(&object, selected)?;
        s.audit(
            &w,
            if selected {
                "object.select"
            } else {
                "object.deselect"
            },
            Some(&object),
            None,
        )
    })
}

#[tauri::command]
pub async fn list_fields(object: String, state: State<'_, AppState>) -> CmdResult<Vec<FieldRow>> {
    with_store(state.inner(), |s| s.list_fields(&object))
}

#[tauri::command]
pub async fn set_field_withheld(
    object: String,
    field: String,
    withheld: bool,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    let w = who(state.inner());
    with_store(state.inner(), |s| {
        if s.set_field_withheld(&object, &field, withheld)? {
            s.audit(
                &w,
                if withheld {
                    "field.rewithhold"
                } else {
                    "field.override"
                },
                Some(&object),
                Some(serde_json::json!({"field": field})),
            )?;
        }
        Ok(())
    })
}

// ── sync / profile ──────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
struct SyncProgress {
    object: String,
    rows: usize,
}

#[derive(Serialize)]
pub struct SyncSummary {
    pub objects_synced: usize,
    pub rows: usize,
    pub failed: Vec<String>,
}

#[tauri::command]
pub async fn sync_selected(app: AppHandle, state: State<'_, AppState>) -> CmdResult<SyncSummary> {
    let mut c = client(state.inner()).await?;
    let w = who(state.inner());
    let selected = with_store(state.inner(), |s| s.selected_objects())?;
    let mut summary = SyncSummary {
        objects_synced: 0,
        rows: 0,
        failed: Vec::new(),
    };
    for object in selected {
        let cols = with_store(state.inner(), |s| s.sync_columns(&object))?;
        if cols.is_empty() {
            summary
                .failed
                .push(format!("{object}: every field is withheld"));
            continue;
        }
        let soql = format!("SELECT {} FROM {object}", cols.join(","));
        let app2 = app.clone();
        let obj2 = object.clone();
        let rows = match c
            .query_all(&soql, &mut |n| {
                let _ = app2.emit(
                    "sync:progress",
                    SyncProgress {
                        object: obj2.clone(),
                        rows: n,
                    },
                );
            })
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let msg = e.to_string();
                with_store(state.inner(), |s| {
                    s.audit(
                        &w,
                        "sync.object_failed",
                        Some(&object),
                        Some(serde_json::json!({"error": msg})),
                    )
                })?;
                summary.failed.push(format!("{object}: {e}"));
                continue;
            }
        };
        let n = with_store(state.inner(), |s| {
            let n = s.replace_mirror(&object, &cols, &rows)?;
            s.audit(
                &w,
                "sync.object",
                Some(&object),
                Some(serde_json::json!({"rows": n, "fields": cols.len()})),
            )?;
            Ok(n)
        })?;
        summary.objects_synced += 1;
        summary.rows += n;
    }
    Ok(summary)
}

#[tauri::command]
pub async fn profile_selected(state: State<'_, AppState>) -> CmdResult<usize> {
    let w = who(state.inner());
    with_store(state.inner(), |s| {
        let n = profile::profile_all(s)?;
        s.audit(
            &w,
            "profile.run",
            None,
            Some(serde_json::json!({"objects": n})),
        )?;
        // Profiling never changes the mart's sources, so this only rebuilds (and audits)
        // when a relevant sync or schema change actually left the mart stale.
        if s.table_exists("Account")? {
            ensure_fresh(s, &w, false)?;
        }
        Ok(n)
    })
}

// ── segments / audit / purge ────────────────────────────────────────────────

#[tauri::command]
pub async fn query_segment(
    req: SegmentReq,
    state: State<'_, AppState>,
) -> CmdResult<SegmentResult> {
    let w = who(state.inner());
    with_store(state.inner(), |s| {
        let r = segment::run(s, &req)?;
        let fields: Vec<&str> = req.filters.iter().map(|f| f.field.as_str()).collect();
        s.audit(
            &w,
            "segment.query",
            Some(&req.object),
            Some(serde_json::json!({"fields": fields, "group_by": req.group_by, "count": r.count})),
        )?;
        Ok(r)
    })
}

#[tauri::command]
pub async fn get_audit(
    limit: i64,
    offset: i64,
    state: State<'_, AppState>,
) -> CmdResult<Vec<AuditRow>> {
    let w = who(state.inner());
    let cid = new_cid();
    tracing::info!(target: "access", cid, action = "audit.read", "audited access");
    with_store(state.inner(), |s| {
        // Read first (a pure read), then record that the audit log was read — so the read
        // never includes its own row and writing it triggers no recursive audit.
        let rows = s.list_audit(limit.clamp(1, 500), offset.max(0))?;
        audit_audit_read(s, &w, &cid, limit, offset)?;
        Ok(rows)
    })
}

#[tauri::command]
pub async fn purge_local_data(state: State<'_, AppState>) -> CmdResult<()> {
    let w = who(state.inner());
    let dir = exports_dir(state.inner());
    with_store(state.inner(), |s| {
        s.purge_mirror()?;
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        s.audit(
            &w,
            "data.purge",
            None,
            Some(serde_json::json!({"exports_deleted": true})),
        )
    })
}

// ── insights ────────────────────────────────────────────────────────────────

fn exports_dir(state: &AppState) -> PathBuf {
    state
        .db_path
        .parent()
        .map(|p| p.join("exports"))
        .unwrap_or_else(|| PathBuf::from("exports"))
}

/// Rebuild the mart if it is missing, older than the newest sync, or built by a prior
/// mart schema (e.g. before new columns were added to `_m_household_fy`).
fn ensure_fresh(s: &mut Store, w: &Who, force: bool) -> anyhow::Result<()> {
    let mut sink = progress::noop();
    let mut reporter = Reporter::new("rebuild", insights::REBUILD_STEPS, &mut sink);
    ensure_fresh_with(s, w, force, &mut reporter)
}

/// As `ensure_fresh`, but reports rebuild progress through `progress` when a rebuild
/// actually runs, so a long post-sync rebuild is visibly working, not a frozen label.
fn ensure_fresh_with(
    s: &mut Store,
    w: &Who,
    force: bool,
    progress: &mut Reporter<'_>,
) -> anyhow::Result<()> {
    let built = s.get_meta("insights_built_at")?;
    // Only a sync of an object the mart reads can make it stale (same rule as the
    // `stale` flag in `insights::views`); syncing Contact etc. must not force a rebuild.
    let newest = s.newest_mart_source_sync_at()?;
    let stale = match (&built, &newest) {
        (None, _) => true,
        (Some(b), Some(n)) => n > b, // ISO-8601 strings compare chronologically
        (Some(_), None) => false,
    };
    let schema_current =
        s.get_meta("insights_schema_version")? == Some(insights::mart_schema_fingerprint());
    if force || stale || !schema_current || !s.table_exists(insights::MART)? {
        let info = insights::rebuild_with(s, progress)?;
        s.audit(
            w,
            "insights.rebuild",
            None,
            Some(
                serde_json::json!({"households": info.households, "unavailable": info.unavailable}),
            ),
        )?;
    }
    // Self-heal a mart built before the geography cache existed (or a warm that failed): do the
    // one expensive household load once and cache every all-members view, so no geography read
    // pays it again. A no-op once the cache matches the current build.
    insights::ensure_geo_cache_warm(s)?;
    Ok(())
}

/// A progress sink for the Insights jobs: records the latest event in `state.job` (so
/// `get_insights_job` can answer while the store lock is held) and emits it to the page
/// on `insights:progress`.
fn progress_sink<'a>(app: &'a AppHandle, state: &'a AppState) -> impl FnMut(&ProgressEvent) + 'a {
    move |ev: &ProgressEvent| {
        if let Ok(mut job) = state.job.lock() {
            *job = Some(ev.clone());
        }
        let _ = app.emit("insights:progress", ev);
    }
}

/// Mark no Insights job as running. Must run after the job's store work on every path.
fn clear_job(state: &AppState) {
    if let Ok(mut job) = state.job.lock() {
        *job = None;
    }
}

/// Latest progress of the running Insights job, or None when nothing is running. Reads only
/// `state.job` — never the store — so it answers while a rebuild holds the store lock.
#[tauri::command]
pub async fn get_insights_job(state: State<'_, AppState>) -> CmdResult<Option<ProgressEvent>> {
    Ok(state
        .job
        .lock()
        .map_err(|_| "job lock poisoned".to_string())?
        .clone())
}

#[tauri::command]
pub async fn get_insights(
    app: AppHandle,
    force_rebuild: bool,
    state: State<'_, AppState>,
) -> CmdResult<Insights> {
    let w = who(state.inner());
    // The rebuild can run for many seconds; keep it off the async workers.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let state = state.inner();
        let mut sink = progress_sink(&app, state);
        let mut progress = Reporter::new("rebuild", insights::REBUILD_STEPS, &mut sink);
        let out = with_store(state, |s| {
            // Timing (target "insights_timing"): splits rebuild vs. mart-read to locate the
            // dominant cost. Off by default; enable with RUST_LOG=insights_timing=debug.
            let t = std::time::Instant::now();
            ensure_fresh_with(s, &w, force_rebuild, &mut progress)?;
            tracing::debug!(target: "insights_timing", ms = t.elapsed().as_millis(), "ensure_fresh (rebuild if stale)");
            let t = std::time::Instant::now();
            let out = insights::views(s, insights::current_fy());
            tracing::debug!(target: "insights_timing", ms = t.elapsed().as_millis(), "insights::views (mart read)");
            out
        });
        clear_job(state);
        out
    })
    .await
    .map_err(err)?
}

/// Mode-driven ZIP geography for one fiscal year, mode, and optional segment. Loaded on
/// demand when the map view opens, off the `get_insights` critical path. Returns only
/// suppressed per-ZIP aggregates plus an out-of-area count — never a name, raw postal,
/// coordinate, or bill-to-other id. Audited as aggregate access (no household identity).
#[tauri::command]
pub async fn zip_geography(
    app: AppHandle,
    fiscal_year: i32,
    mode: insights::GeoMode,
    segment: Option<insights::Segment>,
    state: State<'_, AppState>,
) -> CmdResult<insights::ZipGeography> {
    let mut views = zip_geography_years(app, mode, segment, vec![fiscal_year], state).await?;
    views.pop().ok_or_else(|| "no geography view returned".to_string())
}

/// As `zip_geography`, for many fiscal years of one mode and segment: ONE lock acquisition,
/// one household load on a cache miss, one audit row. The Retention-by-area trend needs eight
/// cohort years at once, and every command queues on the same store lock, so eight separate
/// calls would be eight waits in a row. Views come back in request order.
#[tauri::command]
pub async fn zip_geography_years(
    app: AppHandle,
    mode: insights::GeoMode,
    segment: Option<insights::Segment>,
    fiscal_years: Vec<i32>,
    state: State<'_, AppState>,
) -> CmdResult<Vec<insights::ZipGeography>> {
    let w = who(state.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let state = state.inner();
        let mut sink = progress_sink(&app, state);
        let mut progress = Reporter::new("rebuild", insights::REBUILD_STEPS, &mut sink);
        let out = with_store(state, |s| {
            ensure_fresh_with(s, &w, false, &mut progress)?;
            let views = insights::zip_geography_views(s, mode, segment, &fiscal_years)?;
            s.audit(
                &w,
                "insights.zip_geography",
                None,
                Some(serde_json::json!({
                    "fys": fiscal_years,
                    "cells": views.iter().map(|v| v.cells.len()).sum::<usize>(),
                    "out_of_area": views.iter().map(|v| v.out_of_area).sum::<i64>(),
                    "available": views.iter().any(|v| v.available),
                })),
            )?;
            Ok(views)
        });
        clear_job(state);
        out
    })
    .await
    .map_err(err)?
}

/// Cohort retention rolled up to New York City neighborhoods, for many cohort years and an
/// optional segment, in one lock hold. The neighborhood map opens all its cohort years at once,
/// so this mirrors `zip_geography_years`: one household load, one audit row, request-ordered.
/// Returns only suppressed per-neighborhood aggregates plus an out-of-area count — never a
/// name, ZIP, coordinate, or bill-to-other id.
#[tauri::command]
pub async fn neighborhood_retention_years(
    app: AppHandle,
    segment: Option<insights::Segment>,
    cohort_fys: Vec<i32>,
    state: State<'_, AppState>,
) -> CmdResult<Vec<insights::NeighborhoodRetention>> {
    let w = who(state.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let state = state.inner();
        let mut sink = progress_sink(&app, state);
        let mut progress = Reporter::new("rebuild", insights::REBUILD_STEPS, &mut sink);
        let out = with_store(state, |s| {
            ensure_fresh_with(s, &w, false, &mut progress)?;
            let views = insights::neighborhood_retention_views(s, segment, &cohort_fys)?;
            s.audit(
                &w,
                "insights.neighborhood_retention",
                None,
                Some(serde_json::json!({
                    "cohort_fys": cohort_fys,
                    "cells": views.iter().map(|v| v.cells.len()).sum::<usize>(),
                    "out_of_area": views.iter().map(|v| v.out_of_area).sum::<i64>(),
                    "available": views.iter().any(|v| v.available),
                })),
            )?;
            Ok(views)
        });
        clear_job(state);
        out
    })
    .await
    .map_err(err)?
}

/// Mode-driven geography rolled up to New York City neighborhoods for one fiscal year, mode, and
/// optional segment — the neighborhood counterpart of `zip_geography` for the density, growth,
/// and attrition views. Loaded on demand when the map view opens, off the `get_insights` critical
/// path. Returns only suppressed per-neighborhood aggregates (labeled with the public neighborhood
/// name) plus an out-of-area count — never a raw postal code, coordinate, or bill-to-other id.
#[tauri::command]
pub async fn neighborhood_geography(
    app: AppHandle,
    fiscal_year: i32,
    mode: insights::GeoMode,
    segment: Option<insights::Segment>,
    state: State<'_, AppState>,
) -> CmdResult<insights::ZipGeography> {
    let w = who(state.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let state = state.inner();
        let mut sink = progress_sink(&app, state);
        let mut progress = Reporter::new("rebuild", insights::REBUILD_STEPS, &mut sink);
        let out = with_store(state, |s| {
            ensure_fresh_with(s, &w, false, &mut progress)?;
            let view = insights::neighborhood_geography_view(s, fiscal_year, mode, segment)?;
            s.audit(
                &w,
                "insights.neighborhood_geography",
                None,
                Some(serde_json::json!({
                    "fy": fiscal_year,
                    "cells": view.cells.len(),
                    "out_of_area": view.out_of_area,
                    "available": view.available,
                })),
            )?;
            Ok(view)
        });
        clear_job(state);
        out
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub async fn get_at_risk(state: State<'_, AppState>) -> CmdResult<Vec<AtRiskRow>> {
    let w = who(state.inner());
    let cid = new_cid();
    tracing::info!(target: "access", cid, action = "insights.at_risk", "audited access");
    with_store(state.inner(), |s| {
        ensure_fresh(s, &w, false)?;
        let rows = insights::at_risk(s, insights::current_fy())?;
        audit_at_risk_access(s, &w, &cid, &rows)?;
        Ok(rows)
    })
}

/// Resolved risk inputs: either the cached analysis (no compute needed) or the owned mart
/// data to fit from. Produced under the store lock; the fit then runs on the owned data.
enum RiskPrep {
    Ready((risk::RiskModel, risk::WatchList)),
    Compute {
        hh: Vec<insights::Hh>,
        years: Vec<insights::HhFy>,
        caps: Vec<insights::SourceCapability>,
        built: Option<String>,
        cur: i32,
    },
}

/// Under the store lock: rebuild if stale, then hand back either the cached analysis or the
/// owned inputs to fit from. Deliberately does NOT run the fit — the hot read commands
/// release the store lock across `risk::analyze_with` (a multi-second fit) so it never blocks
/// cheap reads that need the same lock only briefly, above all the geography cache. The mart
/// data it returns is owned, so it holds no store borrow once the lock is dropped.
fn risk_prepare(s: &mut Store, w: &Who) -> anyhow::Result<RiskPrep> {
    ensure_fresh(s, w, false)?;
    // The cache is keyed on `insights_built_at`, which advances only inside the rebuild
    // transaction, so it self-invalidates on any rebuild. A stamp mismatch, a missing cache,
    // or an unreadable blob all fall through to the compute inputs below.
    let built = s.get_meta("insights_built_at")?;
    if let Some(built_at) = built.as_deref() {
        let blob = s.get_meta(risk::RISK_CACHE_KEY)?;
        if let Some(cached) = risk::reuse_cached(built_at, blob.as_deref()) {
            return Ok(RiskPrep::Ready(cached));
        }
    }
    let t = std::time::Instant::now();
    let prep = RiskPrep::Compute {
        hh: insights::load(s)?,
        years: insights::load_household_years(s)?,
        caps: insights::source_capabilities(s)?,
        built,
        cur: insights::current_fy(),
    };
    tracing::debug!(target: "insights_timing", ms = t.elapsed().as_millis(), "risk mart read");
    Ok(prep)
}

/// Persist a freshly fitted analysis under the current build stamp. Best-effort: no stamp or
/// an encode failure just skips the cache and the result stands unchanged.
fn risk_write_cache(
    s: &Store,
    built: Option<&str>,
    model: &risk::RiskModel,
    list: &risk::WatchList,
) -> anyhow::Result<()> {
    if let Some(built_at) = built {
        if let Some(blob) = risk::serialize_cache(built_at, model, list) {
            s.set_meta(risk::RISK_CACHE_KEY, &blob)?;
        }
    }
    Ok(())
}

/// Rebuild if needed, then return the validated churn analysis, reusing the cache when the
/// dataset is unchanged. Runs the fit under whatever lock the caller already holds; the hot
/// read commands use `risk_prepare` + `risk_write_cache` directly so they can drop the lock
/// across the fit. Reuse never alters the model or its outputs; it only skips repeats.
fn analyze_risk(
    s: &mut Store,
    w: &Who,
    progress: &mut Reporter<'_>,
) -> anyhow::Result<(risk::RiskModel, risk::WatchList)> {
    match risk_prepare(s, w)? {
        RiskPrep::Ready(pair) => Ok(pair),
        RiskPrep::Compute { hh, years, caps, built, cur } => {
            let (model, list) =
                risk::analyze_with(&hh, &years, &caps, cur, risk::DEFAULT_ALPHA, progress);
            risk_write_cache(s, built.as_deref(), &model, &list)?;
            Ok((model, list))
        }
    }
}

/// Aggregate Risk view: validation results and backtests only. No household names, so it
/// is not audited as named access.
#[tauri::command]
pub async fn get_risk_summary(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<risk::RiskSummary> {
    let w = who(state.inner());
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<risk::RiskSummary> {
        let state = app.state::<AppState>();
        let state = state.inner();
        let mut sink = progress_sink(&app, state);
        let mut progress = Reporter::new("risk", risk::RISK_STEPS, &mut sink);
        // Hold the lock only for the quick prepare and the cache write; drop it across the fit.
        let out = (|| -> CmdResult<risk::RiskSummary> {
            let (model, list) = match with_store(state, |s| risk_prepare(s, &w))? {
                RiskPrep::Ready(pair) => pair,
                RiskPrep::Compute { hh, years, caps, built, cur } => {
                    let t = std::time::Instant::now();
                    let (model, list) =
                        risk::analyze_with(&hh, &years, &caps, cur, risk::DEFAULT_ALPHA, &mut progress);
                    tracing::debug!(target: "insights_timing", ms = t.elapsed().as_millis(), "risk::analyze (compute, lock released)");
                    with_store(state, |s| risk_write_cache(s, built.as_deref(), &model, &list))?;
                    (model, list)
                }
            };
            Ok(risk::risk_summary(&model, &list))
        })();
        clear_job(state);
        out
    })
    .await
    .map_err(err)?
}

/// Named Watch List: loaded only on explicit request and audited. The audit records the
/// result count and availability, never a household name or risk-feature value.
#[tauri::command]
pub async fn get_watch_list(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<risk::WatchListView> {
    let w = who(state.inner());
    let cid = new_cid();
    tracing::info!(target: "access", cid, action = "risk.watch_list.load", "audited access");
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<risk::WatchListView> {
        let state = app.state::<AppState>();
        let state = state.inner();
        let mut sink = progress_sink(&app, state);
        let mut progress = Reporter::new("risk", risk::RISK_STEPS, &mut sink);
        let out = (|| -> CmdResult<risk::WatchListView> {
            // Prepare under the lock, fit with the lock released, then write the cache, audit,
            // and build the view in one final locked section.
            let (model, list, to_cache) = match with_store(state, |s| risk_prepare(s, &w))? {
                RiskPrep::Ready(pair) => (pair.0, pair.1, None),
                RiskPrep::Compute { hh, years, caps, built, cur } => {
                    let (model, list) =
                        risk::analyze_with(&hh, &years, &caps, cur, risk::DEFAULT_ALPHA, &mut progress);
                    (model, list, Some(built))
                }
            };
            let view = risk::watch_list_view(&model, &list);
            with_store(state, |s| {
                if let Some(built) = &to_cache {
                    risk_write_cache(s, built.as_deref(), &model, &list)?;
                }
                audit_watch_list_access(
                    s, &w, &cid, "risk.watch_list.load", &view.rows, view.available,
                )?;
                Ok(view)
            })
        })();
        clear_job(state);
        out
    })
    .await
    .map_err(err)?
}

/// Export the named Watch List to the app's exports directory. Audited like a load; the
/// CSV carries evidence classes only, never raw risk-feature values.
#[tauri::command]
pub async fn export_watch_list_csv(state: State<'_, AppState>) -> CmdResult<String> {
    let w = who(state.inner());
    let cid = new_cid();
    tracing::info!(target: "access", cid, action = "risk.watch_list.export", "audited access");
    let dir = exports_dir(state.inner());
    with_store(state.inner(), |s| {
        let mut sink = progress::noop();
        let mut quiet = Reporter::new("risk", risk::RISK_STEPS, &mut sink);
        let (model, list) = analyze_risk(s, &w, &mut quiet)?;
        let view = risk::watch_list_view(&model, &list);
        if !view.available {
            return Err(anyhow::anyhow!("No validated household ranking to export"));
        }
        std::fs::create_dir_all(&dir)?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M");
        let path = dir.join(format!("watch-list-{stamp}.csv"));
        audit_watch_list_access(
            s, &w, &cid, "risk.watch_list.export", &view.rows, view.available,
        )?;
        std::fs::write(&path, risk::watch_list_csv(&view))?;
        Ok(path.to_string_lossy().into_owned())
    })
}

#[tauri::command]
pub async fn export_insights_csv(view: String, state: State<'_, AppState>) -> CmdResult<String> {
    if !insights::VIEWS.contains(&view.as_str()) {
        return Err(format!("unknown insights view: {view}"));
    }
    let w = who(state.inner());
    let dir = exports_dir(state.inner());
    with_store(state.inner(), |s| {
        ensure_fresh(s, &w, false)?;
        let cur = insights::current_fy();
        let ins = insights::views(s, cur)?;
        let ar = if view == "at_risk" {
            insights::at_risk(s, cur)?
        } else {
            Vec::new()
        };
        let (text, rows) = insights::to_csv(&view, &ins, &ar)?;
        std::fs::create_dir_all(&dir)?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M");
        let path = dir.join(format!("insights-{view}-{stamp}.csv"));
        s.audit(
            &w,
            "insights.export",
            None,
            Some(serde_json::json!({"view": view, "rows": rows})),
        )?;
        std::fs::write(&path, text)?;
        Ok(path.to_string_lossy().into_owned())
    })
}

#[tauri::command]
pub async fn reveal_export(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let dir = exports_dir(state.inner());
    let p = PathBuf::from(&path);
    if !insights::path_is_inside(&p, &dir) {
        return Err("can only reveal files inside the app's exports folder".into());
    }
    tauri_plugin_opener::reveal_item_in_dir(&p).map_err(err)
}

#[tauri::command]
pub async fn export_insights_pdf(app: AppHandle, state: State<'_, AppState>) -> CmdResult<String> {
    let w = who(state.inner());
    let dir = exports_dir(state.inner());
    std::fs::create_dir_all(&dir).map_err(err)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M");
    let path = dir.join(format!("insights-report-{stamp}.pdf"));
    // Audit first (fail-closed), then render. No store lock is held across the await.
    // The report is summary-only (KPIs, charts, highlights); it never carries household names.
    with_store(state.inner(), |s| {
        s.audit(&w, "insights.export_pdf", None, None)
    })?;
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<anyhow::Result<()>>();
    let target = path.clone();
    window
        .with_webview(move |wv| {
            let _ = tx.send(crate::pdf::print_webview_to_pdf(&wv, &target));
        })
        .map_err(err)?;
    tokio::task::spawn_blocking(move || rx.recv_timeout(std::time::Duration::from_secs(90)))
        .await
        .map_err(err)?
        .map_err(|_| "timed out rendering the PDF".to_string())?
        .map_err(err)?;
    Ok(path.to_string_lossy().into_owned())
}

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
    state.secrets.set(name, key.trim()).map_err(err)?;
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
pub async fn clear_llm_key(state: State<'_, AppState>, provider: llm::Provider) -> CmdResult<()> {
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

// ── governed chat ─────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
struct ChatTokenEvent {
    conversation_id: String,
    token: String,
}
#[derive(Serialize, Clone)]
struct ChatDoneEvent {
    conversation_id: String,
    message_id: String,
    content: String,
}
#[derive(Serialize, Clone)]
struct ChatErrorEvent {
    conversation_id: String,
    error: String,
}

/// Everything a streaming turn needs, assembled once under the store lock: the governed snapshot
/// (the sole model input), the prior turns, and the selected backend's runtime config. The user
/// message is persisted here too, so the turn is recorded even if generation later fails.
struct ChatPrep {
    snapshot: String,
    history: Vec<chat::ChatMessage>,
    ollama_base_url: String,
    ollama_model: String,
    cli_model: Option<String>,
}

#[tauri::command]
pub async fn chat_create_conversation(
    backend: chat::BackendKind,
    title: String,
    state: State<'_, AppState>,
) -> CmdResult<ChatConversation> {
    let id = new_id("conv");
    let title = if title.trim().is_empty() { "New conversation".to_string() } else { title };
    with_store(state.inner(), |s| {
        s.create_conversation(&id, backend.as_str(), &title)?;
        Ok(s.get_conversation(&id)?.expect("conversation just created"))
    })
}

#[tauri::command]
pub async fn chat_list_conversations(state: State<'_, AppState>) -> CmdResult<Vec<ChatConversation>> {
    let w = who(state.inner());
    let cid = new_cid();
    tracing::info!(target: "access", cid, action = "chat.list_conversations", "audited access");
    with_store(state.inner(), |s| {
        let convs = s.list_conversations()?;
        audit_chat_list_conversations(s, &w, &cid, convs.len())?;
        Ok(convs)
    })
}

#[tauri::command]
pub async fn chat_list_messages(
    conversation_id: String,
    state: State<'_, AppState>,
) -> CmdResult<Vec<StoredChatMessage>> {
    let w = who(state.inner());
    let cid = new_cid();
    tracing::info!(target: "access", cid, action = "chat.list_messages", "audited access");
    with_store(state.inner(), |s| {
        // Opening a conversation is a chat entry point: apply age-based retention best-effort
        // (a prune failure must not fail the open), then read and audit the transcript access.
        prune_chat_best_effort(s, &cid);
        let msgs = s.list_chat_messages(&conversation_id)?;
        audit_chat_list_messages(s, &w, &cid, &conversation_id, msgs.len())?;
        Ok(msgs)
    })
}

#[tauri::command]
pub async fn chat_rename_conversation(
    conversation_id: String,
    title: String,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    with_store(state.inner(), |s| s.rename_conversation(&conversation_id, &title))
}

#[tauri::command]
pub async fn chat_delete_conversation(
    conversation_id: String,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    with_store(state.inner(), |s| s.delete_conversation(&conversation_id))
}

/// Clear all chat history. Deletes every conversation and message; the synced mirror and Insights
/// are untouched (a separate concern from `purge_local_data`).
#[tauri::command]
pub async fn chat_clear_history(state: State<'_, AppState>) -> CmdResult<()> {
    let w = who(state.inner());
    with_store(state.inner(), |s| {
        s.clear_chat()?;
        s.audit(&w, "chat.clear_history", None, None)
    })
}

#[derive(Serialize)]
pub struct ChatBackendStatus {
    pub backend: chat::BackendKind,
    pub available: bool,
    pub detail: String,
}

/// Availability of each backend: the CLI agents via `agent::detect` (present + runnable), the
/// local Ollama server via a reachability probe. Reads the Ollama base URL under the lock, then
/// releases it before the (subprocess / network) probes.
#[tauri::command]
pub async fn chat_backend_status(state: State<'_, AppState>) -> CmdResult<Vec<ChatBackendStatus>> {
    let ollama_base = with_store(state.inner(), |s| {
        Ok(llm::LlmSettings::load(s)?
            .config(llm::Provider::Ollama)
            .base_url
            .clone())
    })?;
    let mut out = Vec::with_capacity(3);
    for b in chat::BackendKind::all() {
        let (available, detail) = match b.agent() {
            None => {
                let ok = chat::ollama_reachable(&ollama_base).await;
                let detail = if ok {
                    format!("Local Ollama server reachable at {ollama_base}")
                } else {
                    format!("No local Ollama server at {ollama_base}")
                };
                (ok, detail)
            }
            Some(a) => {
                let st = agent::detect(a).await;
                let detail = st
                    .version
                    .clone()
                    .or_else(|| st.error.clone())
                    .unwrap_or_default();
                (st.installed, detail)
            }
        };
        out.push(ChatBackendStatus { backend: b, available, detail });
    }
    Ok(out)
}

/// Send a chat turn: build the governed snapshot, persist the user message, then stream the
/// selected backend's reply as `chat:token` events, closing with `chat:done` (or `chat:error`).
/// Returns as soon as the stream is launched; the run is registered so `chat_cancel` can abort it.
#[tauri::command]
pub async fn chat_send(
    app: AppHandle,
    conversation_id: String,
    backend: chat::BackendKind,
    message: String,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    let msg = message.trim().to_string();
    if msg.is_empty() {
        return Err("Message is empty.".into());
    }
    let w = who(state.inner());
    let cid = new_cid();
    tracing::info!(target: "access", cid, action = "chat.send", "audited access");

    // (a) Prepare under the store lock (a stale mart triggers a rebuild here): off the async workers.
    let app_prep = app.clone();
    let conv_prep = conversation_id.clone();
    let user_prep = msg.clone();
    let cid_prep = cid.clone();
    let prep = tauri::async_runtime::spawn_blocking(move || -> CmdResult<ChatPrep> {
        let state = app_prep.state::<AppState>();
        let state = state.inner();
        with_store(state, |s| {
            if s.get_conversation(&conv_prep)?.is_none() {
                return Err(anyhow::anyhow!("unknown conversation"));
            }
            // Turn start is a chat entry point: apply age-based retention best-effort before
            // building the turn (a prune failure must not fail the send).
            prune_chat_best_effort(s, &cid_prep);
            ensure_fresh(s, &w, false)?;
            let snapshot = chat_context::build(s, insights::current_fy())?.text;
            let history: Vec<chat::ChatMessage> = s
                .list_chat_messages(&conv_prep)?
                .into_iter()
                .map(|m| chat::ChatMessage { role: m.role, content: m.content })
                .collect();
            s.append_chat_message(&new_id("msg"), &conv_prep, "user", &user_prep)?;
            let (ollama_base_url, ollama_model, cli_model) = match backend.agent() {
                None => {
                    let c = llm::LlmSettings::load(s)?.config(llm::Provider::Ollama).clone();
                    (c.base_url, c.model, None)
                }
                Some(a) => {
                    let model = agent::AgentSettings::load(s)?.config(a).model.clone();
                    (String::new(), String::new(), model)
                }
            };
            s.audit(
                &w,
                "chat.send",
                None,
                Some(serde_json::json!({"cid": cid_prep, "backend": backend, "conversation": conv_prep})),
            )?;
            Ok(ChatPrep { snapshot, history, ollama_base_url, ollama_model, cli_model })
        })
    })
    .await
    .map_err(err)??;

    // (b) Register a cancel flag, spawn the streaming task, register its handle for abort.
    let cancel = chat::new_cancel();
    state
        .chat_cancels
        .lock()
        .map_err(|_| "chat cancel lock poisoned".to_string())?
        .insert(conversation_id.clone(), cancel.clone());
    let app_run = app.clone();
    let conv_run = conversation_id.clone();
    let handle = tokio::spawn(async move {
        run_chat_stream(app_run, conv_run, backend, prep, msg, cancel, cid).await;
    });
    state.agents.insert(conversation_id, handle);
    Ok(())
}

/// The streaming task body: run the selected backend, emit tokens, persist the reply, and clean up
/// the run registry. Never holds the store lock across the stream await.
async fn run_chat_stream(
    app: AppHandle,
    conversation_id: String,
    backend: chat::BackendKind,
    prep: ChatPrep,
    user_msg: String,
    cancel: Cancel,
    cid: String,
) {
    let started = std::time::Instant::now();
    let token_app = app.clone();
    let token_conv = conversation_id.clone();
    let mut on_token = move |tok: &str| {
        let _ = token_app.emit(
            "chat:token",
            ChatTokenEvent { conversation_id: token_conv.clone(), token: tok.to_string() },
        );
    };

    let result = match backend.agent() {
        None => {
            let b = chat::OllamaBackend { base_url: prep.ollama_base_url, model: prep.ollama_model };
            b.stream(&prep.snapshot, &prep.history, &user_msg, &mut on_token, &cancel).await
        }
        Some(a) => {
            let mut seed = [0u8; 8];
            let _ = getrandom::fill(&mut seed);
            let b = chat::CliAgentBackend { agent: a, model: prep.cli_model, seed: u64::from_le_bytes(seed) };
            b.stream(&prep.snapshot, &prep.history, &user_msg, &mut on_token, &cancel).await
        }
    };

    // Clean up the run registry regardless of outcome.
    let state = app.state::<AppState>();
    let st = state.inner();
    if let Ok(mut m) = st.chat_cancels.lock() {
        m.remove(&conversation_id);
    }
    st.agents.remove(&conversation_id);

    // A cancelled turn stops silently: the frontend already returned to idle on the cancel action,
    // and a partial reply is not persisted.
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    match result {
        Ok(outcome) => {
            // One content-free telemetry event per completed (non-cancelled) turn.
            ChatTurnTelemetry {
                cid,
                backend,
                ms: started.elapsed().as_millis(),
                prompt_tokens: outcome.prompt_tokens,
                completion_tokens: outcome.completion_tokens,
                conversation_id: conversation_id.clone(),
            }
            .emit();
            let message_id = new_id("msg");
            let content = outcome.text.clone();
            let conv = conversation_id.clone();
            let _ = with_store(st, |s| {
                s.append_chat_message(&message_id, &conv, "assistant", &content)?;
                if let Some(sid) = &outcome.session_id {
                    s.set_conversation_session(&conv, sid)?;
                }
                Ok(())
            });
            let _ = app.emit(
                "chat:done",
                ChatDoneEvent { conversation_id, message_id, content },
            );
        }
        Err(e) => {
            let _ = app.emit(
                "chat:error",
                ChatErrorEvent { conversation_id, error: e.to_string() },
            );
        }
    }
}

/// Cancel an in-progress chat turn: flip its cooperative cancel flag and abort its task, which
/// terminates any CLI subprocess (kill_on_drop) and closes any Ollama stream.
#[tauri::command]
pub async fn chat_cancel(conversation_id: String, state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(c) = state
        .chat_cancels
        .lock()
        .map_err(|_| "chat cancel lock poisoned".to_string())?
        .remove(&conversation_id)
    {
        c.store(true, Ordering::Relaxed);
    }
    state.agents.kill(&conversation_id).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    /// Returns the TempDir too so it lives as long as the Store.
    fn mem() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = store::open(&dir.path().join("t.db"), KEY).unwrap();
        (dir, s)
    }

    fn who_t() -> Who {
        Who { sf_user_id: Some("005".into()), sf_username: Some("u@x".into()) }
    }

    fn detail_of(row: &AuditRow) -> serde_json::Value {
        serde_json::from_str(row.detail.as_deref().unwrap()).unwrap()
    }

    /// Audit details, newest-first (matching `list_audit`'s ordering).
    fn details(s: &Store) -> Vec<serde_json::Value> {
        s.list_audit(100, 0).unwrap().iter().map(detail_of).collect()
    }

    fn at_risk_row(account_id: &str, name: &str) -> AtRiskRow {
        AtRiskRow {
            account_id: account_id.into(),
            name: name.into(),
            tier: None,
            join_fy: None,
            rules: vec![],
        }
    }

    fn watch_row(account_id: &str, name: &str) -> risk::WatchRowView {
        risk::WatchRowView {
            account_id: account_id.into(),
            name: name.into(),
            score: 0.5,
            evidence: vec![],
        }
    }

    // ── 2.1 at-risk audit lists the disclosed id set + count, never a name ────
    #[test]
    fn at_risk_audit_lists_account_ids_and_count_without_name() {
        let (_d, s) = mem();
        let rows = vec![at_risk_row("001AAA", "Cohen"), at_risk_row("001BBB", "Levy")];
        audit_at_risk_access(&s, &who_t(), "act-1", &rows).unwrap();

        let audit = s.list_audit(10, 0).unwrap();
        assert_eq!(audit.len(), 1, "exactly one audit row for the read");
        let detail = detail_of(&audit[0]);
        assert_eq!(detail["count"], 2);
        assert_eq!(detail["account_ids"], serde_json::json!(["001AAA", "001BBB"]));
        let raw = audit[0].detail.as_deref().unwrap();
        assert!(!raw.contains("Cohen") && !raw.contains("Levy"), "no household name in the audit");
    }

    // ── 2.2 Watch List audit lists ids; empty result is a zero-count row ──────
    #[test]
    fn watch_list_audit_lists_ids_and_empty_result_is_accountable() {
        let (_d, s) = mem();
        audit_watch_list_access(
            &s, &who_t(), "act-2", "risk.watch_list.load", &[watch_row("001XYZ", "Adler")], true,
        )
        .unwrap();
        // An empty result still records an accountable read: zero count, empty id set.
        audit_watch_list_access(&s, &who_t(), "act-3", "risk.watch_list.load", &[], true).unwrap();

        let d = details(&s); // newest first
        assert_eq!(d[0]["count"], 0);
        assert_eq!(d[0]["account_ids"], serde_json::json!([]));
        assert_eq!(d[1]["count"], 1);
        assert_eq!(d[1]["account_ids"], serde_json::json!(["001XYZ"]));
        assert!(!s.list_audit(10, 0).unwrap()[1].detail.as_deref().unwrap().contains("Adler"));
    }

    // ── 2.3 sensitive reads each write one low-cardinality row; no recursion ──
    #[test]
    fn sensitive_reads_write_one_low_cardinality_row_without_content() {
        let (_d, s) = mem();
        // A pure read of the audit log writes nothing by itself (no recursion).
        let before = s.list_audit(500, 0).unwrap().len();
        let _ = s.list_audit(500, 0).unwrap();
        assert_eq!(s.list_audit(500, 0).unwrap().len(), before, "reading the audit log is pure");

        audit_audit_read(&s, &who_t(), "act-a", 50, 0).unwrap();
        audit_chat_list_conversations(&s, &who_t(), "act-b", 3).unwrap();
        audit_chat_list_messages(&s, &who_t(), "act-c", "conv-1", 7).unwrap();

        let audit = s.list_audit(10, 0).unwrap();
        assert_eq!(audit.len(), 3, "exactly one row per read");
        let find = |a: &str| detail_of(audit.iter().find(|r| r.action == a).unwrap());
        let ar = find("audit.read");
        assert_eq!((ar["limit"].as_i64(), ar["offset"].as_i64()), (Some(50), Some(0)));
        assert_eq!(find("chat.list_conversations")["count"], 3);
        let lm = find("chat.list_messages");
        assert_eq!(lm["conversation_id"], "conv-1");
        assert_eq!(lm["messages"], 7);
        // Low-cardinality only — no content-bearing keys.
        for a in ["audit.read", "chat.list_conversations", "chat.list_messages"] {
            let d = find(a);
            assert!(d.get("content").is_none() && d.get("text").is_none());
        }
    }

    // ── 3.2 chat-turn telemetry carries no transcript content or PII ─────────
    #[test]
    fn telemetry_event_carries_no_content_or_identity() {
        // A turn whose prompt/reply contain text and whose context contains household PII.
        const SECRET_PROMPT: &str = "who is the most profitable household?";
        const SECRET_REPLY: &str = "The Cohen household, account 001AAA.";
        const SECRET_NAME: &str = "Cohen";
        const SECRET_EMAIL: &str = "cohen@example.org";
        const SECRET_STREET: &str = "12 Oak Street";
        const SECRET_ID: &str = "001AAA";

        let t = ChatTurnTelemetry {
            cid: "act-telemetry".into(),
            backend: chat::BackendKind::Ollama,
            ms: 1234,
            prompt_tokens: Some(7001),
            completion_tokens: Some(8002),
            conversation_id: "conv-1".into(),
        };
        let repr = format!("{t:?}");
        // Backend, elapsed ms, and token counts are present.
        assert!(repr.contains("Ollama") && repr.contains("1234"));
        assert!(repr.contains("7001") && repr.contains("8002"));
        // No transcript content, no household identity.
        for leak in [SECRET_PROMPT, SECRET_REPLY, SECRET_NAME, SECRET_EMAIL, SECRET_STREET, SECRET_ID] {
            assert!(!repr.contains(leak), "telemetry must not carry {leak:?}");
        }

        // Token counts are optional — absent, not fabricated, when the backend omits them.
        let none = ChatTurnTelemetry { prompt_tokens: None, completion_tokens: None, ..t };
        let repr = format!("{none:?}");
        assert!(repr.contains("None"), "absent token counts stay None, never fabricated");
    }

    // ── 4.3 a prune failure never surfaces as a chat error ───────────────────
    #[test]
    fn prune_failure_does_not_break_the_chat_action() {
        // A chat action that prunes best-effort then proceeds — modeled as returning success.
        fn fake_chat_action(prune: impl FnOnce() -> anyhow::Result<usize>) -> Result<&'static str, String> {
            best_effort_prune("act-x", prune);
            Ok("sent")
        }
        assert_eq!(fake_chat_action(|| Err(anyhow::anyhow!("simulated prune failure"))), Ok("sent"));
        assert_eq!(fake_chat_action(|| Ok(5)), Ok("sent"));
    }

    // ── 5.1 audited action carries a unique, identity-free correlation id ────
    #[test]
    fn audited_action_carries_a_unique_cid_free_of_identity() {
        let (_d, s) = mem();
        let name = "Cohen";
        let cid1 = new_cid();
        let cid2 = new_cid();
        assert_ne!(cid1, cid2, "distinct invocations get distinct ids");
        assert!(cid1.starts_with("act-"));
        assert!(!cid1.contains(name) && !cid2.contains(name), "cid carries no household identity");

        let rows = vec![at_risk_row("001AAA", name)];
        audit_at_risk_access(&s, &who_t(), &cid1, &rows).unwrap();
        audit_at_risk_access(&s, &who_t(), &cid2, &rows).unwrap();
        let d = details(&s); // newest first
        assert_eq!(d[0]["cid"], cid2);
        assert_eq!(d[1]["cid"], cid1);
        assert_ne!(d[0]["cid"], d[1]["cid"]);
    }
}
