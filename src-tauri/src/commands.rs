//! The command boundary — the only surface the webview can reach.
//! Every command: (1) never returns a token, (2) audits itself, (3) never holds
//! the store lock across an await.

use crate::auth::{self, Identity};
use crate::config::Config;
use crate::insights::{self, AtRiskRow, Insights};
use crate::profile;
use crate::salesforce::SfClient;
use crate::secrets::{Secrets, TOKENS};
use crate::segment::{self, SegmentReq, SegmentResult};
use crate::store::{self, AuditRow, FieldRow, ObjectRow, Store, Who};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

pub struct AppState {
    pub cfg: Config,
    pub secrets: Secrets,
    pub db_path: PathBuf,
    pub store: Mutex<Option<Store>>,
    pub identity: Mutex<Option<Identity>>,
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

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> CmdResult<StatusView> {
    let tokens = auth::load_tokens(&state.secrets).map_err(err)?;
    let connected = tokens.is_some();
    if connected && state.identity.lock().map(|g| g.is_none()).unwrap_or(true) {
        // App restarted with a stored session: recover who we are (refreshes if needed).
        let mut c = client(state.inner()).await?;
        let id = match auth::fetch_identity(c.tokens()).await {
            Ok(id) => Some(id),
            Err(_) => {
                let t = auth::refresh(&state.cfg, &state.secrets, c.tokens())
                    .await
                    .map_err(err)?;
                c = SfClient::new(state.cfg.clone(), state.secrets.clone(), t);
                Some(auth::fetch_identity(c.tokens()).await.map_err(err)?)
            }
        };
        *state.identity.lock().map_err(|_| "lock".to_string())? = id;
    }
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
        if s.table_exists("Account")? {
            let info = insights::rebuild(s)?;
            s.audit(
                &w,
                "insights.rebuild",
                None,
                Some(serde_json::json!({"households": info.households, "unavailable": info.unavailable})),
            )?;
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
    with_store(state.inner(), |s| {
        s.list_audit(limit.clamp(1, 500), offset.max(0))
    })
}

#[tauri::command]
pub async fn purge_local_data(state: State<'_, AppState>) -> CmdResult<()> {
    let w = who(state.inner());
    with_store(state.inner(), |s| {
        s.purge_mirror()?;
        s.audit(&w, "data.purge", None, None)
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

/// Rebuild the mart if it is missing or older than the newest sync.
fn ensure_fresh(s: &mut Store, w: &Who, force: bool) -> anyhow::Result<()> {
    let built = s.get_meta("insights_built_at")?;
    let newest = s.newest_sync_at()?;
    let stale = match (&built, &newest) {
        (None, _) => true,
        (Some(b), Some(n)) => n > b, // ISO-8601 strings compare chronologically
        (Some(_), None) => false,
    };
    if force || stale || !s.table_exists(insights::MART)? {
        let info = insights::rebuild(s)?;
        s.audit(
            w,
            "insights.rebuild",
            None,
            Some(
                serde_json::json!({"households": info.households, "unavailable": info.unavailable}),
            ),
        )?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_insights(force_rebuild: bool, state: State<'_, AppState>) -> CmdResult<Insights> {
    let w = who(state.inner());
    with_store(state.inner(), |s| {
        ensure_fresh(s, &w, force_rebuild)?;
        insights::views(s, insights::current_fy())
    })
}

#[tauri::command]
pub async fn get_at_risk(state: State<'_, AppState>) -> CmdResult<Vec<AtRiskRow>> {
    let w = who(state.inner());
    with_store(state.inner(), |s| {
        ensure_fresh(s, &w, false)?;
        let rows = insights::at_risk(s, insights::current_fy())?;
        s.audit(
            &w,
            "insights.at_risk",
            None,
            Some(serde_json::json!({"count": rows.len()})),
        )?;
        Ok(rows)
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
        std::fs::write(&path, text)?;
        s.audit(
            &w,
            "insights.export",
            None,
            Some(serde_json::json!({"view": view, "rows": rows})),
        )?;
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
