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
    pub thumbnail_relative_path: Option<String>,
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
pub struct LessonNote {
    pub lesson_id: String,
    pub body: String,
    pub updated_at: String,
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
    pub lesson_notes: Vec<LessonNote>,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaStorageAudit {
    pub scanned_files: i64,
    pub referenced_files: i64,
    pub stale_files: i64,
    pub stale_bytes: i64,
    pub partial_files: i64,
    pub stale_samples: Vec<String>,
    pub messages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaStorageCleanup {
    pub audit: MediaStorageAudit,
    pub removed_files: i64,
    pub failed_removals: i64,
    pub reclaimed_bytes: i64,
    pub messages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePlaybackResult {
    pub media_file_id: String,
    pub lesson_id: String,
    pub title: String,
    pub player_name: String,
    pub command_label: String,
    pub launched: bool,
    pub messages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenMediaResult {
    pub media_file_id: String,
    pub lesson_id: String,
    pub title: String,
    pub opened: bool,
    pub messages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDiagnostics {
    pub desktop_runtime_available: bool,
    pub yt_dlp_available: bool,
    pub yt_dlp_version: Option<String>,
    pub yt_dlp_command: Option<String>,
    pub required_media_tools_available: bool,
    pub media_tool_source: String,
    pub missing_media_tools: Vec<String>,
    pub native_playback_available: bool,
    pub native_playback_player: Option<String>,
    pub native_playback_command: Option<String>,
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
pub struct PhoneMediaEndpoint {
    pub label: String,
    pub host: String,
    pub kind: String,
    pub base_url: String,
    pub playlist_url: String,
    pub preferred: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PhoneMediaSession {
    pub id: String,
    pub active: bool,
    pub base_url: Option<String>,
    pub playlist_url: Option<String>,
    #[serde(default)]
    pub endpoints: Vec<PhoneMediaEndpoint>,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NostrRelayConfig {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlossomServerConfig {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMirrorConfig {
    pub service: String,
    pub url: String,
    pub gateway_url: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PublisherProfile {
    pub id: String,
    pub display_name: String,
    pub curator_public_key: String,
    pub nostr_pubkey: String,
    pub relays: Vec<NostrRelayConfig>,
    pub blossom_servers: Vec<BlossomServerConfig>,
    pub created_at: String,
    pub updated_at: String,
    pub vault_configured: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PublisherChannel {
    pub id: String,
    pub profile_id: String,
    pub title: String,
    pub description: Option<String>,
    pub channel_identifier: String,
    pub naddr: Option<String>,
    pub canonical_channel_link: Option<String>,
    pub last_manifest_sha256: Option<String>,
    pub last_manifest_url: Option<String>,
    pub last_published_at: Option<String>,
    pub media_count: i64,
    pub post_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PublishedChannelItem {
    pub id: String,
    pub channel_id: String,
    pub item_type: String,
    pub title: String,
    pub content_type: String,
    pub description: Option<String>,
    pub origin_url: String,
    pub retrieval_url: Option<String>,
    pub sha256: String,
    pub size_bytes: Option<i64>,
    pub mime_type: Option<String>,
    pub published_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreatePublisherProfileRequest {
    pub display_name: String,
    pub passphrase: String,
    pub relays: Vec<NostrRelayConfig>,
    pub blossom_servers: Vec<BlossomServerConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SavePublisherChannelRequest {
    pub profile_id: String,
    pub channel_id: Option<String>,
    pub channel_title: String,
    pub channel_description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PublishedLessonDraft {
    pub title: String,
    pub content_type: String,
    pub path: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PublishedPostDraft {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PublishTeacherChannelRequest {
    pub profile_id: String,
    pub channel_id: Option<String>,
    pub passphrase: String,
    pub channel_title: String,
    pub channel_description: Option<String>,
    pub relays: Vec<NostrRelayConfig>,
    pub blossom_servers: Vec<BlossomServerConfig>,
    #[serde(default)]
    pub archive_mirrors: Vec<ArchiveMirrorConfig>,
    #[serde(default)]
    pub lessons: Vec<PublishedLessonDraft>,
    #[serde(default)]
    pub posts: Vec<PublishedPostDraft>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PublisherEndpointTestRequest {
    pub profile_id: String,
    pub passphrase: String,
    pub relays: Vec<NostrRelayConfig>,
    pub blossom_servers: Vec<BlossomServerConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlossomUploadResult {
    pub server_url: String,
    pub hash: String,
    pub url: Option<String>,
    pub uploaded: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NostrRelayPublishResult {
    pub relay_url: String,
    pub accepted: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMirrorResult {
    pub service: String,
    pub endpoint_url: String,
    pub url: Option<String>,
    pub cid: Option<String>,
    pub archived: bool,
    pub verified: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPublishResult {
    pub channel_id: String,
    pub channel_title: String,
    pub naddr: String,
    pub canonical_channel_link: String,
    pub invite_text: String,
    pub verification_code: String,
    pub manifest_json: String,
    pub manifest_sha256: String,
    pub manifest_url: String,
    pub nostr_event_id: String,
    pub blossom_results: Vec<BlossomUploadResult>,
    pub archive_results: Vec<ArchiveMirrorResult>,
    pub relay_results: Vec<NostrRelayPublishResult>,
    pub media_count: i64,
    pub post_count: i64,
    pub total_item_count: i64,
    pub messages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PublisherEndpointTestReport {
    pub passed: bool,
    pub blossom_results: Vec<BlossomUploadResult>,
    pub relay_results: Vec<NostrRelayPublishResult>,
    pub messages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NostrChannelPreview {
    pub naddr: String,
    pub manifest_url: String,
    pub manifest_sha256: String,
    pub title: String,
    pub curator_display_name: String,
    pub curator_public_key: Option<String>,
    pub trust_state: String,
    pub published_at: Option<String>,
    pub lesson_count: i64,
    pub media_count: i64,
    pub relay_count: i64,
    pub blossom_server_count: i64,
    pub archive_mirror_count: i64,
    pub relays: Vec<String>,
    pub blossom_servers: Vec<String>,
    pub archive_mirrors: Vec<String>,
    pub warnings: Vec<String>,
    pub messages: Vec<String>,
}
