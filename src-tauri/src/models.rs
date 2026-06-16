use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SourceCapability {
    pub metadata: String,
    pub download: String,
    pub auto_update: String,
    pub auth_required: bool,
    pub auth_mode: String,
    pub reliability: String,
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub platform: String,
    pub label: String,
    pub identifier: String,
    pub feed_format: String,
    pub feed_transport: String,
    pub trust_state: String,
    pub auth_mode: String,
    pub update_schedule: String,
    pub capability: SourceCapability,
    pub enabled: bool,
    pub last_checked_at: Option<String>,
    pub trusted_curator_id: Option<String>,
    pub last_verified_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Teacher {
    pub id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub source_links: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TeacherRelay {
    pub id: String,
    pub teacher_id: String,
    pub title: String,
    pub feed_url: String,
    pub feed_format: String,
    pub feed_transport: String,
    pub trust_state: String,
    pub subscriber_count: i64,
    pub visibility: String,
    pub trust_policy: String,
    pub auto_download: bool,
    pub last_published_at: Option<String>,
    pub last_verified_at: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LiveSession {
    pub id: String,
    pub teacher_id: String,
    pub relay_id: String,
    pub title: String,
    pub provider: String,
    pub provider_url: String,
    pub status: String,
    pub starts_at: String,
    pub archive_lesson_id: Option<String>,
    pub auto_publish_archive: bool,
    pub recording_policy: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub title: String,
    pub owner_label: String,
    pub sort_order: i64,
    pub lesson_count: i64,
    pub source_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Lesson {
    pub id: String,
    pub title: String,
    pub content_type: String,
    pub teacher_id: String,
    pub collection_id: String,
    pub source_id: String,
    pub source_url: String,
    pub published_at: Option<String>,
    pub description: Option<String>,
    pub thumbnail_tone: String,
    pub duration_seconds: Option<i64>,
    pub media_file_id: Option<String>,
    pub provenance_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaFile {
    pub id: String,
    pub lesson_id: String,
    pub relative_path: String,
    pub content_hash: String,
    pub size_bytes: i64,
    pub codec: Option<String>,
    pub import_status: String,
    pub hash_verification_state: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceRecord {
    pub id: String,
    pub lesson_id: String,
    pub origin_url: String,
    pub permission_note: String,
    pub imported_at: String,
    pub adapter_name: String,
    pub content_hash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WatchState {
    pub lesson_id: String,
    pub progress_seconds: i64,
    pub completed: bool,
    pub last_watched_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub source_id: Option<String>,
    pub lesson_id: Option<String>,
    pub label: String,
    pub detail: String,
    pub retry_count: i64,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrustedCurator {
    pub id: String,
    pub display_name: String,
    pub public_key: String,
    pub trust_note: Option<String>,
    pub added_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ManifestCuratorIdentity {
    pub id: String,
    pub display_name: String,
    pub public_key: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub sources: Vec<Source>,
    pub teachers: Vec<Teacher>,
    pub teacher_relays: Vec<TeacherRelay>,
    pub live_sessions: Vec<LiveSession>,
    pub collections: Vec<Collection>,
    pub lessons: Vec<Lesson>,
    pub media_files: Vec<MediaFile>,
    pub provenance_records: Vec<ProvenanceRecord>,
    pub watch_state: Vec<WatchState>,
    pub jobs: Vec<Job>,
    pub trusted_curators: Vec<TrustedCurator>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub imported: i64,
    pub skipped: i64,
    pub failed: i64,
    pub messages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestSummary {
    pub source_url: String,
    pub discovered: i64,
    pub imported: i64,
    pub skipped: i64,
    pub failed: i64,
    pub messages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearSourceSummary {
    pub source_id: String,
    pub source_label: String,
    pub removed_source: bool,
    pub lessons_removed: i64,
    pub media_files_removed: i64,
    pub jobs_removed: i64,
    pub messages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSourceSummary {
    pub source_id: String,
    pub source_label: String,
    pub attempted: i64,
    pub downloaded: i64,
    pub skipped: i64,
    pub failed: i64,
    pub messages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDiagnostics {
    pub desktop_runtime_available: bool,
    pub yt_dlp_available: bool,
    pub yt_dlp_version: Option<String>,
    pub yt_dlp_command: Option<String>,
    pub yt_dlp_cookies_configured: bool,
    pub yt_dlp_cookies_file: Option<String>,
    pub messages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PhoneMediaScope {
    pub source_id: Option<String>,
    pub collection_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PhoneMediaShareItem {
    pub media_file_id: String,
    pub lesson_id: String,
    pub title: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub duration_seconds: Option<i64>,
    pub teacher_label: Option<String>,
    pub collection_title: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PhoneMediaSession {
    pub id: String,
    pub active: bool,
    pub base_url: Option<String>,
    pub playlist_url: Option<String>,
    pub started_at: Option<String>,
    pub item_count: i64,
    pub items: Vec<PhoneMediaShareItem>,
    pub messages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestValidationReport {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub trust_state: Option<String>,
    pub curator: Option<ManifestCuratorIdentity>,
    pub trusted_curator_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustCuratorSummary {
    pub curator_id: String,
    pub display_name: String,
    pub public_key: String,
    pub sources_updated: i64,
    pub messages: Vec<String>,
}
