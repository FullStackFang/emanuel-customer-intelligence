pub mod agent;
pub mod auth;
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
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cfg = config::Config::from_env().expect("configuration: set SF_CLIENT_ID in .env");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let db_path = app.path().app_data_dir()?.join("mirror.db");
            app.manage(AppState {
                cfg: cfg.clone(),
                secrets: secrets::Secrets::default_service(),
                db_path,
                store: Mutex::new(None),
                identity: Mutex::new(None),
                job: Mutex::new(None),
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
