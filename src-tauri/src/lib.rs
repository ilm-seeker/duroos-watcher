mod db;
mod manifest;
mod models;
mod phone_access;

use models::{
    AppSnapshot, ClearSourceSummary, DownloadSourceSummary, ImportSummary, IngestSummary, Lesson,
    ManifestValidationReport, PhoneMediaScope, PhoneMediaSession, RuntimeDiagnostics,
    TrustCuratorSummary, TrustedCurator,
};
use phone_access::PhoneAccessState;

#[tauri::command]
fn get_app_snapshot(app: tauri::AppHandle) -> Result<AppSnapshot, String> {
    let connection = db::open_connection(&app)?;
    db::fetch_snapshot(&connection)
}

#[tauri::command]
fn get_runtime_diagnostics(app: tauri::AppHandle) -> Result<RuntimeDiagnostics, String> {
    Ok(db::runtime_diagnostics(&app))
}

#[tauri::command]
fn search_lessons(app: tauri::AppHandle, query: String) -> Result<Vec<Lesson>, String> {
    let connection = db::open_connection(&app)?;
    db::search_lessons(&connection, &query)
}

#[tauri::command]
fn resolve_media_file_path(app: tauri::AppHandle, media_file_id: String) -> Result<String, String> {
    db::resolve_media_file_path(&app, media_file_id)
}

#[tauri::command]
fn import_local_files(app: tauri::AppHandle, paths: Vec<String>) -> Result<ImportSummary, String> {
    db::import_local_files(&app, paths)
}

#[tauri::command]
fn ingest_source_url(app: tauri::AppHandle, source_url: String) -> Result<IngestSummary, String> {
    db::ingest_source_url(&app, source_url)
}

#[tauri::command]
fn clear_source_content(
    app: tauri::AppHandle,
    source_id: String,
    remove_source: bool,
) -> Result<ClearSourceSummary, String> {
    db::clear_source_content(&app, source_id, remove_source)
}

#[tauri::command]
fn download_source_media(
    app: tauri::AppHandle,
    source_id: String,
) -> Result<DownloadSourceSummary, String> {
    db::download_source_media(&app, source_id)
}

#[tauri::command]
fn start_phone_media_session(
    app: tauri::AppHandle,
    state: tauri::State<PhoneAccessState>,
    scope: Option<PhoneMediaScope>,
) -> Result<PhoneMediaSession, String> {
    phone_access::start_session(&app, &state, scope)
}

#[tauri::command]
fn get_phone_media_session(
    state: tauri::State<PhoneAccessState>,
) -> Result<Option<PhoneMediaSession>, String> {
    phone_access::current_session(&state)
}

#[tauri::command]
fn stop_phone_media_session(
    state: tauri::State<PhoneAccessState>,
    session_id: String,
) -> Result<PhoneMediaSession, String> {
    phone_access::stop_session(&state, session_id)
}

#[tauri::command]
fn validate_collection_manifest(
    app: tauri::AppHandle,
    manifest_json: String,
) -> Result<ManifestValidationReport, String> {
    let connection = db::open_connection(&app)?;
    db::validate_collection_manifest(&connection, &manifest_json)
}

#[tauri::command]
fn add_trusted_curator(
    app: tauri::AppHandle,
    display_name: String,
    public_key: String,
    trust_note: Option<String>,
) -> Result<TrustedCurator, String> {
    let mut connection = db::open_connection(&app)?;
    db::add_trusted_curator(&mut connection, display_name, public_key, trust_note)
}

#[tauri::command]
fn remove_trusted_curator(
    app: tauri::AppHandle,
    curator_id: String,
) -> Result<TrustCuratorSummary, String> {
    let mut connection = db::open_connection(&app)?;
    db::remove_trusted_curator(&mut connection, curator_id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(PhoneAccessState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle();
            db::initialize(handle).map_err(|error| -> Box<dyn std::error::Error> {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("database initialization failed: {error}"),
                ))
            })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            get_runtime_diagnostics,
            search_lessons,
            resolve_media_file_path,
            import_local_files,
            ingest_source_url,
            clear_source_content,
            download_source_media,
            start_phone_media_session,
            get_phone_media_session,
            stop_phone_media_session,
            validate_collection_manifest,
            add_trusted_curator,
            remove_trusted_curator
        ])
        .run(tauri::generate_context!())
        .expect("error while running Duroos Watcher");
}
