mod db;
mod manifest;
mod media_tools;
mod models;
mod phone_access;
mod publisher;

use models::{
    AppSnapshot, ChannelPublishResult, ClearSourceSummary, CreatePublisherProfileRequest,
    DownloadSourceSummary, ImportSummary, IngestSummary, Lesson, LessonNote,
    ManifestValidationReport, MediaStorageAudit, MediaStorageCleanup, NativePlaybackResult,
    NostrChannelPreview, OpenMediaResult, PhoneMediaScope, PhoneMediaSession,
    PublishTeacherChannelRequest, PublisherChannel, PublisherEndpointTestReport,
    PublisherEndpointTestRequest, PublisherProfile, RuntimeDiagnostics,
    SavePublisherChannelRequest, TrustCuratorSummary, TrustedCurator, WatchState,
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
fn save_watch_state(
    app: tauri::AppHandle,
    lesson_id: String,
    progress_seconds: i64,
    duration_seconds: Option<i64>,
    completed: bool,
) -> Result<WatchState, String> {
    let connection = db::open_connection(&app)?;
    db::save_watch_state(
        &connection,
        lesson_id,
        progress_seconds,
        duration_seconds,
        completed,
    )
}

#[tauri::command]
fn save_lesson_note(
    app: tauri::AppHandle,
    lesson_id: String,
    body: String,
) -> Result<LessonNote, String> {
    let connection = db::open_connection(&app)?;
    db::save_lesson_note(&connection, lesson_id, body)
}

#[tauri::command]
fn update_lesson_organization(
    app: tauri::AppHandle,
    lesson_id: String,
    teacher_display_name: String,
    collection_title: String,
) -> Result<Lesson, String> {
    let mut connection = db::open_connection(&app)?;
    db::update_lesson_organization(
        &mut connection,
        lesson_id,
        teacher_display_name,
        collection_title,
    )
}

#[tauri::command]
fn resolve_media_file_path(app: tauri::AppHandle, media_file_id: String) -> Result<String, String> {
    db::resolve_media_file_path(&app, media_file_id)
}

#[tauri::command]
fn resolve_media_thumbnail_path(
    app: tauri::AppHandle,
    media_file_id: String,
) -> Result<String, String> {
    db::resolve_media_thumbnail_path(&app, media_file_id)
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
fn refresh_source(app: tauri::AppHandle, source_id: String) -> Result<IngestSummary, String> {
    db::refresh_source(&app, source_id)
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
async fn download_source_media(
    app: tauri::AppHandle,
    source_id: String,
) -> Result<DownloadSourceSummary, String> {
    tauri::async_runtime::spawn_blocking(move || db::download_source_media(&app, source_id))
        .await
        .map_err(|error| format!("Downloader worker failed: {error}"))?
}

#[tauri::command]
fn audit_media_storage(app: tauri::AppHandle) -> Result<MediaStorageAudit, String> {
    db::audit_media_storage(&app)
}

#[tauri::command]
fn cleanup_media_storage(app: tauri::AppHandle) -> Result<MediaStorageCleanup, String> {
    db::cleanup_media_storage(&app)
}

#[tauri::command]
fn play_media_file_native(
    app: tauri::AppHandle,
    media_file_id: String,
) -> Result<NativePlaybackResult, String> {
    db::play_media_file_native(&app, media_file_id)
}

#[tauri::command]
fn open_pdf_file(app: tauri::AppHandle, media_file_id: String) -> Result<OpenMediaResult, String> {
    db::open_pdf_file(&app, media_file_id)
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

#[tauri::command]
fn list_publisher_profiles(app: tauri::AppHandle) -> Result<Vec<PublisherProfile>, String> {
    publisher::list_publisher_profiles(&app)
}

#[tauri::command]
fn list_publisher_channels(app: tauri::AppHandle) -> Result<Vec<PublisherChannel>, String> {
    publisher::list_publisher_channels(&app)
}

#[tauri::command]
fn save_publisher_channel(
    app: tauri::AppHandle,
    request: SavePublisherChannelRequest,
) -> Result<PublisherChannel, String> {
    publisher::save_publisher_channel(&app, request)
}

#[tauri::command]
fn create_publisher_profile(
    app: tauri::AppHandle,
    request: CreatePublisherProfileRequest,
) -> Result<PublisherProfile, String> {
    publisher::create_publisher_profile(&app, request)
}

#[tauri::command]
fn unlock_publisher_profile(
    app: tauri::AppHandle,
    profile_id: String,
    passphrase: String,
) -> Result<PublisherProfile, String> {
    publisher::unlock_publisher_profile(&app, profile_id, passphrase)
}

#[tauri::command]
fn publish_teacher_channel(
    app: tauri::AppHandle,
    request: PublishTeacherChannelRequest,
) -> Result<ChannelPublishResult, String> {
    publisher::publish_teacher_channel(&app, request)
}

#[tauri::command]
fn test_publisher_endpoints(
    app: tauri::AppHandle,
    request: PublisherEndpointTestRequest,
) -> Result<PublisherEndpointTestReport, String> {
    publisher::test_publisher_endpoints(&app, request)
}

#[tauri::command]
fn ingest_nostr_channel(
    app: tauri::AppHandle,
    channel_ref: String,
) -> Result<IngestSummary, String> {
    publisher::ingest_nostr_channel(&app, channel_ref)
}

#[tauri::command]
fn preview_nostr_channel(
    app: tauri::AppHandle,
    channel_ref: String,
) -> Result<NostrChannelPreview, String> {
    publisher::preview_nostr_channel(&app, channel_ref)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(PhoneAccessState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle();
            db::initialize(handle).map_err(|error| -> Box<dyn std::error::Error> {
                Box::new(std::io::Error::other(format!(
                    "database initialization failed: {error}"
                )))
            })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            get_runtime_diagnostics,
            search_lessons,
            save_watch_state,
            save_lesson_note,
            update_lesson_organization,
            resolve_media_file_path,
            resolve_media_thumbnail_path,
            import_local_files,
            ingest_source_url,
            refresh_source,
            clear_source_content,
            download_source_media,
            audit_media_storage,
            cleanup_media_storage,
            play_media_file_native,
            open_pdf_file,
            start_phone_media_session,
            get_phone_media_session,
            stop_phone_media_session,
            validate_collection_manifest,
            add_trusted_curator,
            remove_trusted_curator,
            list_publisher_profiles,
            list_publisher_channels,
            save_publisher_channel,
            create_publisher_profile,
            unlock_publisher_profile,
            publish_teacher_channel,
            test_publisher_endpoints,
            ingest_nostr_channel,
            preview_nostr_channel
        ])
        .run(tauri::generate_context!())
        .expect("error while running Duroos Watcher");
}
