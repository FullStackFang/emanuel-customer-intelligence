pub mod agent;
pub mod auth;
pub mod chat;
pub mod chat_context;
pub mod commands;
pub mod config;
pub mod insights;
pub mod llm;
pub mod pdf;
pub mod profile;
pub mod progress;
pub mod risk;
pub mod salesforce;
pub mod secrets;
pub mod segment;
pub mod store;

use commands::AppState;
use std::path::Path;
use std::sync::Mutex;
use tauri::Manager;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Initialize `tracing` with the existing stdout layer plus a daily-rotating file layer under
/// `<app_data_dir>/logs/`, so diagnostics survive process exit. A single registry-level
/// `EnvFilter` (default `info`, overridable via `RUST_LOG`) governs both sinks, preserving the
/// `insights_timing` target behavior. Returns the non-blocking writer's `WorkerGuard`, which the
/// caller must hold for the process lifetime; `None` (stdout only) if the logs dir is unwritable.
fn init_tracing(app_data_dir: &Path) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter = || {
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())
    };
    let logs_dir = app_data_dir.join("logs");
    match std::fs::create_dir_all(&logs_dir) {
        Ok(()) => {
            let (writer, guard) =
                tracing_appender::non_blocking(tracing_appender::rolling::daily(&logs_dir, "app.log"));
            tracing_subscriber::registry()
                .with(filter())
                .with(tracing_subscriber::fmt::layer())
                .with(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(writer))
                .init();
            Some(guard)
        }
        Err(e) => {
            tracing_subscriber::registry()
                .with(filter())
                .with(tracing_subscriber::fmt::layer())
                .init();
            tracing::warn!("could not create log directory {logs_dir:?}: {e}; logging to stdout only");
            None
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = config::Config::from_env().expect("configuration: set SF_CLIENT_ID in .env");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let app_data_dir = app.path().app_data_dir()?;
            // Tracing is initialized here (not before the builder) because the file sink lives
            // under the app data dir, which only the resolved app can provide.
            let log_guard = init_tracing(&app_data_dir);
            let db_path = app_data_dir.join("mirror.db");
            app.manage(AppState {
                cfg: cfg.clone(),
                secrets: secrets::Secrets::default_service(),
                db_path,
                store: Mutex::new(None),
                identity: Mutex::new(None),
                job: Mutex::new(None),
                agents: Default::default(),
                chat_cancels: Mutex::new(std::collections::HashMap::new()),
                _log_guard: log_guard,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::recover_identity,
            commands::connect,
            commands::disconnect,
            commands::scan,
            commands::list_objects,
            commands::set_object_selected,
            commands::list_fields,
            commands::set_field_withheld,
            commands::sync_selected,
            commands::profile_selected,
            commands::query_segment,
            commands::get_audit,
            commands::purge_local_data,
            commands::get_insights,
            commands::get_insights_job,
            commands::zip_geography,
            commands::zip_geography_years,
            commands::neighborhood_retention_years,
            commands::neighborhood_geography,
            commands::get_at_risk,
            commands::get_risk_summary,
            commands::get_watch_list,
            commands::export_watch_list_csv,
            commands::export_insights_csv,
            commands::reveal_export,
            commands::export_insights_pdf,
            commands::get_llm_settings,
            commands::set_llm_settings,
            commands::set_llm_key,
            commands::clear_llm_key,
            commands::test_llm_connection,
            commands::chat_create_conversation,
            commands::chat_list_conversations,
            commands::chat_list_messages,
            commands::chat_rename_conversation,
            commands::chat_delete_conversation,
            commands::chat_clear_history,
            commands::chat_backend_status,
            commands::chat_send,
            commands::chat_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
