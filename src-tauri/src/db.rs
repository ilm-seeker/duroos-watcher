use crate::{
    manifest, media_tools,
    models::{
        AppSnapshot, ClearSourceSummary, Collection, DownloadSourceSummary, ImportSummary,
        IngestSummary, Job, Lesson, LessonNote, LiveSession, ManifestValidationReport, MediaFile,
        MediaStorageAudit, MediaStorageCleanup, MediaStorageCleanupRequest, MediaStorageStaleItem,
        NativePlaybackResult, OpenMediaResult, ProvenanceRecord, RetrievalRef, RuntimeDiagnostics,
        Source, SourceCapability, Teacher, TeacherRelay, TrustCuratorSummary, TrustedCurator,
        WatchState,
    },
    publisher,
};
use chrono::Utc;
use reqwest::{
    blocking::{Client, Response},
    header::CONTENT_TYPE,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};
use url::Url;
use uuid::Uuid;
use walkdir::WalkDir;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "flv", "mpg", "mpeg", "ts", "m2ts", "mts",
    "vob", "3gp", "3g2",
];
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "m4a", "aac", "wav", "flac", "ogg", "opus", "wma", "aiff", "aif", "amr",
];
const PDF_EXTENSIONS: &[&str] = &["pdf"];
const INGEST_USER_AGENT: &str = "DuroosWatcher/0.1 local-first-study-library";
const LOCAL_PROVENANCE_NOTE: &str = "Imported from local media selected by the user.";
const PUBLIC_SOURCE_PROVENANCE_NOTE: &str =
    "Captured from public source metadata; media download remains user controlled.";
const MIN_PLAUSIBLE_MEDIA_BYTES: u64 = 1024;
const YT_DLP_COOKIE_FILE_NAMES: &[&str] = &["yt-dlp-cookies.txt", "cookies.txt"];
const DEFAULT_SOURCE_IDS: &[&str] = &[
    "source-local-files",
    "source-telegram",
    "source-rss-feed",
    "source-archive-org",
    "source-youtube",
    "source-x",
    "source-rumble",
    "source-odysee",
    "source-teacher-relay",
];

#[derive(Debug)]
struct TelegramSource {
    username: String,
    post_id: Option<String>,
}

#[derive(Debug, Clone)]
struct DiscoveredLesson {
    title: String,
    content_type: String,
    source_url: String,
    retrieval_refs: Vec<RetrievalRef>,
    published_at: Option<String>,
    description: Option<String>,
    duration_seconds: Option<i64>,
    adapter_name: String,
    provenance_note: String,
    content_hash: Option<String>,
}

#[derive(Debug)]
struct ParsedFeed {
    title: String,
    feed_format: String,
    trust_state: String,
    curator: Option<ManifestCurator>,
    lessons: Vec<DiscoveredLesson>,
}

#[derive(Debug)]
struct ManifestCurator {
    id: String,
    display_name: String,
    public_key: String,
}

#[derive(Debug)]
struct SourceContext {
    source_id: String,
    platform: String,
    source_label: String,
    source_identifier: String,
    feed_format: String,
    feed_transport: String,
    trust_state: String,
    trusted_curator_id: Option<String>,
    last_verified_at: Option<String>,
    source_capability: SourceCapability,
    teacher_id: String,
    teacher_label: String,
    teacher_description: String,
    teacher_source_links: Vec<String>,
    collection_id: String,
    collection_title: String,
    collection_owner_label: String,
}

#[derive(Debug)]
struct RefreshableSource {
    id: String,
    platform: String,
    label: String,
    identifier: String,
}

#[derive(Debug)]
struct LocalLessonOrganization {
    title: String,
    teacher_id: String,
    teacher_label: String,
    collection_id: String,
    collection_title: String,
}

#[derive(Debug)]
struct DownloadLesson {
    id: String,
    title: String,
    content_type: String,
    source_url: String,
    retrieval_refs: Vec<RetrievalRef>,
    expected_content_hash: Option<String>,
    has_invalid_media_record: bool,
}

#[derive(Debug)]
struct JobUpdate<'a> {
    id: &'a str,
    kind: &'a str,
    state: &'a str,
    source_id: Option<&'a str>,
    lesson_id: Option<&'a str>,
    label: &'a str,
    detail: &'a str,
}

#[derive(Debug, Clone)]
struct JobProgress {
    started_at: Option<String>,
    completed_at: Option<String>,
    bytes_expected: Option<i64>,
    bytes_downloaded: Option<i64>,
    bytes_per_second: Option<f64>,
    elapsed_ms: Option<i64>,
}

#[derive(Debug)]
struct DirectDownloadProgress<'a> {
    connection: &'a Connection,
    job_id: &'a str,
    source_id: &'a str,
    lesson_id: &'a str,
    label: &'a str,
    started_at: &'a str,
    total_bytes: Option<i64>,
    last_recorded_bytes: i64,
}

struct DownloadMediaContext<'a> {
    data_dir: &'a Path,
    cookies_file: Option<&'a Path>,
    expected_content_hash: Option<&'a str>,
    progress: Option<DirectDownloadProgress<'a>>,
}

#[derive(Debug, Clone)]
struct RetrievalDownloadCandidate {
    url: String,
    label: String,
    file_name: Option<String>,
}

#[derive(Debug, Clone)]
struct YtDlpCommand {
    program: String,
    args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativePlayerCommand {
    name: String,
    program: String,
    args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentTypeEvidence {
    FilePath,
    SourceUrlExtension,
    VideoPage,
    TextSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ContentTypeInference {
    content_type: &'static str,
    evidence: ContentTypeEvidence,
}

pub fn initialize(app: &AppHandle) -> Result<(), String> {
    media_tools::prepare_bundled_media_tool_path(app);

    let data_dir = app_data_dir(app)?;
    fs::create_dir_all(data_dir.join("library/imports")).map_err(|error| error.to_string())?;

    let mut connection = open_connection(app)?;
    run_migrations(&connection)?;
    ensure_default_records(&mut connection)?;
    backfill_missing_video_thumbnails(&connection, &data_dir)?;

    Ok(())
}

pub fn open_connection(app: &AppHandle) -> Result<Connection, String> {
    let data_dir = app_data_dir(app)?;
    fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    Connection::open(data_dir.join("duroos.sqlite3")).map_err(|error| error.to_string())
}

pub fn fetch_snapshot(connection: &Connection) -> Result<AppSnapshot, String> {
    Ok(AppSnapshot {
        sources: fetch_sources(connection)?,
        teachers: fetch_teachers(connection)?,
        teacher_relays: fetch_teacher_relays(connection)?,
        live_sessions: fetch_live_sessions(connection)?,
        collections: fetch_collections(connection)?,
        lessons: fetch_lessons(connection)?,
        media_files: fetch_media_files(connection)?,
        provenance_records: fetch_provenance_records(connection)?,
        watch_state: fetch_watch_state(connection)?,
        lesson_notes: fetch_lesson_notes(connection)?,
        jobs: fetch_jobs(connection)?,
        trusted_curators: fetch_trusted_curators(connection)?,
    })
}

pub fn runtime_diagnostics(app: &AppHandle) -> RuntimeDiagnostics {
    media_tools::prepare_bundled_media_tool_path(app);

    let cookies_file = app_data_dir(app)
        .ok()
        .and_then(|data_dir| yt_dlp_cookie_file(&data_dir));
    let cookies_file_name = cookies_file
        .as_ref()
        .and_then(|path| path.file_name())
        .and_then(|file_name| file_name.to_str())
        .map(str::to_string);
    let native_player = native_player_command_for_app(app);
    let native_player_name = native_player.as_ref().map(|player| player.name.clone());
    let native_player_command = native_player.as_ref().map(native_player_command_label);
    let required_media_tools = media_tools::required_media_tool_status(app);

    match find_yt_dlp_command() {
        Ok(command) => {
            let version = Command::new(&command.program)
                .args(&command.args)
                .arg("--version")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                .filter(|value| !value.is_empty());
            let command_label = if command.args.is_empty() {
                command.program.clone()
            } else {
                format!("{} {}", command.program, command.args.join(" "))
            };

            let mut messages = vec![format!(
                "yt-dlp is available{} via {command_label}.",
                version
                    .as_ref()
                    .map(|value| format!(" ({value})"))
                    .unwrap_or_default()
            )];
            messages.extend(media_tool_messages(&required_media_tools));

            if let Some(file_name) = cookies_file_name.as_deref() {
                messages.push(format!(
                    "Local downloader cookies are configured through {file_name}."
                ));
            } else {
                messages.push(
                    "For credential-bound platforms, place yt-dlp-cookies.txt in the app data directory."
                        .to_string(),
                );
            }
            if let Some(player_name) = native_player_name.as_deref() {
                messages.push(format!(
                    "Native playback is available through {player_name}."
                ));
            } else {
                messages.push(
                    "Native playback needs VLC, mpv, or ffplay available locally or bundled with the app."
                        .to_string(),
                );
            }

            RuntimeDiagnostics {
                desktop_runtime_available: true,
                yt_dlp_available: true,
                yt_dlp_version: version.clone(),
                yt_dlp_command: Some(command_label.clone()),
                required_media_tools_available: required_media_tools.available,
                media_tool_source: required_media_tools.source,
                missing_media_tools: required_media_tools.missing,
                native_playback_available: native_player.is_some(),
                native_playback_player: native_player_name,
                native_playback_command: native_player_command,
                yt_dlp_cookies_configured: cookies_file.is_some(),
                yt_dlp_cookies_file: cookies_file_name,
                messages,
            }
        }
        Err(error) => {
            let mut messages = vec![error];
            messages.extend(media_tool_messages(&required_media_tools));
            if let Some(player_name) = native_player_name.as_deref() {
                messages.push(format!(
                    "Native playback is available through {player_name}."
                ));
            } else {
                messages.push(
                    "Native playback needs VLC, mpv, or ffplay available locally or bundled with the app."
                        .to_string(),
                );
            }

            RuntimeDiagnostics {
                desktop_runtime_available: true,
                yt_dlp_available: false,
                yt_dlp_version: None,
                yt_dlp_command: None,
                required_media_tools_available: required_media_tools.available,
                media_tool_source: required_media_tools.source,
                missing_media_tools: required_media_tools.missing,
                native_playback_available: native_player.is_some(),
                native_playback_player: native_player_name,
                native_playback_command: native_player_command,
                yt_dlp_cookies_configured: cookies_file.is_some(),
                yt_dlp_cookies_file: cookies_file_name,
                messages,
            }
        }
    }
}

fn media_tool_messages(status: &media_tools::RequiredMediaToolStatus) -> Vec<String> {
    if status.available {
        return vec![format!(
            "Required media tools are available from {} sources.",
            status.source
        )];
    }

    vec![format!(
        "Required media tools missing: {}.",
        status.missing.join(", ")
    )]
}

pub fn validate_collection_manifest(
    connection: &Connection,
    manifest_json: &str,
) -> Result<ManifestValidationReport, String> {
    let mut report = manifest::validate_collection_manifest(manifest_json);

    if report.valid && report.trust_state.as_deref() == Some("signed-untrusted") {
        if let Some(curator) = report.curator.as_ref() {
            if let Some(trusted_curator_id) =
                trusted_curator_id_for_public_key(connection, &curator.public_key)?
            {
                report.trust_state = Some("signed-trusted".to_string());
                report.trusted_curator_id = Some(trusted_curator_id);
            }
        }
    }

    Ok(report)
}

pub fn add_trusted_curator(
    connection: &mut Connection,
    display_name: String,
    public_key: String,
    trust_note: Option<String>,
) -> Result<TrustedCurator, String> {
    let display_name = display_name.trim();
    if display_name.is_empty() {
        return Err("Trusted curator display name is required.".to_string());
    }

    let public_key = public_key.trim();
    if public_key.is_empty() {
        return Err("Trusted curator public key is required.".to_string());
    }
    manifest::validate_ed25519_public_key(public_key)
        .map_err(|_| "Trusted curator public key must be an Ed25519 public key.".to_string())?;

    let trust_note = trust_note
        .map(|note| note.trim().to_string())
        .filter(|note| !note.is_empty());
    let curator_id = format!("trusted-curator-{}", stable_suffix(public_key));
    let added_at = Utc::now().to_rfc3339();
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;

    transaction
        .execute(
            "INSERT INTO trusted_curators (id, display_name, public_key, trust_note, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(public_key) DO UPDATE SET
               display_name = excluded.display_name,
               trust_note = excluded.trust_note",
            params![
                curator_id,
                display_name,
                public_key,
                trust_note.as_deref(),
                added_at
            ],
        )
        .map_err(|error| error.to_string())?;

    let curator = trusted_curator_for_public_key(&transaction, public_key)?
        .ok_or_else(|| "Trusted curator could not be saved.".to_string())?;
    promote_sources_for_trusted_curator(&transaction, &curator.id, &curator.public_key)?;
    transaction.commit().map_err(|error| error.to_string())?;

    Ok(curator)
}

pub fn remove_trusted_curator(
    connection: &mut Connection,
    curator_id: String,
) -> Result<TrustCuratorSummary, String> {
    let curator_id = curator_id.trim();
    if curator_id.is_empty() {
        return Err("Trusted curator id is required.".to_string());
    }

    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let curator = trusted_curator_for_id(&transaction, curator_id)?
        .ok_or_else(|| "Trusted curator was not found.".to_string())?;
    let sources_updated =
        downgrade_sources_for_trusted_curator(&transaction, &curator.id, &curator.public_key)?;

    transaction
        .execute(
            "DELETE FROM trusted_curators WHERE id = ?1",
            params![&curator.id],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;

    Ok(TrustCuratorSummary {
        curator_id: curator.id,
        display_name: curator.display_name.clone(),
        public_key: curator.public_key.clone(),
        sources_updated,
        messages: vec![format!(
            "Removed {} from trusted curators. {} signed source(s) are now untrusted.",
            curator.display_name, sources_updated
        )],
    })
}

pub fn search_lessons(connection: &Connection, query: &str) -> Result<Vec<Lesson>, String> {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        return fetch_lessons(connection);
    }

    let fts_query = trimmed
        .split_whitespace()
        .map(|token| {
            token
                .chars()
                .filter(|character| character.is_alphanumeric())
                .collect::<String>()
        })
        .filter(|token| !token.is_empty())
        .map(|token| format!("{token}*"))
        .collect::<Vec<_>>()
        .join(" ");

    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let mut statement = connection
        .prepare(
            "SELECT l.id, l.title, l.content_type, l.teacher_id, l.collection_id, l.source_id,
                    l.source_url, l.retrieval_refs_json, l.published_at, l.description,
                    l.thumbnail_tone, l.duration_seconds, l.media_file_id, l.provenance_id
             FROM lessons_fts f
             JOIN lessons l ON l.id = f.lesson_id
             WHERE lessons_fts MATCH ?
             ORDER BY rank",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map(params![fts_query], lesson_from_row)
        .map_err(|error| error.to_string())?;
    let lessons = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(lessons)
}

pub fn save_watch_state(
    connection: &Connection,
    lesson_id: String,
    progress_seconds: i64,
    duration_seconds: Option<i64>,
    completed: bool,
) -> Result<WatchState, String> {
    let lesson_id = lesson_id.trim();
    if lesson_id.is_empty() {
        return Err("Lesson id is required.".to_string());
    }

    let existing_duration: Option<i64> = connection
        .query_row(
            "SELECT duration_seconds FROM lessons WHERE id = ?1",
            params![lesson_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Lesson was not found.".to_string())?;
    let normalized_duration = duration_seconds.filter(|seconds| *seconds > 0);
    if let Some(duration) = normalized_duration {
        connection
            .execute(
                "UPDATE lessons SET duration_seconds = ?1 WHERE id = ?2",
                params![duration, lesson_id],
            )
            .map_err(|error| error.to_string())?;
    }

    let effective_duration = normalized_duration.or(existing_duration);
    let progress_seconds = progress_seconds.max(0);
    let completed = completed
        || effective_duration
            .map(|duration| duration > 0 && progress_seconds.saturating_mul(100) >= duration * 95)
            .unwrap_or(false);
    let stored_progress = if completed {
        effective_duration
            .filter(|duration| *duration > 0)
            .unwrap_or(progress_seconds)
            .max(progress_seconds)
    } else {
        progress_seconds
    };
    let now = Utc::now().to_rfc3339();

    connection
        .execute(
            "INSERT INTO watch_state (lesson_id, progress_seconds, completed, last_watched_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(lesson_id) DO UPDATE SET
               progress_seconds = excluded.progress_seconds,
               completed = excluded.completed,
               last_watched_at = excluded.last_watched_at",
            params![
                lesson_id,
                stored_progress,
                if completed { 1 } else { 0 },
                now
            ],
        )
        .map_err(|error| error.to_string())?;

    watch_state_for_lesson(connection, lesson_id)
}

pub fn save_lesson_note(
    connection: &Connection,
    lesson_id: String,
    body: String,
) -> Result<LessonNote, String> {
    let lesson_id = lesson_id.trim();
    if lesson_id.is_empty() {
        return Err("Lesson id is required.".to_string());
    }

    let exists: Option<String> = connection
        .query_row(
            "SELECT id FROM lessons WHERE id = ?1",
            params![lesson_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if exists.is_none() {
        return Err("Lesson was not found.".to_string());
    }

    let body = body.trim().to_string();
    let now = Utc::now().to_rfc3339();
    if body.is_empty() {
        connection
            .execute(
                "DELETE FROM lesson_notes WHERE lesson_id = ?1",
                params![lesson_id],
            )
            .map_err(|error| error.to_string())?;
        return Ok(LessonNote {
            lesson_id: lesson_id.to_string(),
            body,
            updated_at: now,
        });
    }

    connection
        .execute(
            "INSERT INTO lesson_notes (lesson_id, body, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(lesson_id) DO UPDATE SET
               body = excluded.body,
               updated_at = excluded.updated_at",
            params![lesson_id, body, now],
        )
        .map_err(|error| error.to_string())?;

    lesson_note_for_lesson(connection, lesson_id)
}

pub fn update_lesson_organization(
    connection: &mut Connection,
    lesson_id: String,
    teacher_display_name: String,
    collection_title: String,
) -> Result<Lesson, String> {
    let lesson_id = lesson_id.trim();
    if lesson_id.is_empty() {
        return Err("Lesson id is required.".to_string());
    }

    let teacher_label = teacher_display_name.trim();
    if teacher_label.is_empty() {
        return Err("Teacher name is required.".to_string());
    }

    let collection_title = collection_title.trim();
    if collection_title.is_empty() {
        return Err("Collection title is required.".to_string());
    }

    let lesson_record: Option<(String, String, String, String, String)> = connection
        .query_row(
            "SELECT l.title, COALESCE(l.description, ''), l.source_id, l.teacher_id, l.collection_id
             FROM lessons l
             WHERE l.id = ?1",
            params![lesson_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (title, description, source_id, old_teacher_id, old_collection_id) =
        lesson_record.ok_or_else(|| "Lesson was not found.".to_string())?;
    let source_label: String = connection
        .query_row(
            "SELECT label FROM sources WHERE id = ?1",
            params![&source_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let teacher_id = existing_teacher_id_for_label(&transaction, teacher_label)?
        .unwrap_or_else(|| user_teacher_id(teacher_label));
    let collection_id =
        existing_collection_id_for_title(&transaction, collection_title, &source_id)?
            .unwrap_or_else(|| user_collection_id(collection_title));

    if teacher_id.starts_with("teacher-user-") {
        upsert_user_teacher(&transaction, &teacher_id, teacher_label)?;
    }
    if collection_id.starts_with("collection-user-") {
        upsert_user_collection(&transaction, &collection_id, collection_title, &source_id)?;
    } else {
        ensure_collection_source_membership(&transaction, &collection_id, &source_id)?;
    }
    transaction
        .execute(
            "UPDATE lessons
             SET teacher_id = ?1, collection_id = ?2
             WHERE id = ?3",
            params![&teacher_id, &collection_id, lesson_id],
        )
        .map_err(|error| error.to_string())?;
    refresh_lesson_fts(
        &transaction,
        lesson_id,
        &title,
        &description,
        teacher_label,
        collection_title,
        &source_label,
    )?;
    refresh_collection_count(&transaction, &old_collection_id)?;
    refresh_collection_count(&transaction, &collection_id)?;
    cleanup_empty_collection(&transaction, &old_collection_id)?;
    cleanup_empty_teacher(&transaction, &old_teacher_id)?;

    transaction.commit().map_err(|error| error.to_string())?;

    lesson_for_id(connection, lesson_id)
}

pub fn play_media_file_native(
    app: &AppHandle,
    media_file_id: String,
) -> Result<NativePlaybackResult, String> {
    let media_file_id = media_file_id.trim();

    if media_file_id.is_empty() {
        return Err("Media file id is required.".to_string());
    }

    let data_dir = app_data_dir(app)?;
    let connection = open_connection(app)?;
    let record: Option<(String, String, String, String, String)> = connection
        .query_row(
            "SELECT m.id, m.relative_path, l.id, l.title, l.content_type
             FROM media_files m
             JOIN lessons l ON l.id = m.lesson_id
             WHERE m.id = ?1",
            params![media_file_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (media_file_id, relative_path, lesson_id, title, content_type) =
        record.ok_or_else(|| "Media file was not found.".to_string())?;

    if !matches!(content_type.as_str(), "video" | "audio") {
        return Err("Native playback is available for video and audio files.".to_string());
    }

    let media_path = resolve_library_media_path(&data_dir, &relative_path)
        .ok_or_else(|| "Media file path is outside the app library.".to_string())?;
    if !media_path.is_file() {
        return Err("Media file is missing from disk. Re-download this lesson.".to_string());
    }
    validate_downloaded_media_file(&media_path, &content_type)?;

    let player = native_player_command_for_app(app).ok_or_else(|| {
        "Native playback is unavailable. Install VLC, mpv, or ffplay, or bundle one of them with the app."
            .to_string()
    })?;
    let command_label = native_player_command_label(&player);
    spawn_native_player_checked(&player, &media_path)?;

    Ok(NativePlaybackResult {
        media_file_id,
        lesson_id,
        title: title.clone(),
        player_name: player.name.clone(),
        command_label,
        launched: true,
        messages: vec![format!("Opened \"{title}\" in {}.", player.name)],
    })
}

pub fn open_pdf_file(app: &AppHandle, media_file_id: String) -> Result<OpenMediaResult, String> {
    let (media_file_id, media_path, lesson_id, title) =
        resolve_validated_pdf_record(app, &media_file_id)?;

    tauri_plugin_opener::open_path(&media_path, None::<&str>)
        .map_err(|error| format!("Could not open PDF in the system viewer: {error}"))?;

    Ok(OpenMediaResult {
        media_file_id,
        lesson_id,
        title: title.clone(),
        opened: true,
        messages: vec![format!("Opened \"{title}\" in the system PDF viewer.")],
    })
}

fn resolve_validated_pdf_record(
    app: &AppHandle,
    media_file_id: &str,
) -> Result<(String, PathBuf, String, String), String> {
    let media_file_id = media_file_id.trim();

    if media_file_id.is_empty() {
        return Err("Media file id is required.".to_string());
    }

    let data_dir = app_data_dir(app)?;
    let connection = open_connection(app)?;
    resolve_validated_pdf_record_from_connection(&connection, &data_dir, media_file_id)
}

fn resolve_validated_pdf_record_from_connection(
    connection: &Connection,
    data_dir: &Path,
    media_file_id: &str,
) -> Result<(String, PathBuf, String, String), String> {
    let record: Option<(String, String, String, String, String)> = connection
        .query_row(
            "SELECT m.id, m.relative_path, l.id, l.title, l.content_type
             FROM media_files m
             JOIN lessons l ON l.id = m.lesson_id
             WHERE m.id = ?1",
            params![media_file_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (media_file_id, relative_path, lesson_id, title, content_type) =
        record.ok_or_else(|| "Media file was not found.".to_string())?;

    if content_type != "pdf" {
        return Err("Open PDF is available only for PDF lessons.".to_string());
    }

    let media_path = resolve_library_media_path(data_dir, &relative_path)
        .ok_or_else(|| "Media file path is outside the app library.".to_string())?;
    if !media_path.is_file() {
        return Err("PDF file is missing from disk. Re-download this lesson.".to_string());
    }
    validate_downloaded_media_file(&media_path, &content_type)?;

    Ok((media_file_id, media_path, lesson_id, title))
}

pub fn resolve_media_file_path(app: &AppHandle, media_file_id: String) -> Result<String, String> {
    let media_file_id = media_file_id.trim();

    if media_file_id.is_empty() {
        return Err("Media file id is required.".to_string());
    }

    let data_dir = app_data_dir(app)?;
    let connection = open_connection(app)?;
    let media_record: Option<(String, String)> = connection
        .query_row(
            "SELECT m.relative_path, l.content_type
             FROM media_files m
             JOIN lessons l ON l.id = m.lesson_id
             WHERE m.id = ?1",
            params![media_file_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (relative_path, content_type) =
        media_record.ok_or_else(|| "Media file was not found.".to_string())?;
    let media_path = resolve_library_media_path(&data_dir, &relative_path)
        .ok_or_else(|| "Media file path is outside the app library.".to_string())?;

    if !media_path.is_file() {
        return Err("Media file is missing from disk. Re-download this lesson.".to_string());
    }
    validate_downloaded_media_file(&media_path, &content_type)
        .map_err(|error| format!("{error} Re-download this lesson."))?;

    Ok(media_path.to_string_lossy().to_string())
}

pub fn resolve_media_thumbnail_path(
    app: &AppHandle,
    media_file_id: String,
) -> Result<String, String> {
    let media_file_id = media_file_id.trim();

    if media_file_id.is_empty() {
        return Err("Media file id is required.".to_string());
    }

    let data_dir = app_data_dir(app)?;
    let connection = open_connection(app)?;
    let media_record: Option<(Option<String>, String, String)> = connection
        .query_row(
            "SELECT m.thumbnail_relative_path, m.relative_path, l.content_type
             FROM media_files m
             JOIN lessons l ON l.id = m.lesson_id
             WHERE m.id = ?1",
            params![media_file_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (thumbnail_relative_path, relative_path, content_type) =
        media_record.ok_or_else(|| "Media file was not found.".to_string())?;

    if content_type != "video" {
        return Err("Cover previews are only available for video media.".to_string());
    }

    if let Some(thumbnail_path) = thumbnail_relative_path
        .as_deref()
        .and_then(|path| resolve_library_media_path(&data_dir, path))
        .filter(|path| path.is_file())
    {
        return Ok(thumbnail_path.to_string_lossy().to_string());
    }

    let media_path = resolve_library_media_path(&data_dir, &relative_path)
        .ok_or_else(|| "Media file path is outside the app library.".to_string())?;
    if !media_path.is_file() {
        return Err("Media file is missing from disk. Re-download this lesson.".to_string());
    }

    let generated_thumbnail =
        generate_video_thumbnail(&data_dir, &media_path, media_file_id, &content_type)?
            .ok_or_else(|| "Cover preview is not available for this video.".to_string())?;
    connection
        .execute(
            "UPDATE media_files SET thumbnail_relative_path = ?1 WHERE id = ?2",
            params![&generated_thumbnail, media_file_id],
        )
        .map_err(|error| error.to_string())?;
    let thumbnail_path = resolve_library_media_path(&data_dir, &generated_thumbnail)
        .ok_or_else(|| "Cover preview path is outside the app library.".to_string())?;

    Ok(thumbnail_path.to_string_lossy().to_string())
}

pub fn import_local_files(app: &AppHandle, paths: Vec<String>) -> Result<ImportSummary, String> {
    let data_dir = app_data_dir(app)?;
    let library_dir = data_dir.join("library/imports");
    fs::create_dir_all(&library_dir).map_err(|error| error.to_string())?;

    let mut imported: i64 = 0;
    let mut skipped: i64 = 0;
    let mut failed: i64 = 0;
    let mut messages = Vec::new();
    let mut connection = open_connection(app)?;

    let import_candidates = paths
        .iter()
        .flat_map(|path| collect_media_files(Path::new(path)))
        .collect::<Vec<_>>();

    if import_candidates.is_empty() {
        return Ok(ImportSummary {
            imported,
            skipped: paths.len() as i64,
            failed,
            messages: vec!["No supported video, audio, or PDF files were provided.".to_string()],
        });
    }

    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;

    for source_path in import_candidates {
        let organization = infer_local_lesson_organization(&source_path);
        let title = organization.title.clone();
        let source_hash = match hash_file(&source_path) {
            Ok(hash) => format!("sha256:{hash}"),
            Err(error) => {
                failed += 1;
                messages.push(format!("{}: {error}", source_path.display()));
                continue;
            }
        };

        if let Some(existing_title) = duplicate_lesson_title_for_hash(&transaction, &source_hash)? {
            skipped += 1;
            messages.push(format!(
                "Skipped duplicate \"{title}\"; already saved as \"{existing_title}\"."
            ));
            continue;
        }

        match copy_media_into_library(&source_path, &library_dir) {
            Ok((relative_path, content_hash, size_bytes)) => {
                let lesson_id = format!("lesson-{}", Uuid::new_v4());
                let media_file_id = format!("media-{}", Uuid::new_v4());
                let provenance_id = format!("prov-{}", Uuid::new_v4());
                let content_type = content_type_from_path(&source_path);
                let media_path = resolve_library_media_path(&data_dir, &relative_path)
                    .ok_or_else(|| "Imported media path is outside the app library.".to_string())?;
                let playback_profile = playback_profile_for_media_file(&media_path, &content_type)?;
                let thumbnail_relative_path =
                    generate_video_thumbnail(&data_dir, &media_path, &media_file_id, &content_type)
                        .ok()
                        .flatten();
                let imported_at = Utc::now().to_rfc3339();
                let origin_url = format!(
                    "local-import://{}",
                    source_path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("unknown")
                );

                upsert_user_teacher(
                    &transaction,
                    &organization.teacher_id,
                    &organization.teacher_label,
                )?;
                upsert_user_collection(
                    &transaction,
                    &organization.collection_id,
                    &organization.collection_title,
                    "source-local-files",
                )?;
                transaction
                    .execute(
                        "INSERT INTO lessons
                         (id, title, content_type, teacher_id, collection_id, source_id, source_url,
                          published_at, description, thumbnail_tone, duration_seconds,
                          media_file_id, provenance_id)
                         VALUES (?1, ?2, ?3, ?4, ?5, 'source-local-files',
                          ?6, ?7, 'Imported local study file', 'emerald', NULL, ?8, ?9)",
                        params![
                            lesson_id,
                            title,
                            content_type,
                            organization.teacher_id,
                            organization.collection_id,
                            origin_url,
                            imported_at,
                            media_file_id,
                            provenance_id
                        ],
                    )
                    .map_err(|error| error.to_string())?;

                transaction
                    .execute(
                        "INSERT INTO media_files
                         (id, lesson_id, relative_path, thumbnail_relative_path, content_hash,
                          size_bytes, codec, import_status, hash_verification_state)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'ready', 'matched')",
                        params![
                            media_file_id,
                            lesson_id,
                            relative_path,
                            thumbnail_relative_path,
                            content_hash,
                            size_bytes,
                            playback_profile
                        ],
                    )
                    .map_err(|error| error.to_string())?;

                transaction
                    .execute(
                        "INSERT INTO provenance_records
                         (id, lesson_id, origin_url, permission_note, imported_at, adapter_name, content_hash)
                         VALUES (?1, ?2, ?3, ?4, ?5, 'LocalFilesAdapter', ?6)",
                        params![
                            provenance_id,
                            lesson_id,
                            origin_url,
                            LOCAL_PROVENANCE_NOTE,
                            imported_at,
                            content_hash
                        ],
                    )
                    .map_err(|error| error.to_string())?;

                transaction
                    .execute(
                        "INSERT INTO lessons_fts
                         (lesson_id, title, description, teacher, collection_title, source_label)
                         VALUES (?1, ?2, 'Imported local study file', ?3, ?4, 'Local Files')",
                        params![
                            lesson_id,
                            title,
                            organization.teacher_label,
                            organization.collection_title
                        ],
                    )
                    .map_err(|error| error.to_string())?;

                refresh_collection_count(&transaction, &organization.collection_id)?;
                imported += 1;
            }
            Err(error) => {
                failed += 1;
                messages.push(format!("{}: {error}", source_path.display()));
            }
        }
    }

    transaction.commit().map_err(|error| error.to_string())?;

    if messages.is_empty() {
        messages.push(format!(
            "{imported} study file(s) imported into the app library."
        ));
    } else if imported > 0 || skipped > 0 {
        messages.insert(
            0,
            format!("{imported} study file(s) imported; {skipped} duplicate(s) skipped."),
        );
    }

    Ok(ImportSummary {
        imported,
        skipped,
        failed,
        messages,
    })
}

pub fn ingest_source_url(app: &AppHandle, source_url: String) -> Result<IngestSummary, String> {
    let normalized_input = normalize_source_input(&source_url)?;
    let mut connection = open_connection(app)?;
    let client = Client::builder()
        .user_agent(INGEST_USER_AGENT)
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|error| error.to_string())?;

    if let Some(telegram_source) = parse_telegram_source(&normalized_input) {
        return ingest_public_telegram(
            &mut connection,
            &client,
            &normalized_input,
            telegram_source,
        );
    }

    if let Some(identifier) = parse_archive_org_identifier(&normalized_input) {
        return ingest_archive_org_item(&mut connection, &client, &normalized_input, &identifier);
    }

    if let Some(summary) = ingest_direct_source_url(&mut connection, &client, &normalized_input)? {
        return Ok(summary);
    }

    if is_nostr_reference(&normalized_input) {
        match publisher::resolve_nostr_channel_manifest_url(&normalized_input) {
            Ok(resolved) => {
                let mut summary = ingest_feed_url(
                    &mut connection,
                    &client,
                    &normalized_input,
                    &resolved.manifest_url,
                )?;
                summary.source_url = normalized_input;
                let resolution_message = if resolved.used_rescue_fallback {
                    format!(
                        "Resolved Nostr rescue invite {} to signed manifest {} across {} verified fallback URL(s).",
                        resolved.naddr,
                        resolved.manifest_sha256,
                        resolved.manifest_urls.len()
                    )
                } else {
                    format!(
                        "Resolved Nostr channel {} to signed manifest {} across {} advertised manifest mirror(s).",
                        resolved.naddr,
                        resolved.manifest_sha256,
                        resolved.manifest_urls.len()
                    )
                };
                summary.messages.insert(0, resolution_message);
                return Ok(summary);
            }
            Err(error) => {
                let detail = format!("Could not resolve Nostr channel: {error}");
                record_standalone_job(&mut connection, &normalized_input, "unsupported", &detail)?;
                return Ok(IngestSummary {
                    source_url: normalized_input,
                    discovered: 0,
                    imported: 0,
                    skipped: 0,
                    failed: 1,
                    messages: vec![detail],
                });
            }
        }
    }

    let feed_url = normalize_feed_url(&normalized_input);
    match ingest_feed_url(&mut connection, &client, &normalized_input, &feed_url) {
        Ok(summary) => Ok(summary),
        Err(error) => {
            let detail = feed_ingest_error_detail(&normalized_input, &error);
            record_standalone_job(&mut connection, &normalized_input, "unsupported", &detail)?;
            Ok(IngestSummary {
                source_url: normalized_input,
                discovered: 0,
                imported: 0,
                skipped: 0,
                failed: 1,
                messages: vec![detail],
            })
        }
    }
}

pub fn refresh_source(app: &AppHandle, source_id: String) -> Result<IngestSummary, String> {
    let source_id = source_id.trim();

    if source_id.is_empty() {
        return Err("Source id is required.".to_string());
    }

    let mut connection = open_connection(app)?;
    let source = refreshable_source(&connection, source_id)?
        .ok_or_else(|| "Source was not found.".to_string())?;

    if is_nostr_reference(&source.identifier) {
        return ingest_source_url(app, source.identifier);
    }

    if !(source.identifier.starts_with("http://") || source.identifier.starts_with("https://")) {
        let detail = format!("{} is not a refreshable remote source.", source.label);
        record_source_refresh_job(&connection, &source.id, &source.label, "skipped", &detail)?;
        mark_source_checked(&connection, &source.id)?;
        return Ok(IngestSummary {
            source_url: source.identifier,
            discovered: 0,
            imported: 0,
            skipped: 0,
            failed: 0,
            messages: vec![detail],
        });
    }

    let client = Client::builder()
        .user_agent(INGEST_USER_AGENT)
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|error| error.to_string())?;

    if source.platform == "youtube" {
        return refresh_youtube_source(&mut connection, &client, &source);
    }

    ingest_source_url(app, source.identifier)
}

fn feed_ingest_error_detail(source_url: &str, error: &str) -> String {
    if is_probably_telegram_invite(source_url) {
        return "Private Telegram links cannot be scraped without a Telegram session. Export media manually or connect a local session when that adapter is added.".to_string();
    }

    if is_youtube_feed_source_url(source_url) {
        return format!(
            "{error} YouTube playlist and channel imports require a public playlist_id or channel_id that YouTube exposes through its RSS feed."
        );
    }

    format!(
        "{error} Supported no-login inputs today: Archive.org item URLs, custom RSS/Atom feeds, public t.me channel URLs, YouTube channel_id feeds, YouTube playlist feeds, direct YouTube/Rumble/Odysee video URLs, and Nostr naddr channel links."
    )
}

pub fn clear_source_content(
    app: &AppHandle,
    source_id: String,
    remove_source: bool,
) -> Result<ClearSourceSummary, String> {
    let source_id = source_id.trim().to_string();

    if source_id.is_empty() {
        return Err("Source id is required.".to_string());
    }

    let data_dir = app_data_dir(app)?;
    let mut connection = open_connection(app)?;
    let source_label: Option<String> = connection
        .query_row(
            "SELECT label FROM sources WHERE id = ?1",
            params![&source_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let source_label = source_label.ok_or_else(|| "Source was not found.".to_string())?;
    let source_is_default = is_default_source_id(&source_id);
    let should_remove_source = remove_source && !source_is_default;
    let affected_collections = collect_source_column(&connection, "collection_id", &source_id)?;
    let affected_teachers = collect_source_column(&connection, "teacher_id", &source_id)?;
    let media_paths = collect_source_media_paths(&connection, &source_id)?;
    let lessons_removed: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM lessons WHERE source_id = ?1",
            params![&source_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let media_files_removed = media_paths.len() as i64;
    let mut messages = Vec::new();

    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;

    transaction
        .execute(
            "DELETE FROM lessons_fts
             WHERE lesson_id IN (SELECT id FROM lessons WHERE source_id = ?1)",
            params![&source_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM watch_state
             WHERE lesson_id IN (SELECT id FROM lessons WHERE source_id = ?1)",
            params![&source_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM lesson_notes
             WHERE lesson_id IN (SELECT id FROM lessons WHERE source_id = ?1)",
            params![&source_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM media_files
             WHERE lesson_id IN (SELECT id FROM lessons WHERE source_id = ?1)",
            params![&source_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM provenance_records
             WHERE lesson_id IN (SELECT id FROM lessons WHERE source_id = ?1)",
            params![&source_id],
        )
        .map_err(|error| error.to_string())?;
    let jobs_removed = transaction
        .execute(
            "DELETE FROM jobs
             WHERE source_id = ?1
                OR lesson_id IN (SELECT id FROM lessons WHERE source_id = ?1)",
            params![&source_id],
        )
        .map_err(|error| error.to_string())? as i64;
    transaction
        .execute(
            "DELETE FROM teacher_relays WHERE id = ?1",
            params![format!("relay-{source_id}")],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM lessons WHERE source_id = ?1",
            params![&source_id],
        )
        .map_err(|error| error.to_string())?;

    for collection_id in &affected_collections {
        let remaining_lessons: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM lessons WHERE collection_id = ?1",
                params![collection_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;

        if remaining_lessons == 0
            && collection_owned_by_source(&transaction, collection_id, &source_id)?
        {
            transaction
                .execute(
                    "DELETE FROM collections WHERE id = ?1 AND id <> 'collection-2'",
                    params![collection_id],
                )
                .map_err(|error| error.to_string())?;
        } else {
            transaction
                .execute(
                    "UPDATE collections
                     SET lesson_count = (SELECT COUNT(*) FROM lessons WHERE collection_id = ?1)
                     WHERE id = ?1",
                    params![collection_id],
                )
                .map_err(|error| error.to_string())?;
        }
    }

    for teacher_id in &affected_teachers {
        let remaining_lessons: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM lessons WHERE teacher_id = ?1",
                params![teacher_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;

        if remaining_lessons == 0 && teacher_id != "teacher-3" {
            transaction
                .execute("DELETE FROM teachers WHERE id = ?1", params![teacher_id])
                .map_err(|error| error.to_string())?;
        }
    }

    if should_remove_source {
        transaction
            .execute("DELETE FROM sources WHERE id = ?1", params![&source_id])
            .map_err(|error| error.to_string())?;
    } else {
        transaction
            .execute(
                "UPDATE sources SET last_checked_at = NULL WHERE id = ?1",
                params![&source_id],
            )
            .map_err(|error| error.to_string())?;
    }

    transaction.commit().map_err(|error| error.to_string())?;

    let failed_file_deletes = remove_media_files_from_disk(&data_dir, &media_paths);

    if should_remove_source {
        messages.push(format!(
            "Removed source and cleared {lessons_removed} lesson(s)."
        ));
    } else {
        messages.push(format!(
            "Cleared {lessons_removed} lesson(s) from {source_label}."
        ));
    }

    if media_files_removed > 0 {
        messages.push(format!(
            "Removed {media_files_removed} media file record(s) from the library."
        ));
    }

    if failed_file_deletes > 0 {
        messages.push(format!(
            "{failed_file_deletes} copied media file(s) could not be removed from disk."
        ));
    }

    Ok(ClearSourceSummary {
        source_id,
        source_label,
        removed_source: should_remove_source,
        lessons_removed,
        media_files_removed,
        jobs_removed,
        messages,
    })
}

pub fn audit_media_storage(app: &AppHandle) -> Result<MediaStorageAudit, String> {
    let data_dir = app_data_dir(app)?;
    let connection = open_connection(app)?;
    Ok(media_storage_audit_detail(&connection, &data_dir)?.audit)
}

pub fn cleanup_media_storage(
    app: &AppHandle,
    request: MediaStorageCleanupRequest,
) -> Result<MediaStorageCleanup, String> {
    let cleanup_mode = validate_media_cleanup_mode(&request.mode)?;
    let data_dir = app_data_dir(app)?;
    let connection = open_connection(app)?;
    let before = media_storage_audit_detail(&connection, &data_dir)?;
    let mut removed_files = 0_i64;
    let mut failed_removals = 0_i64;
    let mut reclaimed_bytes = 0_i64;

    for stale_file in &before.stale_files {
        if !cleanup_mode_matches(&cleanup_mode, &stale_file.category) {
            continue;
        }
        if !stale_file.path.is_file() {
            continue;
        }
        match fs::remove_file(&stale_file.path) {
            Ok(()) => {
                removed_files += 1;
                reclaimed_bytes += stale_file.size_bytes;
            }
            Err(_) => {
                failed_removals += 1;
            }
        }
    }

    let audit = media_storage_audit_detail(&connection, &data_dir)?.audit;
    let mut messages = vec![format!(
        "Removed {removed_files} {} stale library file(s), reclaiming {}.",
        cleanup_mode_label(&cleanup_mode),
        format_bytes(reclaimed_bytes)
    )];
    if failed_removals > 0 {
        messages.push(format!(
            "{failed_removals} stale file(s) could not be removed; check file permissions and retry."
        ));
    }
    if audit.stale_files > 0 {
        messages.push(format!(
            "{} stale file(s) remain in the app library.",
            audit.stale_files
        ));
    }

    Ok(MediaStorageCleanup {
        audit,
        mode: cleanup_mode,
        removed_files,
        failed_removals,
        reclaimed_bytes,
        messages,
    })
}

pub fn download_source_media(
    app: &AppHandle,
    source_id: String,
) -> Result<DownloadSourceSummary, String> {
    let source_id = source_id.trim().to_string();

    if source_id.is_empty() {
        return Err("Source id is required.".to_string());
    }

    let data_dir = app_data_dir(app)?;
    let download_root = data_dir
        .join("library")
        .join("downloads")
        .join(safe_path_segment(&source_id));
    fs::create_dir_all(&download_root).map_err(|error| error.to_string())?;

    let mut connection = open_connection(app)?;
    let source_label: Option<String> = connection
        .query_row(
            "SELECT label FROM sources WHERE id = ?1",
            params![&source_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let source_label = source_label.ok_or_else(|| "Source was not found.".to_string())?;
    let (lessons, skipped) = source_download_plan(&connection, &data_dir, &source_id)?;
    let attempted = lessons.len() as i64;
    let source_job_id = format!("job-download-source-{}", stable_suffix(&source_id));
    let mut downloaded = 0;
    let mut failed = 0;
    let mut total_downloaded_bytes = 0_i64;
    let mut messages = Vec::new();

    if lessons.is_empty() {
        let detail = format!(
            "No missing or invalid video, audio, or PDF files. {skipped} item(s) already have playable local files or saved post content."
        );
        upsert_job(
            &connection,
            JobUpdate {
                id: &source_job_id,
                kind: "download",
                state: "skipped",
                source_id: Some(&source_id),
                lesson_id: None,
                label: &format!("Download media: {source_label}"),
                detail: &detail,
            },
        )?;

        return Ok(DownloadSourceSummary {
            source_id,
            source_label,
            attempted,
            downloaded,
            skipped,
            failed,
            messages: vec![detail],
        });
    }

    let source_started_at = Utc::now().to_rfc3339();
    let source_started_instant = Instant::now();
    upsert_job_with_progress(
        &connection,
        JobUpdate {
            id: &source_job_id,
            kind: "download",
            state: "running",
            source_id: Some(&source_id),
            lesson_id: None,
            label: &format!("Download media: {source_label}"),
            detail: &format!(
                "Starting download of {attempted} missing or invalid file-backed item(s)."
            ),
        },
        Some(JobProgress {
            started_at: Some(source_started_at.clone()),
            completed_at: None,
            bytes_expected: None,
            bytes_downloaded: Some(0),
            bytes_per_second: None,
            elapsed_ms: None,
        }),
    )?;

    let mut yt_dlp = None;
    let yt_dlp_cookies = yt_dlp_cookie_file(&data_dir);
    let client = Client::builder()
        .user_agent(INGEST_USER_AGENT)
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())?;

    for (index, lesson) in lessons.iter().enumerate() {
        let lesson_dir = download_root.join(safe_path_segment(&lesson.id));
        fs::create_dir_all(&lesson_dir).map_err(|error| error.to_string())?;
        let lesson_job_id = format!("job-download-lesson-{}", lesson.id);
        let position = index + 1;
        let lesson_started_at = Utc::now().to_rfc3339();
        let lesson_started_instant = Instant::now();
        let lesson_label = format!("Download: {}", lesson.title);

        upsert_job_with_progress(
            &connection,
            JobUpdate {
                id: &lesson_job_id,
                kind: "download",
                state: "running",
                source_id: Some(&source_id),
                lesson_id: Some(&lesson.id),
                label: &lesson_label,
                detail: &format!("Item {position} of {attempted} from {source_label}."),
            },
            Some(JobProgress {
                started_at: Some(lesson_started_at.clone()),
                completed_at: None,
                bytes_expected: None,
                bytes_downloaded: Some(0),
                bytes_per_second: None,
                elapsed_ms: None,
            }),
        )?;

        if lesson.has_invalid_media_record {
            detach_lesson_media_records(&mut connection, &data_dir, &lesson.id)?;
        }

        let media_result = existing_completed_media_file(&lesson_dir, &lesson.content_type)
            .map(Ok)
            .unwrap_or_else(|| {
                download_lesson_media(
                    &client,
                    &mut yt_dlp,
                    lesson,
                    &lesson_dir,
                    DownloadMediaContext {
                        data_dir: &data_dir,
                        cookies_file: yt_dlp_cookies.as_deref(),
                        expected_content_hash: lesson.expected_content_hash.as_deref(),
                        progress: Some(DirectDownloadProgress {
                            connection: &connection,
                            job_id: &lesson_job_id,
                            source_id: &source_id,
                            lesson_id: &lesson.id,
                            label: &lesson_label,
                            started_at: &lesson_started_at,
                            total_bytes: None,
                            last_recorded_bytes: 0,
                        }),
                    },
                )
            })
            .and_then(|media_path| {
                let media_path =
                    ensure_webkit_playable_media(&media_path, &lesson.content_type, &lesson_dir)?;
                record_downloaded_media(
                    &mut connection,
                    &data_dir,
                    &lesson.id,
                    &media_path,
                    &lesson.content_type,
                    lesson.expected_content_hash.as_deref(),
                )?;
                Ok(media_path)
            });

        match media_result {
            Ok(media_path) => {
                downloaded += 1;
                let media_size = media_path
                    .metadata()
                    .map(|metadata| metadata.len().min(i64::MAX as u64) as i64)
                    .unwrap_or(0);
                total_downloaded_bytes = total_downloaded_bytes.saturating_add(media_size);
                let lesson_progress = completed_job_progress(
                    lesson_started_at.clone(),
                    lesson_started_instant,
                    Some(media_size),
                    Some(media_size),
                );
                upsert_job_with_progress(
                    &connection,
                    JobUpdate {
                        id: &lesson_job_id,
                        kind: "download",
                        state: "downloaded",
                        source_id: Some(&source_id),
                        lesson_id: Some(&lesson.id),
                        label: &lesson_label,
                        detail: &format!(
                            "Saved {} into the app library in {}.",
                            media_path
                                .file_name()
                                .and_then(|value| value.to_str())
                                .unwrap_or("media file"),
                            format_duration_ms(lesson_progress.elapsed_ms.unwrap_or_default())
                        ),
                    },
                    Some(lesson_progress),
                )?;
            }
            Err(error) => {
                failed += 1;
                messages.push(format!("{}: {error}", lesson.title));
                upsert_job_with_progress(
                    &connection,
                    JobUpdate {
                        id: &lesson_job_id,
                        kind: "download",
                        state: "failed",
                        source_id: Some(&source_id),
                        lesson_id: Some(&lesson.id),
                        label: &lesson_label,
                        detail: &error,
                    },
                    Some(completed_job_progress(
                        lesson_started_at.clone(),
                        lesson_started_instant,
                        None,
                        None,
                    )),
                )?;
            }
        }

        let remaining = attempted - downloaded - failed;
        let source_elapsed_ms = source_started_instant
            .elapsed()
            .as_millis()
            .min(i64::MAX as u128) as i64;
        upsert_job_with_progress(
            &connection,
            JobUpdate {
                id: &source_job_id,
                kind: "download",
                state: "running",
                source_id: Some(&source_id),
                lesson_id: None,
                label: &format!("Download media: {source_label}"),
                detail: &format!(
                    "{downloaded} downloaded, {failed} failed, {skipped} skipped; {remaining} remaining."
                ),
            },
            Some(JobProgress {
                started_at: Some(source_started_at.clone()),
                completed_at: None,
                bytes_expected: None,
                bytes_downloaded: Some(total_downloaded_bytes),
                bytes_per_second: bytes_per_second(total_downloaded_bytes, source_elapsed_ms),
                elapsed_ms: Some(source_elapsed_ms),
            }),
        )?;
    }

    let job_state = if failed == 0 { "downloaded" } else { "failed" };
    let source_progress = completed_job_progress(
        source_started_at.clone(),
        source_started_instant,
        Some(total_downloaded_bytes),
        Some(total_downloaded_bytes),
    );
    let detail = format!(
        "{downloaded} downloaded, {failed} failed, {skipped} skipped from {attempted} missing or invalid file-backed item(s), saving {} in {}.",
        format_bytes(total_downloaded_bytes),
        format_duration_ms(source_progress.elapsed_ms.unwrap_or_default())
    );
    upsert_job_with_progress(
        &connection,
        JobUpdate {
            id: &source_job_id,
            kind: "download",
            state: job_state,
            source_id: Some(&source_id),
            lesson_id: None,
            label: &format!("Download media: {source_label}"),
            detail: &detail,
        },
        Some(source_progress),
    )?;

    messages.insert(
        0,
        format!("{downloaded} file-backed item(s) downloaded for {source_label}."),
    );

    Ok(DownloadSourceSummary {
        source_id,
        source_label,
        attempted,
        downloaded,
        skipped,
        failed,
        messages,
    })
}

fn ingest_public_telegram(
    connection: &mut Connection,
    client: &Client,
    original_url: &str,
    telegram_source: TelegramSource,
) -> Result<IngestSummary, String> {
    let preview_url = format!("https://t.me/s/{}", telegram_source.username);
    let body = fetch_text(client, &preview_url)?;
    let document = Html::parse_document(&body);
    let message_selector = selector(".tgme_widget_message");
    let message_text_selector = selector(".tgme_widget_message_text");
    let message_link_selector = selector("a.tgme_widget_message_date");
    let message_time_selector = selector(".tgme_widget_message_date time");
    let channel_title = first_document_text(
        &document,
        &[
            ".tgme_channel_info_header_title span",
            ".tgme_channel_info_header_title",
            ".tgme_page_title",
        ],
    )
    .unwrap_or_else(|| telegram_source.username.clone());
    let mut lessons = Vec::new();

    for message in document.select(&message_selector) {
        let post_ref = message.value().attr("data-post").unwrap_or_default();
        let post_id = post_ref
            .rsplit('/')
            .next()
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        if let (Some(expected), Some(actual)) = (&telegram_source.post_id, &post_id) {
            if expected != actual {
                continue;
            }
        }

        let source_url = message
            .select(&message_link_selector)
            .next()
            .and_then(|link| link.value().attr("href"))
            .map(str::to_string)
            .or_else(|| {
                post_id
                    .as_ref()
                    .map(|id| format!("https://t.me/{}/{}", telegram_source.username, id))
            })
            .unwrap_or_else(|| preview_url.clone());

        let description = message
            .select(&message_text_selector)
            .next()
            .map(|element| normalize_text(&element.text().collect::<Vec<_>>().join(" ")))
            .filter(|value| !value.is_empty());
        let published_at = message
            .select(&message_time_selector)
            .next()
            .and_then(|time| time.value().attr("datetime"))
            .map(str::to_string);
        let title = title_from_text(description.as_deref())
            .or_else(|| post_id.as_ref().map(|id| format!("Telegram post {id}")))
            .unwrap_or_else(|| "Telegram channel post".to_string());

        lessons.push(DiscoveredLesson {
            title,
            content_type: "post".to_string(),
            source_url,
            retrieval_refs: Vec::new(),
            published_at,
            description,
            duration_seconds: None,
            adapter_name: "TelegramPublicPreviewAdapter".to_string(),
            provenance_note: "Captured from Telegram public channel preview.".to_string(),
            content_hash: None,
        });
    }

    if lessons.is_empty() {
        let detail = format!(
            "No public posts were found at {preview_url}. Public channels can usually be read without sign-in through t.me/s, but private channels, invite links, removed posts, and restricted media require a Telegram account or manual export."
        );
        record_standalone_job(connection, original_url, "unsupported", &detail)?;
        return Ok(IngestSummary {
            source_url: original_url.to_string(),
            discovered: 0,
            imported: 0,
            skipped: 0,
            failed: 1,
            messages: vec![detail],
        });
    }

    let source_suffix = stable_suffix(&format!("telegram:{}", telegram_source.username));
    let source_label = format!("Telegram: {channel_title}");
    let context = SourceContext {
        source_id: format!("source-telegram-{source_suffix}"),
        platform: "telegram".to_string(),
        source_label: source_label.clone(),
        source_identifier: preview_url,
        feed_format: "rss".to_string(),
        feed_transport: "https".to_string(),
        trust_state: "unsigned".to_string(),
        trusted_curator_id: None,
        last_verified_at: None,
        source_capability: capability(
            "supported",
            "limited",
            "supported",
            false,
            "none",
            "best-effort",
            "Public channel preview scraping works without sign-in when Telegram exposes t.me/s pages; private channels require a local session.",
        ),
        teacher_id: format!("teacher-telegram-{source_suffix}"),
        teacher_label: channel_title,
        teacher_description: "Imported from a public Telegram channel preview.".to_string(),
        teacher_source_links: vec![original_url.to_string()],
        collection_id: format!("collection-telegram-{source_suffix}"),
        collection_title: format!("Telegram: {}", telegram_source.username),
        collection_owner_label: "Public Telegram source".to_string(),
    };

    persist_discovered_lessons(
        connection,
        original_url,
        context,
        lessons,
        "Telegram public channel ingest",
    )
}

fn ingest_archive_org_item(
    connection: &mut Connection,
    client: &Client,
    original_url: &str,
    identifier: &str,
) -> Result<IngestSummary, String> {
    let metadata_url = archive_metadata_url(identifier);
    let metadata = fetch_json(client, &metadata_url)?;
    let item_metadata = metadata.get("metadata").unwrap_or(&metadata);
    let item_title = json_field_text(item_metadata, "title")
        .unwrap_or_else(|| format!("Archive.org item {identifier}"));
    let creator =
        json_field_text(item_metadata, "creator").unwrap_or_else(|| "Archive.org".to_string());
    let item_description = json_field_text(item_metadata, "description");
    let published_at = json_field_text(item_metadata, "date");
    let Some(files) = metadata.get("files").and_then(serde_json::Value::as_array) else {
        let detail = metadata
            .get("error")
            .and_then(json_text)
            .map(|error| format!("Archive.org metadata error for {identifier}: {error}."))
            .unwrap_or_else(|| {
                format!(
                    "Archive.org item {identifier} was not found or did not include a files list."
                )
            });
        record_standalone_job(connection, original_url, "unsupported", &detail)?;
        return Ok(IngestSummary {
            source_url: original_url.to_string(),
            discovered: 0,
            imported: 0,
            skipped: 0,
            failed: 1,
            messages: vec![detail],
        });
    };
    let lessons = files
        .iter()
        .filter_map(|file| {
            archive_file_lesson(
                identifier,
                file,
                &item_title,
                item_description.as_deref(),
                published_at.as_deref(),
            )
        })
        .take(300)
        .collect::<Vec<_>>();

    if lessons.is_empty() {
        let detail = format!(
            "Archive.org item {identifier} was found, but no supported video, audio, or PDF files were listed."
        );
        record_standalone_job(connection, original_url, "unsupported", &detail)?;
        return Ok(IngestSummary {
            source_url: original_url.to_string(),
            discovered: 0,
            imported: 0,
            skipped: 0,
            failed: 1,
            messages: vec![detail],
        });
    }

    let source_suffix = stable_suffix(&format!("archive-org:{identifier}"));
    let source_label = format!("Archive.org: {item_title}");
    let context = SourceContext {
        source_id: format!("source-archive-org-{source_suffix}"),
        platform: "archive-org".to_string(),
        source_label,
        source_identifier: format!("https://archive.org/details/{identifier}"),
        feed_format: "json-feed".to_string(),
        feed_transport: "https".to_string(),
        trust_state: "unsigned".to_string(),
        trusted_curator_id: None,
        last_verified_at: None,
        source_capability: capability(
            "supported",
            "supported",
            "supported",
            false,
            "none",
            "stable",
            "Uses Archive.org official metadata and downloadable file listings for videos, audio, and PDFs.",
        ),
        teacher_id: format!("teacher-archive-org-{source_suffix}"),
        teacher_label: creator,
        teacher_description: "Imported from Archive.org item metadata.".to_string(),
        teacher_source_links: vec![original_url.to_string()],
        collection_id: format!("collection-archive-org-{source_suffix}"),
        collection_title: item_title,
        collection_owner_label: "Archive.org item".to_string(),
    };

    persist_discovered_lessons(
        connection,
        original_url,
        context,
        lessons,
        "Archive.org metadata ingest",
    )
}

fn ingest_direct_source_url(
    connection: &mut Connection,
    client: &Client,
    original_url: &str,
) -> Result<Option<IngestSummary>, String> {
    let Some((platform, content_type)) = direct_source_platform(original_url) else {
        return Ok(None);
    };
    let title = direct_source_title(client, original_url);
    let source_suffix = stable_suffix(&format!("{platform}:{original_url}"));
    let platform_label = direct_source_platform_label(platform);
    let context = SourceContext {
        source_id: format!("source-{platform}-{source_suffix}"),
        platform: platform.to_string(),
        source_label: format!("{platform_label}: {title}"),
        source_identifier: original_url.to_string(),
        feed_format: "rss".to_string(),
        feed_transport: "https".to_string(),
        trust_state: "unsigned".to_string(),
        trusted_curator_id: None,
        last_verified_at: None,
        source_capability: direct_source_capability(platform),
        teacher_id: format!("teacher-{platform}-{source_suffix}"),
        teacher_label: host_label(original_url),
        teacher_description: format!("Imported from a user-added {platform_label} URL."),
        teacher_source_links: vec![original_url.to_string()],
        collection_id: format!("collection-{platform}-{source_suffix}"),
        collection_title: title.clone(),
        collection_owner_label: format!("{platform_label} source"),
    };
    let lessons = vec![DiscoveredLesson {
        title,
        content_type: content_type.to_string(),
        source_url: original_url.to_string(),
        retrieval_refs: Vec::new(),
        published_at: None,
        description: Some(
            "Captured from a user-added source URL; media download remains user controlled."
                .to_string(),
        ),
        duration_seconds: None,
        adapter_name: "DirectSourceUrlAdapter".to_string(),
        provenance_note:
            "Captured from a user-added source URL; media download remains user controlled."
                .to_string(),
        content_hash: None,
    }];

    persist_discovered_lessons(
        connection,
        original_url,
        context,
        lessons,
        "Direct source ingest",
    )
    .map(Some)
}

fn ingest_feed_url(
    connection: &mut Connection,
    client: &Client,
    original_url: &str,
    feed_url: &str,
) -> Result<IngestSummary, String> {
    let body = fetch_text(client, feed_url)?;
    let mut parsed_feed = parse_feed_document(&body, feed_url)?;

    if parsed_feed.lessons.is_empty() {
        return Err("Feed parsed successfully but contained no item or entry records.".to_string());
    }

    let platform = if parsed_feed.feed_format == "duroos-manifest" {
        "teacher-relay".to_string()
    } else {
        platform_for_feed(original_url)
    };
    let trusted_curator_id = parsed_feed.curator.as_ref().and_then(|curator| {
        trusted_curator_id_for_public_key(connection, &curator.public_key)
            .ok()
            .flatten()
    });
    if parsed_feed.trust_state == "signed-untrusted" && trusted_curator_id.is_some() {
        parsed_feed.trust_state = "signed-trusted".to_string();
    }
    let source_suffix = if is_nostr_reference(original_url) {
        stable_suffix(original_url)
    } else {
        stable_suffix(feed_url)
    };
    let source_label = match platform.as_str() {
        "youtube" => format!("YouTube: {}", parsed_feed.title),
        "archive-org" => format!("Archive.org: {}", parsed_feed.title),
        "teacher-relay" => format!("Channel: {}", parsed_feed.title),
        "rss-feed" => format!("Feed: {}", parsed_feed.title),
        _ => parsed_feed.title.clone(),
    };
    let teacher_label = parsed_feed
        .curator
        .as_ref()
        .map(|curator| curator.display_name.clone())
        .unwrap_or_else(|| parsed_feed.title.clone());
    let teacher_description = parsed_feed
        .curator
        .as_ref()
        .map(|curator| format!("Signed curator feed key: {}", curator.public_key))
        .unwrap_or_else(|| {
            format!(
                "Imported from a {} subscription.",
                parsed_feed.feed_format.replace('-', " ")
            )
        });
    let source_note = match parsed_feed.feed_format.as_str() {
        "duroos-manifest" => {
            "Signed Duroos manifests publish curator identity, source refs, hashes, and optional retrieval refs."
        }
        "json-feed" => {
            "JSON Feed subscriptions can ingest videos, audio, PDFs, and posts without account credentials."
        }
        "rss" | "atom" => {
            "RSS/Atom subscriptions can ingest video, audio, PDF, and teacher post items without account credentials."
        }
        _ => "Open feed subscriptions keep source metadata local.",
    };
    let context = SourceContext {
        source_id: format!("source-{platform}-{source_suffix}"),
        platform: platform.clone(),
        source_label: source_label.clone(),
        source_identifier: original_url.to_string(),
        feed_format: parsed_feed.feed_format.clone(),
        feed_transport: if is_nostr_reference(original_url) {
            "nostr".to_string()
        } else {
            "https".to_string()
        },
        trust_state: parsed_feed.trust_state.clone(),
        trusted_curator_id,
        last_verified_at: if parsed_feed.trust_state.starts_with("signed-") {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        },
        source_capability: capability(
            "supported",
            "limited",
            "supported",
            false,
            "none",
            if platform == "teacher-relay" {
                "stable"
            } else {
                "best-effort"
            },
            source_note,
        ),
        teacher_id: parsed_feed
            .curator
            .as_ref()
            .map(|curator| format!("teacher-curator-{}", safe_path_segment(&curator.id)))
            .unwrap_or_else(|| format!("teacher-feed-{source_suffix}")),
        teacher_label,
        teacher_description,
        teacher_source_links: vec![original_url.to_string()],
        collection_id: format!("collection-feed-{source_suffix}"),
        collection_title: parsed_feed.title.clone(),
        collection_owner_label: "Subscribed feed".to_string(),
    };

    persist_discovered_lessons(
        connection,
        original_url,
        context,
        parsed_feed.lessons,
        "Feed ingest",
    )
}

fn refresh_youtube_source(
    connection: &mut Connection,
    client: &Client,
    source: &RefreshableSource,
) -> Result<IngestSummary, String> {
    let feed_url = normalize_feed_url(&source.identifier);

    match ingest_feed_url(connection, client, &source.identifier, &feed_url) {
        Ok(summary) => Ok(summary),
        Err(error) => {
            let existing_count = count_source_lessons(connection, &source.id)?;
            let detail = if existing_count > 0 {
                format!(
                    "YouTube did not return a refreshable RSS feed for {}. Kept {} existing item(s). {}",
                    source.label,
                    existing_count,
                    feed_ingest_error_detail(&source.identifier, &error)
                )
            } else {
                feed_ingest_error_detail(&source.identifier, &error)
            };
            let state = if existing_count > 0 {
                "skipped"
            } else {
                "unsupported"
            };

            record_source_refresh_job(connection, &source.id, &source.label, state, &detail)?;
            mark_source_checked(connection, &source.id)?;

            Ok(IngestSummary {
                source_url: source.identifier.clone(),
                discovered: 0,
                imported: 0,
                skipped: 0,
                failed: if existing_count > 0 { 0 } else { 1 },
                messages: vec![detail],
            })
        }
    }
}

fn refreshable_source(
    connection: &Connection,
    source_id: &str,
) -> Result<Option<RefreshableSource>, String> {
    connection
        .query_row(
            "SELECT id, platform, label, identifier
             FROM sources
             WHERE id = ?1",
            params![source_id],
            |row| {
                Ok(RefreshableSource {
                    id: row.get(0)?,
                    platform: row.get(1)?,
                    label: row.get(2)?,
                    identifier: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn count_source_lessons(connection: &Connection, source_id: &str) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM lessons WHERE source_id = ?1",
            params![source_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn mark_source_checked(connection: &Connection, source_id: &str) -> Result<(), String> {
    connection
        .execute(
            "UPDATE sources SET last_checked_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), source_id],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn record_source_refresh_job(
    connection: &Connection,
    source_id: &str,
    source_label: &str,
    state: &str,
    detail: &str,
) -> Result<(), String> {
    upsert_job(
        connection,
        JobUpdate {
            id: &format!("job-refresh-source-{}", stable_suffix(source_id)),
            kind: "metadata",
            state,
            source_id: Some(source_id),
            lesson_id: None,
            label: &format!("Refresh: {source_label}"),
            detail,
        },
    )
}

fn parse_feed_document(body: &str, feed_url: &str) -> Result<ParsedFeed, String> {
    let trimmed = body.trim_start();

    if trimmed.starts_with('{') {
        let parsed = serde_json::from_str::<serde_json::Value>(body)
            .map_err(|error| format!("Could not parse source JSON: {error}."))?;

        if is_duroos_manifest(&parsed) {
            return parse_duroos_manifest(body, &parsed);
        }

        if is_json_feed(&parsed) {
            return parse_json_feed(&parsed, feed_url);
        }

        return Err("JSON source is not a JSON Feed or Duroos collection manifest.".to_string());
    }

    let document = roxmltree::Document::parse(body)
        .map_err(|error| format!("Could not parse source as RSS/Atom XML: {error}."))?;
    let feed_format = xml_feed_format(&document);
    let title = feed_title(&document).unwrap_or_else(|| host_label(feed_url));
    let lessons = feed_lessons(&document)?;

    Ok(ParsedFeed {
        title,
        feed_format,
        trust_state: "unsigned".to_string(),
        curator: None,
        lessons,
    })
}

fn is_duroos_manifest(value: &serde_json::Value) -> bool {
    value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_i64)
        .is_some_and(|version| version == 1 || version == 2)
        && value.get("collection").is_some()
        && value.get("lessons").is_some()
}

fn is_json_feed(value: &serde_json::Value) -> bool {
    value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(|version| version.contains("jsonfeed.org"))
        .unwrap_or(false)
        && value
            .get("items")
            .and_then(serde_json::Value::as_array)
            .is_some()
}

fn parse_duroos_manifest(
    manifest_json: &str,
    value: &serde_json::Value,
) -> Result<ParsedFeed, String> {
    let report = manifest::validate_collection_manifest(manifest_json);
    if !report.valid {
        return Err(format!(
            "Duroos manifest validation failed: {}",
            report.errors.join("; ")
        ));
    }

    let collection = value
        .get("collection")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Duroos manifest collection must be an object.".to_string())?;
    let title = collection
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(|title| clip_text(title, 90))
        .unwrap_or_else(|| "Duroos collection".to_string());
    let curator = value
        .get("curator")
        .and_then(serde_json::Value::as_object)
        .map(|curator| ManifestCurator {
            id: curator
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("curator")
                .to_string(),
            display_name: curator
                .get("displayName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Curator")
                .to_string(),
            public_key: curator
                .get("publicKey")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
        });
    let lessons = value
        .get("lessons")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Duroos manifest lessons must be an array.".to_string())?
        .iter()
        .take(300)
        .filter_map(duroos_manifest_lesson)
        .collect::<Vec<_>>();

    Ok(ParsedFeed {
        title,
        feed_format: "duroos-manifest".to_string(),
        trust_state: report.trust_state.unwrap_or_else(|| "unsigned".to_string()),
        curator,
        lessons,
    })
}

fn duroos_manifest_lesson(lesson: &serde_json::Value) -> Option<DiscoveredLesson> {
    let lesson_object = lesson.as_object()?;
    let title = lesson_object
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(|title| clip_text(title, 140))
        .filter(|title| !title.is_empty())?;
    let description = lesson_object
        .get("description")
        .and_then(serde_json::Value::as_str)
        .map(|description| clip_text(description, 900));
    let retrieval_refs = manifest_retrieval_refs(lesson_object);
    let source_url = downloadable_retrieval_url(&retrieval_refs)
        .or_else(|| first_source_ref_url(lesson_object))?;
    let published_at = lesson_object
        .get("sourceRefs")
        .and_then(serde_json::Value::as_array)
        .and_then(|refs| refs.first())
        .and_then(|source_ref| source_ref.get("publishedAt"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let content_hash = first_sha256_hash(lesson_object);
    let content_type = lesson_object
        .get("contentType")
        .and_then(serde_json::Value::as_str)
        .filter(|content_type| is_valid_content_type(content_type))
        .map(str::to_string)
        .unwrap_or_else(|| classify_feed_content(&source_url, None, description.as_deref()));
    let provenance_note = lesson_object
        .get("provenance")
        .and_then(serde_json::Value::as_object)
        .and_then(|provenance| provenance.get("permissionNote"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Imported from a Duroos curator manifest; review rights before downloading.")
        .to_string();

    Some(DiscoveredLesson {
        title,
        content_type,
        source_url,
        retrieval_refs,
        published_at,
        description,
        duration_seconds: lesson_object
            .get("durationSeconds")
            .and_then(serde_json::Value::as_i64),
        adapter_name: "DuroosManifestAdapter".to_string(),
        provenance_note,
        content_hash,
    })
}

fn parse_json_feed(value: &serde_json::Value, feed_url: &str) -> Result<ParsedFeed, String> {
    let title = value
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(|title| clip_text(title, 90))
        .unwrap_or_else(|| host_label(feed_url));
    let lessons = value
        .get("items")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "JSON Feed items must be an array.".to_string())?
        .iter()
        .take(100)
        .filter_map(json_feed_lesson)
        .collect::<Vec<_>>();

    Ok(ParsedFeed {
        title,
        feed_format: "json-feed".to_string(),
        trust_state: "unsigned".to_string(),
        curator: None,
        lessons,
    })
}

fn json_feed_lesson(item: &serde_json::Value) -> Option<DiscoveredLesson> {
    let item_object = item.as_object()?;
    let attachment = item_object
        .get("attachments")
        .and_then(serde_json::Value::as_array)
        .and_then(|attachments| {
            attachments
                .iter()
                .filter_map(serde_json::Value::as_object)
                .find(|attachment| {
                    attachment
                        .get("url")
                        .and_then(serde_json::Value::as_str)
                        .map(|url| {
                            classify_feed_content(
                                url,
                                attachment
                                    .get("mime_type")
                                    .and_then(serde_json::Value::as_str),
                                None,
                            ) != "post"
                        })
                        .unwrap_or(false)
                })
        });
    let source_url = attachment
        .and_then(|attachment| attachment.get("url"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            item_object
                .get("external_url")
                .and_then(serde_json::Value::as_str)
        })
        .or_else(|| item_object.get("url").and_then(serde_json::Value::as_str))
        .or_else(|| item_object.get("id").and_then(serde_json::Value::as_str))?
        .to_string();
    let description = item_object
        .get("summary")
        .or_else(|| item_object.get("content_text"))
        .or_else(|| item_object.get("content_html"))
        .and_then(serde_json::Value::as_str)
        .and_then(normalize_source_text)
        .map(|description| clip_text(&description, 900));
    let title = item_object
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(|title| clip_text(title, 140))
        .or_else(|| title_from_text(description.as_deref()))
        .unwrap_or_else(|| "Untitled JSON feed item".to_string());
    let mime_type = attachment
        .and_then(|attachment| attachment.get("mime_type"))
        .and_then(serde_json::Value::as_str);
    let content_type = classify_feed_content(&source_url, mime_type, description.as_deref());
    let content_hash = item_object
        .get("contentHash")
        .or_else(|| item_object.get("hash"))
        .and_then(serde_json::Value::as_str)
        .filter(|hash| hash.starts_with("sha256:") || hash.len() == 64)
        .map(str::to_string);

    Some(DiscoveredLesson {
        title,
        content_type,
        source_url,
        retrieval_refs: Vec::new(),
        published_at: item_object
            .get("date_published")
            .or_else(|| item_object.get("date_modified"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        description,
        duration_seconds: attachment
            .and_then(|attachment| attachment.get("duration_in_seconds"))
            .and_then(serde_json::Value::as_i64),
        adapter_name: "JsonFeedAdapter".to_string(),
        provenance_note: PUBLIC_SOURCE_PROVENANCE_NOTE.to_string(),
        content_hash,
    })
}

fn downloadable_retrieval_url(retrieval_refs: &[RetrievalRef]) -> Option<String> {
    retrieval_refs.iter().find_map(|retrieval_ref| {
        if matches!(retrieval_ref.kind.as_str(), "direct-url" | "enclosure-url") {
            retrieval_ref.url.clone()
        } else {
            None
        }
    })
}

fn manifest_retrieval_refs(
    lesson_object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<RetrievalRef> {
    lesson_object
        .get("retrievalRefs")
        .and_then(serde_json::Value::as_array)
        .map(|refs| {
            refs.iter()
                .filter_map(serde_json::Value::as_object)
                .filter_map(manifest_retrieval_ref)
                .collect()
        })
        .unwrap_or_default()
}

fn manifest_retrieval_ref(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<RetrievalRef> {
    let kind = object
        .get("kind")
        .and_then(serde_json::Value::as_str)?
        .to_string();
    let media_type = object
        .get("mediaType")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let mime_type = object
        .get("mimeType")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| media_type.clone());
    let sha256 = object
        .get("sha256")
        .and_then(serde_json::Value::as_str)
        .map(format_sha256);
    let size_bytes = object.get("sizeBytes").and_then(serde_json::Value::as_i64);

    match kind.as_str() {
        "direct-url" | "enclosure-url" => {
            let url = object
                .get("url")
                .and_then(serde_json::Value::as_str)
                .filter(|url| is_safe_http_url(url))?
                .to_string();
            Some(RetrievalRef {
                kind,
                url: Some(url),
                service: object
                    .get("service")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                sha256,
                size_bytes,
                mime_type,
                media_type,
                ..Default::default()
            })
        }
        "ipfs-cid" => object
            .get("cid")
            .and_then(serde_json::Value::as_str)
            .filter(|cid| !cid.trim().is_empty())
            .map(|cid| RetrievalRef {
                kind,
                cid: Some(cid.to_string()),
                gateway_url: object
                    .get("gatewayUrl")
                    .and_then(serde_json::Value::as_str)
                    .filter(|url| is_safe_http_url(url))
                    .map(str::to_string),
                sha256,
                size_bytes,
                mime_type,
                media_type,
                ..Default::default()
            }),
        "magnet" => object
            .get("magnetUri")
            .and_then(serde_json::Value::as_str)
            .map(|magnet_uri| RetrievalRef {
                kind,
                magnet_uri: Some(magnet_uri.to_string()),
                media_type,
                ..Default::default()
            }),
        _ => None,
    }
}

fn first_source_ref_url(
    lesson_object: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    lesson_object
        .get("sourceRefs")
        .and_then(serde_json::Value::as_array)
        .and_then(|refs| refs.first())
        .and_then(|source_ref| source_ref.get("originUrl"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn first_sha256_hash(lesson_object: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    lesson_object
        .get("contentHashes")
        .and_then(serde_json::Value::as_array)
        .and_then(|hashes| {
            hashes.iter().find_map(|hash| {
                let hash = hash.as_str()?;
                if hash.starts_with("sha256:") {
                    Some(hash.to_string())
                } else if hash.len() == 64
                    && hash.chars().all(|character| character.is_ascii_hexdigit())
                {
                    Some(format!("sha256:{hash}"))
                } else {
                    None
                }
            })
        })
}

fn trusted_curator_id_for_public_key(
    connection: &Connection,
    public_key: &str,
) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT id FROM trusted_curators WHERE public_key = ?1",
            params![public_key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn trusted_curator_for_public_key(
    connection: &Connection,
    public_key: &str,
) -> Result<Option<TrustedCurator>, String> {
    connection
        .query_row(
            "SELECT id, display_name, public_key, trust_note, added_at
             FROM trusted_curators
             WHERE public_key = ?1",
            params![public_key],
            |row| {
                Ok(TrustedCurator {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    public_key: row.get(2)?,
                    trust_note: row.get(3)?,
                    added_at: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn trusted_curator_for_id(
    connection: &Connection,
    curator_id: &str,
) -> Result<Option<TrustedCurator>, String> {
    connection
        .query_row(
            "SELECT id, display_name, public_key, trust_note, added_at
             FROM trusted_curators
             WHERE id = ?1",
            params![curator_id],
            |row| {
                Ok(TrustedCurator {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    public_key: row.get(2)?,
                    trust_note: row.get(3)?,
                    added_at: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn source_ids_for_curator_public_key(
    connection: &Connection,
    public_key: &str,
) -> Result<Vec<String>, String> {
    let marker = format!("Signed curator feed key: {public_key}");
    let mut teacher_statement = connection
        .prepare(
            "SELECT source_links_json
             FROM teachers
             WHERE description = ?1",
        )
        .map_err(|error| error.to_string())?;
    let teacher_rows = teacher_statement
        .query_map(params![marker], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    let source_links = teacher_rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter_map(|json| serde_json::from_str::<Vec<String>>(&json).ok())
        .flatten()
        .collect::<Vec<_>>();

    if source_links.is_empty() {
        return Ok(Vec::new());
    }

    let mut source_statement = connection
        .prepare(
            "SELECT id, identifier
             FROM sources
             WHERE feed_format = 'duroos-manifest'
               AND trust_state IN ('signed-untrusted', 'signed-trusted')",
        )
        .map_err(|error| error.to_string())?;
    let source_rows = source_statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let source_rows = source_rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(source_rows
        .into_iter()
        .filter_map(|(source_id, identifier)| {
            source_links
                .iter()
                .any(|source_link| source_link == &identifier)
                .then_some(source_id)
        })
        .collect())
}

fn promote_sources_for_trusted_curator(
    connection: &Connection,
    curator_id: &str,
    public_key: &str,
) -> Result<i64, String> {
    let mut updated = 0;
    for source_id in source_ids_for_curator_public_key(connection, public_key)? {
        updated += connection
            .execute(
                "UPDATE sources
                 SET trust_state = 'signed-trusted',
                     trusted_curator_id = ?1,
                     last_verified_at = COALESCE(last_verified_at, ?2)
                 WHERE id = ?3
                   AND trust_state = 'signed-untrusted'",
                params![curator_id, Utc::now().to_rfc3339(), source_id],
            )
            .map_err(|error| error.to_string())? as i64;
    }

    Ok(updated)
}

fn downgrade_sources_for_trusted_curator(
    connection: &Connection,
    curator_id: &str,
    public_key: &str,
) -> Result<i64, String> {
    let mut updated = connection
        .execute(
            "UPDATE sources
             SET trust_state = 'signed-untrusted',
                 trusted_curator_id = NULL
             WHERE trusted_curator_id = ?1
               AND trust_state = 'signed-trusted'",
            params![curator_id],
        )
        .map_err(|error| error.to_string())? as i64;

    for source_id in source_ids_for_curator_public_key(connection, public_key)? {
        updated += connection
            .execute(
                "UPDATE sources
                 SET trust_state = 'signed-untrusted',
                     trusted_curator_id = NULL
                 WHERE id = ?1
                   AND trust_state = 'signed-trusted'",
                params![source_id],
            )
            .map_err(|error| error.to_string())? as i64;
    }

    Ok(updated)
}

fn persist_discovered_lessons(
    connection: &mut Connection,
    source_url: &str,
    context: SourceContext,
    lessons: Vec<DiscoveredLesson>,
    job_label: &str,
) -> Result<IngestSummary, String> {
    let discovered = lessons.len() as i64;
    let mut imported = 0;
    let mut skipped = 0;
    let failed = 0;
    let mut messages = Vec::new();
    let now = Utc::now().to_rfc3339();
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;

    ensure_source_context(&transaction, &context, &now)?;

    for lesson in lessons {
        if insert_discovered_lesson(&transaction, &context, &lesson, &now)? {
            imported += 1;
        } else {
            skipped += 1;
        }
    }

    transaction
        .execute(
            "UPDATE collections
             SET lesson_count = (SELECT COUNT(*) FROM lessons WHERE collection_id = ?1)
             WHERE id = ?1",
            params![context.collection_id],
        )
        .map_err(|error| error.to_string())?;

    let state = if imported > 0 {
        "found"
    } else if skipped > 0 {
        "skipped"
    } else {
        "unsupported"
    };
    let detail = format!(
        "{discovered} item(s) discovered; {imported} new lesson(s), {skipped} duplicate(s)."
    );
    transaction
        .execute(
            "INSERT INTO jobs
             (id, kind, state, source_id, lesson_id, label, detail, retry_count, updated_at)
             VALUES (?1, 'metadata', ?2, ?3, NULL, ?4, ?5, 0, ?6)",
            params![
                format!("job-{}", Uuid::new_v4()),
                state,
                context.source_id,
                job_label,
                detail,
                now
            ],
        )
        .map_err(|error| error.to_string())?;

    transaction.commit().map_err(|error| error.to_string())?;

    messages.push(format!(
        "{discovered} item(s) discovered from {}. {imported} new lesson(s) added; {skipped} duplicate(s) skipped.",
        context.source_label
    ));

    Ok(IngestSummary {
        source_url: source_url.to_string(),
        discovered,
        imported,
        skipped,
        failed,
        messages,
    })
}

fn ensure_source_context(
    transaction: &Transaction<'_>,
    context: &SourceContext,
    checked_at: &str,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO sources
             (id, platform, label, identifier, feed_format, feed_transport, trust_state,
              trusted_curator_id, auth_mode, update_schedule, capability_json, enabled,
              last_checked_at, last_verified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'none', 'Manual + daily check',
              ?9, 1, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
               label = excluded.label,
               identifier = excluded.identifier,
               feed_format = excluded.feed_format,
               feed_transport = excluded.feed_transport,
               trust_state = excluded.trust_state,
               trusted_curator_id = excluded.trusted_curator_id,
               capability_json = excluded.capability_json,
               enabled = 1,
               last_checked_at = excluded.last_checked_at,
               last_verified_at = excluded.last_verified_at",
            params![
                context.source_id,
                context.platform,
                context.source_label,
                context.source_identifier,
                context.feed_format,
                context.feed_transport,
                context.trust_state,
                context.trusted_curator_id.as_deref(),
                serde_json::to_string(&context.source_capability)
                    .map_err(|error| error.to_string())?,
                checked_at,
                context.last_verified_at.as_deref()
            ],
        )
        .map_err(|error| error.to_string())?;

    transaction
        .execute(
            "INSERT INTO teachers (id, display_name, description, source_links_json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
               display_name = excluded.display_name,
               description = excluded.description,
               source_links_json = excluded.source_links_json",
            params![
                context.teacher_id,
                context.teacher_label,
                context.teacher_description,
                serde_json::to_string(&context.teacher_source_links)
                    .map_err(|error| error.to_string())?
            ],
        )
        .map_err(|error| error.to_string())?;

    transaction
        .execute(
            "INSERT INTO collections
             (id, title, owner_label, sort_order, lesson_count, source_ids_json)
             VALUES (?1, ?2, ?3, 500, 0, ?4)
             ON CONFLICT(id) DO UPDATE SET
               title = excluded.title,
               owner_label = excluded.owner_label,
               source_ids_json = excluded.source_ids_json",
            params![
                context.collection_id,
                context.collection_title,
                context.collection_owner_label,
                serde_json::to_string(&vec![context.source_id.clone()])
                    .map_err(|error| error.to_string())?
            ],
        )
        .map_err(|error| error.to_string())?;

    if context.platform == "teacher-relay" {
        let trust_policy = if context.trust_state.starts_with("signed-") {
            "signed-feed"
        } else {
            "manual-review"
        };

        transaction
            .execute(
                "INSERT INTO teacher_relays
                 (id, teacher_id, title, feed_url, feed_format, feed_transport, trust_state,
                  subscriber_count, visibility, trust_policy, auto_download, last_published_at,
                  last_verified_at, description)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 'public', ?8, 0, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET
                   teacher_id = excluded.teacher_id,
                   title = excluded.title,
                   feed_url = excluded.feed_url,
                   feed_format = excluded.feed_format,
                   feed_transport = excluded.feed_transport,
                   trust_state = excluded.trust_state,
                   trust_policy = excluded.trust_policy,
                   auto_download = 0,
                   last_published_at = excluded.last_published_at,
                   last_verified_at = excluded.last_verified_at,
                   description = excluded.description",
                params![
                    format!("relay-{}", context.source_id),
                    context.teacher_id,
                    context.collection_title,
                    context.source_identifier,
                    context.feed_format,
                    context.feed_transport,
                    context.trust_state,
                    trust_policy,
                    checked_at,
                    context.last_verified_at.as_deref(),
                    "Curator feed subscriptions are review-first; no seeding or mirroring is enabled by default."
                ],
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn normalize_organization_label(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn user_teacher_id(display_name: &str) -> String {
    let display_name = normalize_organization_label(display_name);
    if display_name.eq_ignore_ascii_case("Personal Library") {
        return "teacher-3".to_string();
    }

    format!(
        "teacher-user-{}",
        stable_suffix(&display_name.to_lowercase())
    )
}

fn user_collection_id(title: &str) -> String {
    let title = normalize_organization_label(title);
    if title.eq_ignore_ascii_case("Local Imports") {
        return "collection-2".to_string();
    }

    format!("collection-user-{}", stable_suffix(&title.to_lowercase()))
}

fn existing_teacher_id_for_label(
    transaction: &Transaction<'_>,
    display_name: &str,
) -> Result<Option<String>, String> {
    let label = normalize_organization_label(display_name).to_lowercase();
    transaction
        .query_row(
            "SELECT id
             FROM teachers
             WHERE lower(display_name) = ?1
             ORDER BY CASE WHEN id LIKE 'teacher-user-%' THEN 1 ELSE 0 END, id
             LIMIT 1",
            params![label],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn existing_collection_id_for_title(
    transaction: &Transaction<'_>,
    title: &str,
    source_id: &str,
) -> Result<Option<String>, String> {
    let title = normalize_organization_label(title).to_lowercase();
    let source_marker = format!("%\"{source_id}\"%");
    transaction
        .query_row(
            "SELECT id
             FROM collections
             WHERE lower(title) = ?1
             ORDER BY
               CASE WHEN source_ids_json LIKE ?2 THEN 0 ELSE 1 END,
               CASE WHEN id LIKE 'collection-user-%' THEN 1 ELSE 0 END,
               sort_order,
               id
             LIMIT 1",
            params![title, source_marker],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn upsert_user_teacher(
    transaction: &Transaction<'_>,
    teacher_id: &str,
    display_name: &str,
) -> Result<(), String> {
    if teacher_id == "teacher-3" {
        return Ok(());
    }

    transaction
        .execute(
            "INSERT INTO teachers (id, display_name, description, source_links_json)
             VALUES (?1, ?2, 'User-organized teacher label.', '[]')
             ON CONFLICT(id) DO UPDATE SET
               display_name = excluded.display_name",
            params![teacher_id, normalize_organization_label(display_name)],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn upsert_user_collection(
    transaction: &Transaction<'_>,
    collection_id: &str,
    title: &str,
    source_id: &str,
) -> Result<(), String> {
    if collection_id == "collection-2" {
        return Ok(());
    }

    let existing_sources: Option<String> = transaction
        .query_row(
            "SELECT source_ids_json FROM collections WHERE id = ?1",
            params![collection_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let mut source_ids = existing_sources
        .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
        .unwrap_or_default();
    if !source_ids.iter().any(|id| id == source_id) {
        source_ids.push(source_id.to_string());
    }

    transaction
        .execute(
            "INSERT INTO collections
             (id, title, owner_label, sort_order, lesson_count, source_ids_json)
             VALUES (?1, ?2, 'Smart Library', 800, 0, ?3)
             ON CONFLICT(id) DO UPDATE SET
               title = excluded.title,
               owner_label = excluded.owner_label,
               source_ids_json = excluded.source_ids_json",
            params![
                collection_id,
                normalize_organization_label(title),
                serde_json::to_string(&source_ids).map_err(|error| error.to_string())?
            ],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn refresh_lesson_fts(
    transaction: &Transaction<'_>,
    lesson_id: &str,
    title: &str,
    description: &str,
    teacher_label: &str,
    collection_title: &str,
    source_label: &str,
) -> Result<(), String> {
    transaction
        .execute(
            "DELETE FROM lessons_fts WHERE lesson_id = ?1",
            params![lesson_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO lessons_fts
             (lesson_id, title, description, teacher, collection_title, source_label)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                lesson_id,
                title,
                description,
                normalize_organization_label(teacher_label),
                normalize_organization_label(collection_title),
                source_label
            ],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn ensure_collection_source_membership(
    transaction: &Transaction<'_>,
    collection_id: &str,
    source_id: &str,
) -> Result<(), String> {
    let existing_sources: String = transaction
        .query_row(
            "SELECT source_ids_json FROM collections WHERE id = ?1",
            params![collection_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let mut source_ids = serde_json::from_str::<Vec<String>>(&existing_sources).unwrap_or_default();
    if source_ids.iter().any(|id| id == source_id) {
        return Ok(());
    }

    source_ids.push(source_id.to_string());
    transaction
        .execute(
            "UPDATE collections SET source_ids_json = ?1 WHERE id = ?2",
            params![
                serde_json::to_string(&source_ids).map_err(|error| error.to_string())?,
                collection_id
            ],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn refresh_collection_count(
    transaction: &Transaction<'_>,
    collection_id: &str,
) -> Result<(), String> {
    transaction
        .execute(
            "UPDATE collections
             SET lesson_count = (SELECT COUNT(*) FROM lessons WHERE collection_id = ?1)
             WHERE id = ?1",
            params![collection_id],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn cleanup_empty_collection(
    transaction: &Transaction<'_>,
    collection_id: &str,
) -> Result<(), String> {
    if !collection_id.starts_with("collection-user-") {
        return Ok(());
    }

    let remaining_lessons: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM lessons WHERE collection_id = ?1",
            params![collection_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if remaining_lessons == 0 {
        transaction
            .execute(
                "DELETE FROM collections WHERE id = ?1",
                params![collection_id],
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn cleanup_empty_teacher(transaction: &Transaction<'_>, teacher_id: &str) -> Result<(), String> {
    if !teacher_id.starts_with("teacher-user-") {
        return Ok(());
    }

    let remaining_lessons: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM lessons WHERE teacher_id = ?1",
            params![teacher_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let relay_count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM teacher_relays WHERE teacher_id = ?1",
            params![teacher_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if remaining_lessons == 0 && relay_count == 0 {
        transaction
            .execute("DELETE FROM teachers WHERE id = ?1", params![teacher_id])
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn infer_local_lesson_organization(source_path: &Path) -> LocalLessonOrganization {
    let fallback_title = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(clean_local_label)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Imported lesson".to_string());

    if let Some((teacher, collection, title)) = infer_bracketed_local_label(&fallback_title)
        .or_else(|| infer_dash_local_label(&fallback_title))
    {
        return local_lesson_organization(title, teacher, collection);
    }

    if let Some((teacher, collection)) = infer_folder_organization(source_path) {
        return local_lesson_organization(fallback_title, teacher, collection);
    }

    LocalLessonOrganization {
        title: fallback_title,
        teacher_id: "teacher-3".to_string(),
        teacher_label: "Personal Library".to_string(),
        collection_id: "collection-2".to_string(),
        collection_title: "Local Imports".to_string(),
    }
}

fn local_lesson_organization(
    title: String,
    teacher_label: String,
    collection_title: String,
) -> LocalLessonOrganization {
    let teacher_label = normalize_organization_label(&teacher_label);
    let collection_title = normalize_organization_label(&collection_title);
    LocalLessonOrganization {
        title: normalize_organization_label(&title),
        teacher_id: user_teacher_id(&teacher_label),
        teacher_label,
        collection_id: user_collection_id(&collection_title),
        collection_title,
    }
}

fn infer_bracketed_local_label(value: &str) -> Option<(String, String, String)> {
    let remainder = value.strip_prefix('[')?;
    let closing = remainder.find(']')?;
    let teacher = clean_local_label(&remainder[..closing]);
    let after_teacher = clean_local_label(
        remainder[closing + 1..]
            .trim_start_matches(['-', ':', ' '])
            .trim(),
    );
    let parts = dash_parts(&after_teacher);
    if teacher.is_empty() || parts.len() < 2 {
        return None;
    }

    Some((teacher, parts[0].clone(), parts[1..].join(" - ")))
}

fn infer_dash_local_label(value: &str) -> Option<(String, String, String)> {
    let parts = dash_parts(value);
    if parts.len() < 3 {
        return None;
    }

    Some((parts[0].clone(), parts[1].clone(), parts[2..].join(" - ")))
}

fn infer_folder_organization(source_path: &Path) -> Option<(String, String)> {
    let collection_dir = source_path.parent()?;
    let teacher_dir = collection_dir.parent()?;
    let collection = folder_label(collection_dir)?;
    let teacher = folder_label(teacher_dir)?;
    let teacher_parent = teacher_dir.parent().and_then(folder_label);

    if is_generic_folder(&teacher)
        || is_generic_folder(&collection)
        || teacher_parent
            .as_deref()
            .map(|label| matches!(label.to_ascii_lowercase().as_str(), "users" | "home"))
            .unwrap_or(false)
    {
        return None;
    }

    Some((teacher, collection))
}

fn folder_label(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(clean_local_label)
        .filter(|value| !value.is_empty())
}

fn clean_local_label(value: &str) -> String {
    normalize_organization_label(&value.replace(['_', '/'], " "))
}

fn dash_parts(value: &str) -> Vec<String> {
    value
        .split(" - ")
        .map(clean_local_label)
        .filter(|part| !part.is_empty())
        .collect()
}

fn is_generic_folder(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "desktop"
            | "documents"
            | "downloads"
            | "movies"
            | "music"
            | "videos"
            | "library"
            | "imports"
            | "tmp"
            | "temp"
    )
}

fn insert_discovered_lesson(
    transaction: &Transaction<'_>,
    context: &SourceContext,
    lesson: &DiscoveredLesson,
    imported_at: &str,
) -> Result<bool, String> {
    if discovered_duplicate_lesson_title(transaction, lesson)?.is_some() {
        return Ok(false);
    }

    let lesson_id = format!("lesson-{}", stable_suffix(&lesson.source_url));
    let provenance_id = format!(
        "prov-{}",
        stable_suffix(&format!("prov:{}", lesson.source_url))
    );
    let description = lesson
        .description
        .as_deref()
        .unwrap_or("Imported source metadata");

    transaction
        .execute(
            "INSERT INTO lessons
             (id, title, content_type, teacher_id, collection_id, source_id, source_url,
              retrieval_refs_json, published_at, description, thumbnail_tone, duration_seconds,
              media_file_id, provenance_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'emerald', ?11, NULL, ?12)",
            params![
                &lesson_id,
                &lesson.title,
                &lesson.content_type,
                &context.teacher_id,
                &context.collection_id,
                &context.source_id,
                &lesson.source_url,
                retrieval_refs_to_json(&lesson.retrieval_refs)?,
                lesson.published_at.as_deref(),
                description,
                lesson.duration_seconds,
                &provenance_id
            ],
        )
        .map_err(|error| error.to_string())?;

    transaction
        .execute(
            "INSERT INTO provenance_records
             (id, lesson_id, origin_url, permission_note, imported_at, adapter_name, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                &provenance_id,
                &lesson_id,
                &lesson.source_url,
                &lesson.provenance_note,
                imported_at,
                &lesson.adapter_name,
                lesson.content_hash.as_deref()
            ],
        )
        .map_err(|error| error.to_string())?;

    transaction
        .execute(
            "INSERT INTO lessons_fts
             (lesson_id, title, description, teacher, collection_title, source_label)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &lesson_id,
                &lesson.title,
                description,
                &context.teacher_label,
                &context.collection_title,
                &context.source_label
            ],
        )
        .map_err(|error| error.to_string())?;

    Ok(true)
}

fn record_standalone_job(
    connection: &mut Connection,
    source_url: &str,
    state: &str,
    detail: &str,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO jobs
             (id, kind, state, source_id, lesson_id, label, detail, retry_count, updated_at)
             VALUES (?1, 'metadata', ?2, NULL, NULL, 'Source ingest', ?3, 0, ?4)",
            params![
                format!("job-{}", Uuid::new_v4()),
                state,
                format!("{detail} Source: {source_url}"),
                Utc::now().to_rfc3339()
            ],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn upsert_job(connection: &Connection, job: JobUpdate<'_>) -> Result<(), String> {
    upsert_job_with_progress(connection, job, None)
}

fn upsert_job_with_progress(
    connection: &Connection,
    job: JobUpdate<'_>,
    progress: Option<JobProgress>,
) -> Result<(), String> {
    let progress = progress.unwrap_or(JobProgress {
        started_at: None,
        completed_at: None,
        bytes_expected: None,
        bytes_downloaded: None,
        bytes_per_second: None,
        elapsed_ms: None,
    });
    connection
        .execute(
            "INSERT INTO jobs
             (id, kind, state, source_id, lesson_id, label, detail, retry_count, updated_at,
              started_at, completed_at, bytes_expected, bytes_downloaded, bytes_per_second,
              elapsed_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
               kind = excluded.kind,
               state = excluded.state,
               source_id = excluded.source_id,
               lesson_id = excluded.lesson_id,
               label = excluded.label,
               detail = excluded.detail,
               updated_at = excluded.updated_at,
               started_at = COALESCE(excluded.started_at, jobs.started_at),
               completed_at = COALESCE(excluded.completed_at, jobs.completed_at),
               bytes_expected = COALESCE(excluded.bytes_expected, jobs.bytes_expected),
               bytes_downloaded = COALESCE(excluded.bytes_downloaded, jobs.bytes_downloaded),
               bytes_per_second = COALESCE(excluded.bytes_per_second, jobs.bytes_per_second),
               elapsed_ms = COALESCE(excluded.elapsed_ms, jobs.elapsed_ms)",
            params![
                job.id,
                job.kind,
                job.state,
                job.source_id,
                job.lesson_id,
                job.label,
                job.detail,
                Utc::now().to_rfc3339(),
                progress.started_at,
                progress.completed_at,
                progress.bytes_expected,
                progress.bytes_downloaded,
                progress.bytes_per_second,
                progress.elapsed_ms,
            ],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn completed_job_progress(
    started_at: String,
    started_instant: Instant,
    bytes_expected: Option<i64>,
    bytes_downloaded: Option<i64>,
) -> JobProgress {
    let elapsed_ms = started_instant.elapsed().as_millis().min(i64::MAX as u128) as i64;
    let completed_at = Utc::now().to_rfc3339();
    let bytes_per_second = bytes_downloaded.and_then(|bytes| bytes_per_second(bytes, elapsed_ms));

    JobProgress {
        started_at: Some(started_at),
        completed_at: Some(completed_at),
        bytes_expected,
        bytes_downloaded,
        bytes_per_second,
        elapsed_ms: Some(elapsed_ms),
    }
}

fn bytes_per_second(bytes_downloaded: i64, elapsed_ms: i64) -> Option<f64> {
    if bytes_downloaded <= 0 || elapsed_ms <= 0 {
        return None;
    }
    Some((bytes_downloaded as f64) / ((elapsed_ms as f64) / 1000.0))
}

fn record_downloaded_media(
    connection: &mut Connection,
    data_dir: &Path,
    lesson_id: &str,
    media_path: &Path,
    content_type: &str,
    expected_content_hash: Option<&str>,
) -> Result<(), String> {
    validate_downloaded_media_file(media_path, content_type)?;
    let playback_profile = playback_profile_for_media_file(media_path, content_type)?;
    let media_file_id = format!("media-{}", Uuid::new_v4());
    let hash = hash_file(media_path)?;
    let content_hash = format!("sha256:{hash}");
    let hash_verification_state = verify_downloaded_hash(expected_content_hash, &content_hash)
        .inspect_err(|_| {
            let _ = fs::remove_file(media_path);
        })?;
    let size_bytes = media_path
        .metadata()
        .map_err(|error| error.to_string())?
        .len() as i64;
    let relative_path = media_path
        .strip_prefix(data_dir)
        .map_err(|_| "Downloaded file escaped the app data directory.".to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    let thumbnail_relative_path =
        generate_video_thumbnail(data_dir, media_path, &media_file_id, content_type)
            .ok()
            .flatten();
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;

    transaction
        .execute(
            "UPDATE lessons SET media_file_id = NULL WHERE id = ?1",
            params![lesson_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM media_files WHERE lesson_id = ?1",
            params![lesson_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO media_files
             (id, lesson_id, relative_path, thumbnail_relative_path, content_hash, size_bytes,
              codec, import_status, hash_verification_state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'ready', ?8)",
            params![
                &media_file_id,
                lesson_id,
                &relative_path,
                thumbnail_relative_path,
                &content_hash,
                size_bytes,
                playback_profile,
                hash_verification_state
            ],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE lessons SET media_file_id = ?1 WHERE id = ?2",
            params![&media_file_id, lesson_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE provenance_records
             SET content_hash = ?1
             WHERE lesson_id = ?2",
            params![&content_hash, lesson_id],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}

fn verify_downloaded_hash(
    expected_content_hash: Option<&str>,
    actual_content_hash: &str,
) -> Result<&'static str, String> {
    let Some(expected) = expected_content_hash
        .map(str::trim)
        .filter(|hash| !hash.is_empty())
    else {
        return Ok("not-provided");
    };
    let normalized_expected = expected
        .strip_prefix("sha256:")
        .unwrap_or(expected)
        .to_ascii_lowercase();

    if normalized_expected.len() != 64
        || !normalized_expected
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Ok("unverified");
    }

    let normalized_actual = actual_content_hash
        .strip_prefix("sha256:")
        .unwrap_or(actual_content_hash)
        .to_ascii_lowercase();
    if normalized_expected == normalized_actual {
        Ok("matched")
    } else {
        Err(format!(
            "Downloaded file hash mismatch. Expected sha256:{normalized_expected}, got sha256:{normalized_actual}."
        ))
    }
}

fn parse_retrieval_refs_json(value: &str) -> Vec<RetrievalRef> {
    serde_json::from_str::<Vec<RetrievalRef>>(value).unwrap_or_default()
}

fn retrieval_refs_to_json(refs: &[RetrievalRef]) -> Result<String, String> {
    serde_json::to_string(refs).map_err(|error| error.to_string())
}

fn format_sha256(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with("sha256:") {
        trimmed.to_ascii_lowercase()
    } else {
        format!("sha256:{}", trimmed.to_ascii_lowercase())
    }
}

fn archive_file_lesson(
    identifier: &str,
    file: &serde_json::Value,
    item_title: &str,
    item_description: Option<&str>,
    published_at: Option<&str>,
) -> Option<DiscoveredLesson> {
    let name = json_field_text(file, "name")?;
    if should_skip_archive_file(&name, file) {
        return None;
    }

    let extension = Path::new(&name)
        .extension()
        .and_then(|extension| extension.to_str())?;
    if !is_supported_file_extension(extension) {
        return None;
    }

    let content_type = content_type_from_extension(extension).to_string();
    let title = json_field_text(file, "title")
        .unwrap_or_else(|| archive_file_title(item_title, &name, &content_type));
    let mut description_parts = Vec::new();

    if let Some(format) = json_field_text(file, "format") {
        description_parts.push(format!("Archive.org format: {format}."));
    }
    if let Some(size) = json_field_text(file, "size") {
        description_parts.push(format!("Archive.org file size: {size} bytes."));
    }
    if let Some(description) = item_description {
        description_parts.push(description.to_string());
    }

    let content_hash = json_field_text(file, "sha1")
        .map(|hash| format!("sha1:{hash}"))
        .or_else(|| json_field_text(file, "md5").map(|hash| format!("md5:{hash}")));

    Some(DiscoveredLesson {
        title: clip_text(&title, 140),
        content_type,
        source_url: archive_download_url(identifier, &name),
        retrieval_refs: Vec::new(),
        published_at: published_at.map(str::to_string),
        description: if description_parts.is_empty() {
            None
        } else {
            Some(clip_text(&description_parts.join(" "), 900))
        },
        duration_seconds: None,
        adapter_name: "ArchiveOrgMetadataAdapter".to_string(),
        provenance_note: PUBLIC_SOURCE_PROVENANCE_NOTE.to_string(),
        content_hash,
    })
}

fn should_skip_archive_file(name: &str, file: &serde_json::Value) -> bool {
    let lower_name = name.to_ascii_lowercase();
    let lower_format = json_field_text(file, "format")
        .unwrap_or_default()
        .to_ascii_lowercase();

    lower_name.ends_with("_files.xml")
        || lower_name.ends_with("_meta.xml")
        || lower_name.ends_with("_reviews.xml")
        || lower_name.ends_with("_itemimage.jpg")
        || lower_name.ends_with(".torrent")
        || lower_name.ends_with(".sqlite")
        || lower_format.contains("metadata")
        || lower_format.contains("torrent")
        || lower_format.contains("item tile")
}

fn archive_file_title(item_title: &str, file_name: &str, content_type: &str) -> String {
    let file_stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(|value| value.replace(['_', '-'], " "))
        .map(|value| normalize_text(&value))
        .filter(|value| !value.is_empty());

    match file_stem {
        Some(stem) if stem != item_title => stem,
        _ => format!("{item_title} ({content_type})"),
    }
}

fn json_field_text(value: &serde_json::Value, field: &str) -> Option<String> {
    value.get(field).and_then(json_text)
}

fn json_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => normalize_source_text(text),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Array(values) => {
            let parts = values.iter().filter_map(json_text).collect::<Vec<_>>();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(", "))
            }
        }
        _ => None,
    }
}

fn normalize_source_text(text: &str) -> Option<String> {
    let normalized = if text.contains('<') && text.contains('>') {
        let fragment = Html::parse_fragment(text);
        normalize_text(&fragment.root_element().text().collect::<Vec<_>>().join(" "))
    } else {
        normalize_text(text)
    };

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn feed_lessons(document: &roxmltree::Document<'_>) -> Result<Vec<DiscoveredLesson>, String> {
    let mut lessons = Vec::new();

    for item in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "item")
        .take(100)
    {
        let title = child_text(item, "title").unwrap_or_else(|| "Untitled feed item".to_string());
        let enclosure = rss_enclosure(item);
        let source_url = enclosure
            .as_ref()
            .map(|enclosure| enclosure.url.clone())
            .or_else(|| child_text(item, "link"))
            .ok_or_else(|| "Feed item is missing a link or enclosure URL.".to_string())?;
        let description = child_text(item, "description");
        let content_hash = child_text(item, "contentHash").or_else(|| child_text(item, "hash"));
        let content_type = classify_feed_content(
            &source_url,
            enclosure
                .as_ref()
                .and_then(|enclosure| enclosure.mime_type.as_deref()),
            description.as_deref(),
        );

        lessons.push(DiscoveredLesson {
            title: clip_text(&title, 140),
            content_type,
            source_url,
            retrieval_refs: Vec::new(),
            published_at: child_text(item, "pubDate"),
            description,
            duration_seconds: child_text(item, "duration")
                .and_then(|duration| duration.parse::<i64>().ok()),
            adapter_name: "FeedAdapter".to_string(),
            provenance_note: PUBLIC_SOURCE_PROVENANCE_NOTE.to_string(),
            content_hash,
        });
    }

    if !lessons.is_empty() {
        return Ok(lessons);
    }

    for entry in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "entry")
        .take(100)
    {
        let title = child_text(entry, "title").unwrap_or_else(|| "Untitled feed entry".to_string());
        let atom_link = atom_link(entry);
        let source_url = atom_link
            .as_ref()
            .map(|link| link.url.clone())
            .or_else(|| child_text(entry, "id"))
            .ok_or_else(|| "Atom entry is missing a link or id.".to_string())?;
        let description = child_text(entry, "summary").or_else(|| child_text(entry, "content"));
        let content_type = classify_feed_content(
            &source_url,
            atom_link
                .as_ref()
                .and_then(|link| link.mime_type.as_deref()),
            description.as_deref(),
        );

        lessons.push(DiscoveredLesson {
            title: clip_text(&title, 140),
            content_type,
            source_url,
            retrieval_refs: Vec::new(),
            published_at: child_text(entry, "published").or_else(|| child_text(entry, "updated")),
            description,
            duration_seconds: child_text(entry, "duration")
                .and_then(|duration| duration.parse::<i64>().ok()),
            adapter_name: "FeedAdapter".to_string(),
            provenance_note: PUBLIC_SOURCE_PROVENANCE_NOTE.to_string(),
            content_hash: child_text(entry, "contentHash").or_else(|| child_text(entry, "hash")),
        });
    }

    Ok(lessons)
}

fn feed_title(document: &roxmltree::Document<'_>) -> Option<String> {
    document
        .descendants()
        .find(|node| {
            node.is_element()
                && matches!(node.tag_name().name(), "channel" | "feed")
                && child_text(*node, "title").is_some()
        })
        .and_then(|node| child_text(node, "title"))
        .map(|title| clip_text(&title, 90))
}

fn xml_feed_format(document: &roxmltree::Document<'_>) -> String {
    if document
        .root_element()
        .tag_name()
        .name()
        .eq_ignore_ascii_case("feed")
    {
        "atom".to_string()
    } else {
        "rss".to_string()
    }
}

fn child_text(node: roxmltree::Node<'_, '_>, child_name: &str) -> Option<String> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == child_name)
        .and_then(|child| child.text())
        .map(normalize_text)
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone)]
struct FeedLink {
    url: String,
    mime_type: Option<String>,
}

fn rss_enclosure(node: roxmltree::Node<'_, '_>) -> Option<FeedLink> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == "enclosure")
        .and_then(|child| {
            let url = child.attribute("url")?.to_string();
            if url.is_empty() {
                return None;
            }

            Some(FeedLink {
                url,
                mime_type: child.attribute("type").map(str::to_string),
            })
        })
}

fn atom_link(node: roxmltree::Node<'_, '_>) -> Option<FeedLink> {
    node.children()
        .filter(|child| child.is_element() && child.tag_name().name() == "link")
        .filter(|child| {
            child
                .attribute("rel")
                .map(|rel| matches!(rel, "alternate" | "enclosure"))
                .unwrap_or(true)
        })
        .filter_map(|child| {
            let url = child.attribute("href")?.to_string();
            if url.is_empty() {
                return None;
            }

            Some(FeedLink {
                url,
                mime_type: child.attribute("type").map(str::to_string),
            })
        })
        .max_by_key(|link| {
            if link
                .mime_type
                .as_deref()
                .is_some_and(is_file_backed_mime_type)
            {
                1
            } else {
                0
            }
        })
}

fn fetch_text(client: &Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("Could not fetch {url}: {error}"))?;
    let status = response.status();

    if !status.is_success() {
        return Err(format!("Could not fetch {url}: HTTP {status}."));
    }

    response
        .text()
        .map_err(|error| format!("Could not read response from {url}: {error}"))
}

fn fetch_json(client: &Client, url: &str) -> Result<serde_json::Value, String> {
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("Could not fetch {url}: {error}"))?;
    let status = response.status();

    if !status.is_success() {
        return Err(format!("Could not fetch {url}: HTTP {status}."));
    }

    let body = response
        .text()
        .map_err(|error| format!("Could not read response from {url}: {error}"))?;

    serde_json::from_str(&body).map_err(|error| format!("Could not parse JSON from {url}: {error}"))
}

fn normalize_source_input(input: &str) -> Result<String, String> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return Err("Source URL is required.".to_string());
    }

    let with_scheme = if let Some(username) = trimmed.strip_prefix('@') {
        format!("https://t.me/{username}")
    } else if trimmed.starts_with("t.me/") || trimmed.starts_with("telegram.me/") {
        format!("https://{trimmed}")
    } else if trimmed.starts_with("lbry://") {
        trimmed.to_string()
    } else if trimmed.starts_with("www.")
        || trimmed.starts_with("youtube.com/")
        || trimmed.starts_with("archive.org/")
        || trimmed.starts_with("rumble.com/")
        || trimmed.starts_with("odysee.com/")
        || trimmed.starts_with("x.com/")
        || trimmed.starts_with("twitter.com/")
    {
        format!("https://{trimmed}")
    } else {
        trimmed.to_string()
    };

    if with_scheme.starts_with("lbry://") || is_nostr_reference(&with_scheme) {
        return Ok(with_scheme);
    }

    Url::parse(&with_scheme)
        .map(|url| url.to_string())
        .map_err(|error| format!("Source must be a valid URL: {error}"))
}

fn parse_archive_org_identifier(source_url: &str) -> Option<String> {
    let url = Url::parse(source_url).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();

    if host != "archive.org" && host != "www.archive.org" {
        return None;
    }

    let segments = url
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let identifier = match segments.as_slice() {
        ["details", identifier, ..]
        | ["download", identifier, ..]
        | ["metadata", identifier, ..] => *identifier,
        _ => return None,
    };

    if identifier == "." || identifier == ".." || identifier.contains('/') {
        return None;
    }

    Some(identifier.to_string())
}

fn archive_metadata_url(identifier: &str) -> String {
    format!(
        "https://archive.org/metadata/{}",
        encode_url_path_segment(identifier)
    )
}

fn archive_download_url(identifier: &str, file_name: &str) -> String {
    let encoded_name = file_name
        .split('/')
        .map(encode_url_path_segment)
        .collect::<Vec<_>>()
        .join("/");

    format!(
        "https://archive.org/download/{}/{encoded_name}",
        encode_url_path_segment(identifier)
    )
}

fn encode_url_path_segment(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (*byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn normalize_feed_url(source_url: &str) -> String {
    let Ok(url) = Url::parse(source_url) else {
        return source_url.to_string();
    };

    let Some(host) = url.host_str().map(|host| host.to_ascii_lowercase()) else {
        return source_url.to_string();
    };

    if !host.ends_with("youtube.com") && host != "youtu.be" {
        return source_url.to_string();
    }

    if url.path().contains("/feeds/videos.xml") {
        return source_url.to_string();
    }

    if let Some(list_id) = url.query_pairs().find_map(|(key, value)| {
        if key == "list" {
            Some(value.into_owned())
        } else {
            None
        }
    }) {
        return format!("https://www.youtube.com/feeds/videos.xml?playlist_id={list_id}");
    }

    let segments = url
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    if segments.len() >= 2 && segments[0] == "channel" {
        return format!(
            "https://www.youtube.com/feeds/videos.xml?channel_id={}",
            segments[1]
        );
    }

    source_url.to_string()
}

fn parse_telegram_source(source_url: &str) -> Option<TelegramSource> {
    let url = Url::parse(source_url).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();

    if host != "t.me" && host != "telegram.me" {
        return None;
    }

    let segments = url
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let (username, post_id) = match segments.as_slice() {
        ["s", username, post_id, ..] => ((*username).to_string(), Some((*post_id).to_string())),
        ["s", username] => ((*username).to_string(), None),
        [username, post_id, ..] => ((*username).to_string(), Some((*post_id).to_string())),
        [username] => ((*username).to_string(), None),
        _ => return None,
    };

    if username == "c" || username == "joinchat" || username.starts_with('+') || username.len() < 3
    {
        return None;
    }

    Some(TelegramSource { username, post_id })
}

fn is_probably_telegram_invite(source_url: &str) -> bool {
    let Ok(url) = Url::parse(source_url) else {
        return false;
    };
    let Some(host) = url.host_str().map(|host| host.to_ascii_lowercase()) else {
        return false;
    };

    if host != "t.me" && host != "telegram.me" {
        return false;
    }

    url.path().contains("/joinchat") || url.path().contains("/+") || url.path().starts_with("/c/")
}

fn is_nostr_reference(source_url: &str) -> bool {
    let lower = source_url.to_ascii_lowercase();
    source_url.starts_with("naddr1")
        || source_url.starts_with("nostr:")
        || source_url.starts_with("nostr+")
        || ((source_url.contains('\n') || lower.contains("duroos channel invite"))
            && publisher::channel_ref_has_naddr(source_url))
}

fn platform_for_feed(source_url: &str) -> String {
    let Ok(url) = Url::parse(source_url) else {
        return "rss-feed".to_string();
    };
    let host = url
        .host_str()
        .map(|host| host.to_ascii_lowercase())
        .unwrap_or_default();

    if host.ends_with("youtube.com") || host == "youtu.be" {
        return "youtube".to_string();
    }

    if host.ends_with("archive.org") {
        return "archive-org".to_string();
    }

    if host.ends_with("odysee.com") {
        return "odysee".to_string();
    }

    "rss-feed".to_string()
}

fn direct_source_platform(source_url: &str) -> Option<(&'static str, &'static str)> {
    if source_url.starts_with("lbry://") {
        return Some(("odysee", "video"));
    }

    let url = Url::parse(source_url).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();

    if (host.ends_with("youtube.com") || host == "youtu.be") && !is_youtube_feed_source(&url) {
        return Some(("youtube", "video"));
    }

    if host.ends_with("rumble.com") {
        return Some(("rumble", "video"));
    }

    if host.ends_with("odysee.com") {
        return Some(("odysee", "video"));
    }

    if host == "x.com"
        || host.ends_with(".x.com")
        || host == "twitter.com"
        || host.ends_with(".twitter.com")
    {
        return Some(("x", "post"));
    }

    None
}

fn is_youtube_feed_source(url: &Url) -> bool {
    let Some(host) = url.host_str().map(|host| host.to_ascii_lowercase()) else {
        return false;
    };

    if !host.ends_with("youtube.com") {
        return false;
    }

    if url.path().contains("/feeds/videos.xml") {
        return true;
    }

    let segments = url
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    let has_playlist_id = url.query_pairs().any(|(key, _)| key == "list");
    let has_video_id = url.query_pairs().any(|(key, _)| key == "v");

    matches!(segments.as_slice(), ["playlist", ..]) && has_playlist_id
        || matches!(segments.as_slice(), ["watch", ..]) && has_playlist_id && !has_video_id
        || matches!(segments.as_slice(), ["channel", channel_id, ..] if !channel_id.is_empty())
}

fn is_youtube_feed_source_url(source_url: &str) -> bool {
    Url::parse(source_url)
        .map(|url| is_youtube_feed_source(&url))
        .unwrap_or(false)
}

fn direct_source_platform_label(platform: &str) -> &'static str {
    match platform {
        "youtube" => "YouTube",
        "rumble" => "Rumble",
        "odysee" => "Odysee",
        "x" => "X",
        _ => "Source",
    }
}

fn direct_source_capability(platform: &str) -> SourceCapability {
    match platform {
        "youtube" => capability(
            "supported",
            "limited",
            "limited",
            true,
            "api-key",
            "best-effort",
            "Official API covers metadata; user-initiated downloads depend on permitted content and local yt-dlp.",
        ),
        "rumble" => capability(
            "limited",
            "limited",
            "limited",
            false,
            "none",
            "best-effort",
            "No broad public catalog API is assumed; direct URL downloads are best-effort through local tooling.",
        ),
        "odysee" => capability(
            "limited",
            "limited",
            "limited",
            false,
            "none",
            "best-effort",
            "Odysee/LBRY URLs are tracked as best-effort references; native daemon support is future work.",
        ),
        "x" => capability(
            "limited",
            "limited",
            "limited",
            true,
            "api-key",
            "credential-bound",
            "X post and media metadata are credential-bound; saved URL references remain local.",
        ),
        _ => capability(
            "limited",
            "blocked",
            "blocked",
            false,
            "none",
            "best-effort",
            "This source is kept as a local reference.",
        ),
    }
}

fn direct_source_title(client: &Client, source_url: &str) -> String {
    fetch_text(client, source_url)
        .ok()
        .and_then(|body| {
            let document = Html::parse_document(&body);
            first_document_text(&document, &["title"])
        })
        .map(|title| title.split('|').next().unwrap_or(&title).trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| {
            Url::parse(source_url)
                .ok()
                .and_then(|url| {
                    url.path_segments()
                        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
                        .map(|segment| segment.replace(['-', '_'], " "))
                })
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| {
                    direct_source_platform_label(
                        direct_source_platform(source_url)
                            .map(|(platform, _)| platform)
                            .unwrap_or("source"),
                    )
                    .to_string()
                })
        })
}

fn first_document_text(document: &Html, selectors: &[&str]) -> Option<String> {
    selectors.iter().find_map(|query| {
        let selector = Selector::parse(query).ok()?;
        document
            .select(&selector)
            .next()
            .map(|element| normalize_text(&element.text().collect::<Vec<_>>().join(" ")))
            .filter(|value| !value.is_empty())
    })
}

fn selector(query: &str) -> Selector {
    Selector::parse(query).expect("static CSS selector must parse")
}

fn title_from_text(text: Option<&str>) -> Option<String> {
    text.map(|value| clip_text(value, 110))
        .filter(|value| !value.is_empty())
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip_text(value: &str, max_chars: usize) -> String {
    let normalized = normalize_text(value);
    let character_count = normalized.chars().count();

    if character_count <= max_chars {
        return normalized;
    }

    let keep = max_chars.saturating_sub(3);
    format!("{}...", normalized.chars().take(keep).collect::<String>())
}

fn host_label(source_url: &str) -> String {
    Url::parse(source_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "Imported Feed".to_string())
}

fn stable_suffix(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hasher
        .finalize()
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn collect_source_column(
    connection: &Connection,
    column: &str,
    source_id: &str,
) -> Result<Vec<String>, String> {
    if !matches!(column, "collection_id" | "teacher_id") {
        return Err("Unsupported source column lookup.".to_string());
    }

    let mut statement = connection
        .prepare(&format!(
            "SELECT DISTINCT {column} FROM lessons WHERE source_id = ?1"
        ))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![source_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn collect_source_media_paths(
    connection: &Connection,
    source_id: &str,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT m.relative_path, m.thumbnail_relative_path
             FROM media_files m
             JOIN lessons l ON l.id = m.lesson_id
             WHERE l.source_id = ?1",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![source_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|error| error.to_string())?;

    let records = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(records
        .into_iter()
        .flat_map(|(relative_path, thumbnail_relative_path)| {
            std::iter::once(relative_path).chain(thumbnail_relative_path)
        })
        .collect())
}

fn collect_lesson_media_paths(
    connection: &Connection,
    lesson_id: &str,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT relative_path, thumbnail_relative_path
             FROM media_files
             WHERE lesson_id = ?1",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![lesson_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let records = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(records
        .into_iter()
        .flat_map(|(relative_path, thumbnail_relative_path)| {
            std::iter::once(relative_path).chain(thumbnail_relative_path)
        })
        .collect())
}

#[derive(Debug)]
struct StaleMediaFile {
    relative_path: String,
    path: PathBuf,
    size_bytes: i64,
    category: String,
}

#[derive(Debug)]
struct MediaStorageAuditDetail {
    audit: MediaStorageAudit,
    stale_files: Vec<StaleMediaFile>,
}

fn media_storage_audit_detail(
    connection: &Connection,
    data_dir: &Path,
) -> Result<MediaStorageAuditDetail, String> {
    let referenced_paths = collect_all_media_paths(connection)?
        .into_iter()
        .collect::<HashSet<_>>();
    let library_dir = data_dir.join("library");
    let mut scanned_files = 0_i64;
    let mut referenced_files = 0_i64;
    let mut partial_files = 0_i64;
    let mut stale_files = Vec::new();

    if library_dir.is_dir() {
        for entry in WalkDir::new(&library_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            let Some(relative_path) = relative_library_path(data_dir, path) else {
                continue;
            };
            scanned_files += 1;
            if is_probably_partial_media_file(path) {
                partial_files += 1;
            }
            if referenced_paths.contains(&relative_path) {
                referenced_files += 1;
                continue;
            }
            let size_bytes = path
                .metadata()
                .map(|metadata| metadata.len().min(i64::MAX as u64) as i64)
                .unwrap_or(0);
            stale_files.push(StaleMediaFile {
                category: stale_media_file_category(&relative_path, path),
                relative_path,
                path: path.to_path_buf(),
                size_bytes,
            });
        }
    }

    stale_files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let stale_bytes = stale_files.iter().map(|file| file.size_bytes).sum::<i64>();
    let stale_samples = stale_files
        .iter()
        .take(5)
        .map(|file| file.relative_path.clone())
        .collect::<Vec<_>>();
    let stale_items = stale_files
        .iter()
        .map(|file| MediaStorageStaleItem {
            relative_path: file.relative_path.clone(),
            size_bytes: file.size_bytes,
            category: file.category.clone(),
        })
        .collect::<Vec<_>>();
    let messages = if stale_files.is_empty() {
        vec!["No stale app-library media files were found.".to_string()]
    } else {
        vec![format!(
            "Found {} stale app-library file(s), using {}.",
            stale_files.len(),
            format_bytes(stale_bytes)
        )]
    };

    Ok(MediaStorageAuditDetail {
        audit: MediaStorageAudit {
            scanned_files,
            referenced_files,
            stale_files: stale_files.len() as i64,
            stale_bytes,
            partial_files,
            stale_samples,
            stale_items,
            messages,
        },
        stale_files,
    })
}

fn stale_media_file_category(relative_path: &str, path: &Path) -> String {
    if is_probably_partial_media_file(path) {
        return "partial-fragment".to_string();
    }
    if relative_path.starts_with("library/downloads/") {
        return "old-source-download".to_string();
    }
    "unreferenced".to_string()
}

fn validate_media_cleanup_mode(mode: &str) -> Result<String, String> {
    let normalized = mode.trim();
    match normalized {
        "partial-fragments" | "old-source-downloads" | "all-stale" => Ok(normalized.to_string()),
        _ => Err(
            "Storage cleanup mode must be partial-fragments, old-source-downloads, or all-stale."
                .to_string(),
        ),
    }
}

fn cleanup_mode_matches(mode: &str, category: &str) -> bool {
    match mode {
        "partial-fragments" => category == "partial-fragment",
        "old-source-downloads" => category == "old-source-download",
        "all-stale" => true,
        _ => false,
    }
}

fn cleanup_mode_label(mode: &str) -> &'static str {
    match mode {
        "partial-fragments" => "partial-fragment",
        "old-source-downloads" => "old source-download",
        "all-stale" => "all-stale",
        _ => "matching",
    }
}

fn collect_all_media_paths(connection: &Connection) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare("SELECT relative_path, thumbnail_relative_path FROM media_files")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let records = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(records
        .into_iter()
        .flat_map(|(relative_path, thumbnail_relative_path)| {
            std::iter::once(relative_path).chain(thumbnail_relative_path)
        })
        .collect())
}

fn relative_library_path(data_dir: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(data_dir)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .filter(|relative| relative.starts_with("library/"))
}

fn format_bytes(bytes: i64) -> String {
    let bytes = bytes.max(0) as f64;
    if bytes >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} GB", bytes / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024.0 * 1024.0 {
        format!("{:.1} MB", bytes / (1024.0 * 1024.0))
    } else if bytes >= 1024.0 {
        format!("{:.1} KB", bytes / 1024.0)
    } else {
        format!("{} B", bytes as i64)
    }
}

fn format_duration_ms(elapsed_ms: i64) -> String {
    if elapsed_ms < 1000 {
        return format!("{} ms", elapsed_ms.max(0));
    }
    let seconds = elapsed_ms as f64 / 1000.0;
    if seconds < 60.0 {
        return format!("{seconds:.1}s");
    }
    let minutes = (seconds / 60.0).floor() as i64;
    let remainder = (seconds as i64) % 60;
    format!("{minutes}m {remainder}s")
}

fn duplicate_lesson_title_for_hash(
    transaction: &Transaction<'_>,
    content_hash: &str,
) -> Result<Option<String>, String> {
    transaction
        .query_row(
            "SELECT l.title
             FROM lessons l
             LEFT JOIN media_files m ON m.lesson_id = l.id
             LEFT JOIN provenance_records p ON p.lesson_id = l.id
             WHERE m.content_hash = ?1 OR p.content_hash = ?1
             ORDER BY l.published_at DESC, l.title ASC
             LIMIT 1",
            params![content_hash],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn discovered_duplicate_lesson_title(
    transaction: &Transaction<'_>,
    lesson: &DiscoveredLesson,
) -> Result<Option<String>, String> {
    if let Some(title) = transaction
        .query_row(
            "SELECT title FROM lessons WHERE source_url = ?1 LIMIT 1",
            params![lesson.source_url],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    {
        return Ok(Some(title));
    }

    if let Some(content_hash) = lesson
        .content_hash
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(title) = duplicate_lesson_title_for_hash(transaction, content_hash)? {
            return Ok(Some(title));
        }
    }

    if let Some(duration_seconds) = lesson.duration_seconds {
        let normalized_title = normalize_text(&lesson.title).to_ascii_lowercase();

        if normalized_title.chars().count() >= 8 {
            return transaction
                .query_row(
                    "SELECT title
                     FROM lessons
                     WHERE lower(trim(title)) = ?1
                       AND content_type = ?2
                       AND duration_seconds = ?3
                     LIMIT 1",
                    params![normalized_title, lesson.content_type, duration_seconds],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| error.to_string());
        }
    }

    Ok(None)
}

fn source_download_plan(
    connection: &Connection,
    data_dir: &Path,
    source_id: &str,
) -> Result<(Vec<DownloadLesson>, i64), String> {
    let mut statement = connection
        .prepare(
            "SELECT l.id, l.title, l.content_type, l.source_url, l.retrieval_refs_json, p.content_hash,
                    l.media_file_id, m.relative_path, m.import_status
             FROM lessons l
             LEFT JOIN media_files m ON m.id = l.media_file_id
             LEFT JOIN provenance_records p ON p.lesson_id = l.id
             WHERE l.source_id = ?1
               AND l.content_type IN ('video', 'audio', 'pdf', 'post')
             ORDER BY l.published_at DESC, l.title ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![source_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
            ))
        })
        .map_err(|error| error.to_string())?;

    let mut lessons = Vec::new();
    let mut skipped = 0;

    for row in rows {
        let (
            id,
            title,
            content_type,
            source_url,
            retrieval_refs_json,
            expected_content_hash,
            media_file_id,
            relative_path,
            import_status,
        ) = row.map_err(|error| error.to_string())?;

        if content_type == "post" {
            skipped += 1;
            continue;
        }

        let has_ready_media = media_file_id.is_some()
            && import_status.as_deref() == Some("ready")
            && relative_path
                .as_deref()
                .and_then(|path| resolve_library_media_path(data_dir, path))
                .filter(|path| validate_downloaded_media_file(path, &content_type).is_ok())
                .is_some();

        if has_ready_media {
            skipped += 1;
            continue;
        }

        lessons.push(DownloadLesson {
            id,
            title,
            content_type,
            source_url,
            retrieval_refs: parse_retrieval_refs_json(&retrieval_refs_json),
            expected_content_hash,
            has_invalid_media_record: media_file_id.is_some(),
        });
    }

    Ok((lessons, skipped))
}

fn detach_lesson_media_records(
    connection: &mut Connection,
    data_dir: &Path,
    lesson_id: &str,
) -> Result<(), String> {
    let media_paths = collect_lesson_media_paths(connection, lesson_id)?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE lessons SET media_file_id = NULL WHERE id = ?1",
            params![lesson_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM media_files WHERE lesson_id = ?1",
            params![lesson_id],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    remove_media_files_from_disk(data_dir, &media_paths);
    Ok(())
}

fn collection_owned_by_source(
    transaction: &Transaction<'_>,
    collection_id: &str,
    source_id: &str,
) -> Result<bool, String> {
    let source_ids_json: Option<String> = transaction
        .query_row(
            "SELECT source_ids_json FROM collections WHERE id = ?1",
            params![collection_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let source_ids = source_ids_json
        .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
        .unwrap_or_default();

    Ok(source_ids.len() == 1 && source_ids[0] == source_id)
}

fn is_default_source_id(source_id: &str) -> bool {
    DEFAULT_SOURCE_IDS.contains(&source_id)
}

fn remove_media_files_from_disk(data_dir: &Path, relative_paths: &[String]) -> i64 {
    relative_paths
        .iter()
        .filter_map(|relative_path| resolve_library_media_path(data_dir, relative_path))
        .filter(|path| path.is_file())
        .filter(|path| fs::remove_file(path).is_err())
        .count() as i64
}

pub(crate) fn resolve_library_media_path(data_dir: &Path, relative_path: &str) -> Option<PathBuf> {
    let relative = Path::new(relative_path);

    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return None;
    }

    let library_dir = data_dir.join("library");
    let candidate = data_dir.join(relative);

    if candidate.starts_with(&library_dir) {
        Some(candidate)
    } else {
        None
    }
}

fn find_yt_dlp_command() -> Result<YtDlpCommand, String> {
    let mut candidates = vec![
        YtDlpCommand {
            program: "yt-dlp".to_string(),
            args: Vec::new(),
        },
        YtDlpCommand {
            program: "yt-dlp.exe".to_string(),
            args: Vec::new(),
        },
        YtDlpCommand {
            program: "/opt/homebrew/bin/yt-dlp".to_string(),
            args: Vec::new(),
        },
        YtDlpCommand {
            program: "/usr/local/bin/yt-dlp".to_string(),
            args: Vec::new(),
        },
        YtDlpCommand {
            program: "/usr/bin/yt-dlp".to_string(),
            args: Vec::new(),
        },
        YtDlpCommand {
            program: "python3".to_string(),
            args: vec!["-m".to_string(), "yt_dlp".to_string()],
        },
        YtDlpCommand {
            program: "python".to_string(),
            args: vec!["-m".to_string(), "yt_dlp".to_string()],
        },
    ];

    if cfg!(target_os = "windows") {
        if let Some(program_files) = env::var_os("ProgramFiles") {
            candidates.push(YtDlpCommand {
                program: PathBuf::from(program_files)
                    .join("yt-dlp")
                    .join("yt-dlp.exe")
                    .to_string_lossy()
                    .to_string(),
                args: Vec::new(),
            });
        }
    }

    for candidate in candidates {
        let probe = Command::new(&candidate.program)
            .args(&candidate.args)
            .arg("--version")
            .output();

        if probe.map(|output| output.status.success()).unwrap_or(false) {
            return Ok(candidate);
        }
    }

    Err("yt-dlp was not found. Install yt-dlp locally, then retry downloading media for this source.".to_string())
}

fn download_lesson_media(
    client: &Client,
    yt_dlp: &mut Option<YtDlpCommand>,
    lesson: &DownloadLesson,
    lesson_dir: &Path,
    context: DownloadMediaContext<'_>,
) -> Result<PathBuf, String> {
    if !is_file_backed_content_type(&lesson.content_type) {
        return Err(
            "This item is a saved post and does not have a downloadable media file.".to_string(),
        );
    }

    let candidates = retrieval_download_candidates(lesson);
    if !candidates.is_empty() {
        return download_from_retrieval_candidates(
            client,
            &candidates,
            lesson_dir,
            &lesson.content_type,
            context.expected_content_hash,
            context.progress,
        );
    }

    if is_direct_file_url(&lesson.source_url) {
        let candidate = RetrievalDownloadCandidate {
            url: lesson.source_url.clone(),
            label: "source_url".to_string(),
            file_name: None,
        };
        return download_from_retrieval_candidates(
            client,
            &[candidate],
            lesson_dir,
            &lesson.content_type,
            context.expected_content_hash,
            context.progress,
        );
    }

    let command = match yt_dlp {
        Some(command) => command.clone(),
        None => {
            let command = find_yt_dlp_command()?;
            *yt_dlp = Some(command.clone());
            command
        }
    };

    download_lesson_with_yt_dlp(
        &command,
        &lesson.source_url,
        lesson_dir,
        context.data_dir,
        context.cookies_file,
        &lesson.content_type,
    )
}

fn retrieval_download_candidates(lesson: &DownloadLesson) -> Vec<RetrievalDownloadCandidate> {
    let mut http_candidates = Vec::new();
    let mut ipfs_candidates = Vec::new();

    for retrieval_ref in &lesson.retrieval_refs {
        match retrieval_ref.kind.as_str() {
            "direct-url" | "enclosure-url" => {
                let Some(url) = retrieval_ref.url.as_ref() else {
                    continue;
                };
                if !is_safe_http_url(url) {
                    continue;
                }
                http_candidates.push(RetrievalDownloadCandidate {
                    url: url.clone(),
                    label: format!("{} {}", retrieval_ref.kind, url),
                    file_name: fallback_download_file_name(url, &lesson.content_type),
                });
            }
            "ipfs-cid" => {
                let (Some(cid), Some(gateway_url)) = (
                    retrieval_ref.cid.as_ref(),
                    retrieval_ref.gateway_url.as_ref(),
                ) else {
                    continue;
                };
                if !is_safe_http_url(gateway_url) {
                    continue;
                }
                ipfs_candidates.push(RetrievalDownloadCandidate {
                    url: ipfs_gateway_url(gateway_url, cid),
                    label: format!("ipfs-cid {cid}"),
                    file_name: Some(format!(
                        "ipfs-{}.{}",
                        safe_path_segment(cid),
                        extension_for_content_type(&lesson.content_type)
                    )),
                });
            }
            _ => {}
        }
    }

    dedupe_download_candidates(http_candidates.into_iter().chain(ipfs_candidates).collect())
}

fn dedupe_download_candidates(
    candidates: Vec<RetrievalDownloadCandidate>,
) -> Vec<RetrievalDownloadCandidate> {
    let mut output = Vec::new();
    for candidate in candidates {
        if !output
            .iter()
            .any(|existing: &RetrievalDownloadCandidate| existing.url == candidate.url)
        {
            output.push(candidate);
        }
    }
    output
}

fn download_from_retrieval_candidates(
    client: &Client,
    candidates: &[RetrievalDownloadCandidate],
    lesson_dir: &Path,
    content_type: &str,
    expected_content_hash: Option<&str>,
    mut progress: Option<DirectDownloadProgress<'_>>,
) -> Result<PathBuf, String> {
    let mut attempts = Vec::new();

    for candidate in candidates {
        match download_direct_file_candidate_with_progress(
            client,
            candidate,
            lesson_dir,
            content_type,
            progress.take(),
        ) {
            Ok(path) => match verify_candidate_download_hash(expected_content_hash, &path) {
                Ok(()) => return Ok(path),
                Err(error) => {
                    let _ = fs::remove_file(&path);
                    attempts.push(format!("{}: {error}", candidate.label));
                }
            },
            Err(error) => attempts.push(format!("{}: {error}", candidate.label)),
        }
    }

    Err(format!(
        "Every retrieval path failed for this lesson. Tried: {}",
        attempts.join("; ")
    ))
}

fn verify_candidate_download_hash(
    expected_content_hash: Option<&str>,
    media_path: &Path,
) -> Result<(), String> {
    if expected_content_hash
        .map(str::trim)
        .filter(|hash| !hash.is_empty())
        .is_none()
    {
        return Ok(());
    }

    let actual_hash = format!("sha256:{}", hash_file(media_path)?);
    verify_downloaded_hash(expected_content_hash, &actual_hash).map(|_| ())
}

fn is_direct_file_url(source_url: &str) -> bool {
    extension_from_url(source_url).is_some()
}

#[cfg(test)]
fn download_direct_file(
    client: &Client,
    source_url: &str,
    lesson_dir: &Path,
    content_type: &str,
) -> Result<PathBuf, String> {
    download_direct_file_with_progress(client, source_url, lesson_dir, content_type, None)
}

#[cfg(test)]
fn download_direct_file_with_progress(
    client: &Client,
    source_url: &str,
    lesson_dir: &Path,
    content_type: &str,
    mut progress: Option<DirectDownloadProgress<'_>>,
) -> Result<PathBuf, String> {
    let candidate = RetrievalDownloadCandidate {
        url: source_url.to_string(),
        label: source_url.to_string(),
        file_name: None,
    };
    download_direct_file_candidate_with_progress(
        client,
        &candidate,
        lesson_dir,
        content_type,
        progress.take(),
    )
}

fn download_direct_file_candidate_with_progress(
    client: &Client,
    candidate: &RetrievalDownloadCandidate,
    lesson_dir: &Path,
    content_type: &str,
    mut progress: Option<DirectDownloadProgress<'_>>,
) -> Result<PathBuf, String> {
    let mut response = client
        .get(&candidate.url)
        .send()
        .map_err(|error| format!("Could not fetch media file: {error}"))?;
    let status = response.status();

    if !status.is_success() {
        return Err(format!("Could not fetch media file: HTTP {status}."));
    }
    validate_response_media_type(&response, content_type)?;
    let expected_bytes = response
        .content_length()
        .map(|bytes| bytes.min(i64::MAX as u64) as i64);
    if let Some(progress) = progress.as_mut() {
        progress.total_bytes = expected_bytes;
        let _ = record_direct_download_progress(progress, 0);
    }

    let file_name = candidate
        .file_name
        .clone()
        .unwrap_or_else(|| direct_download_file_name(&candidate.url));
    let destination = lesson_dir.join(file_name);
    let partial_destination = destination.with_extension(format!(
        "{}part",
        destination
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!("{extension}."))
            .unwrap_or_default()
    ));
    let mut file = fs::File::create(&partial_destination).map_err(|error| error.to_string())?;
    let mut bytes_downloaded = 0_i64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = match response.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) => {
                let _ = fs::remove_file(&partial_destination);
                return Err(error.to_string());
            }
        };
        if let Err(error) = file.write_all(&buffer[..count]) {
            let _ = fs::remove_file(&partial_destination);
            return Err(error.to_string());
        }
        bytes_downloaded = bytes_downloaded.saturating_add(count as i64);
        if let Some(progress) = progress.as_mut() {
            let threshold = progress.last_recorded_bytes.saturating_add(256 * 1024);
            if bytes_downloaded >= threshold || Some(bytes_downloaded) == expected_bytes {
                let _ = record_direct_download_progress(progress, bytes_downloaded);
            }
        }
    }
    if let Err(error) = fs::rename(&partial_destination, &destination) {
        let _ = fs::remove_file(&partial_destination);
        return Err(error.to_string());
    }
    if let Err(error) = validate_downloaded_media_file(&destination, content_type) {
        let _ = fs::remove_file(&destination);
        return Err(error);
    }

    Ok(destination)
}

fn record_direct_download_progress(
    progress: &mut DirectDownloadProgress<'_>,
    bytes: i64,
) -> Result<(), String> {
    progress.last_recorded_bytes = bytes;
    let detail = match progress.total_bytes {
        Some(total) if total > 0 => format!(
            "Downloaded {} of {}.",
            format_bytes(bytes),
            format_bytes(total)
        ),
        _ => format!("Downloaded {}.", format_bytes(bytes)),
    };
    upsert_job_with_progress(
        progress.connection,
        JobUpdate {
            id: progress.job_id,
            kind: "download",
            state: "running",
            source_id: Some(progress.source_id),
            lesson_id: Some(progress.lesson_id),
            label: progress.label,
            detail: &detail,
        },
        Some(JobProgress {
            started_at: Some(progress.started_at.to_string()),
            completed_at: None,
            bytes_expected: progress.total_bytes,
            bytes_downloaded: Some(bytes),
            bytes_per_second: None,
            elapsed_ms: None,
        }),
    )
}

fn direct_download_file_name(source_url: &str) -> String {
    Url::parse(source_url)
        .ok()
        .and_then(|url| {
            Path::new(url.path())
                .file_name()
                .and_then(|value| value.to_str())
                .map(safe_path_segment)
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("download-{}", Uuid::new_v4()))
}

fn fallback_download_file_name(source_url: &str, content_type: &str) -> Option<String> {
    if extension_from_url(source_url).is_some() {
        None
    } else {
        Some(format!(
            "download-{}.{}",
            stable_suffix(source_url),
            extension_for_content_type(content_type)
        ))
    }
}

fn extension_for_content_type(content_type: &str) -> &'static str {
    match content_type {
        "audio" => "mp3",
        "pdf" => "pdf",
        _ => "mp4",
    }
}

fn ipfs_gateway_url(gateway_url: &str, cid: &str) -> String {
    let base = gateway_url.trim().trim_end_matches('/');
    if base.ends_with(cid) {
        base.to_string()
    } else if base.ends_with("/ipfs") {
        format!("{base}/{cid}")
    } else {
        format!("{base}/ipfs/{cid}")
    }
}

fn download_lesson_with_yt_dlp(
    yt_dlp: &YtDlpCommand,
    source_url: &str,
    lesson_dir: &Path,
    data_dir: &Path,
    cookies_file: Option<&Path>,
    content_type: &str,
) -> Result<PathBuf, String> {
    let output_template = lesson_dir.join("%(title).200B-%(id)s.%(ext)s");
    let mut command = Command::new(&yt_dlp.program);
    command
        .args(&yt_dlp.args)
        .arg("--no-playlist")
        .arg("--restrict-filenames")
        .arg("--no-progress")
        .arg("--continue")
        .arg("--merge-output-format")
        .arg("mp4");

    match content_type {
        "video" => {
            command.arg("-f").arg(yt_dlp_video_format_selector());
        }
        "audio" => {
            command.arg("-f").arg(yt_dlp_audio_format_selector());
        }
        _ => {}
    }

    if let Some(cookies_file) = cookies_file {
        command.arg("--cookies").arg(cookies_file);
    }

    let output = command
        .arg("-o")
        .arg(&output_template)
        .arg(source_url)
        .output()
        .map_err(|error| format!("Could not start yt-dlp: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "yt-dlp failed: {} {}",
            output_summary(&output.stderr),
            yt_dlp_auth_hint(cookies_file, data_dir)
        ));
    }

    let mut media_files = collect_completed_media_files(lesson_dir, content_type);
    media_files.sort_by_key(|path| {
        path.metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
    });

    media_files
        .pop()
        .ok_or_else(|| {
            format!(
                "yt-dlp finished but no playable {content_type} file was produced. The output folder may contain only incomplete adaptive fragments or unsupported media."
            )
        })
}

fn yt_dlp_video_format_selector() -> &'static str {
    "best[ext=mp4][vcodec^=avc1][acodec^=mp4a]/bestvideo[ext=mp4][vcodec^=avc1]+bestaudio[ext=m4a][acodec^=mp4a]"
}

fn yt_dlp_audio_format_selector() -> &'static str {
    "bestaudio[ext=m4a][acodec^=mp4a]/bestaudio[ext=mp3]/bestaudio/best"
}

fn ensure_webkit_playable_media(
    media_path: &Path,
    content_type: &str,
    output_dir: &Path,
) -> Result<PathBuf, String> {
    if !matches!(content_type, "video" | "audio") {
        return Ok(media_path.to_path_buf());
    }

    let profile = playback_profile_for_media_file(media_path, content_type)?;
    if profile.starts_with("webkit-compatible:") {
        return Ok(media_path.to_path_buf());
    }

    let Some(ffmpeg) = find_media_tool("ffmpeg") else {
        return Ok(media_path.to_path_buf());
    };

    match transcode_media_for_webkit(&ffmpeg, media_path, content_type, output_dir) {
        Ok(transcoded_path) => {
            let transcoded_profile =
                playback_profile_for_media_file(&transcoded_path, content_type)?;
            if transcoded_profile.starts_with("webkit-compatible:") {
                Ok(transcoded_path)
            } else {
                let _ = fs::remove_file(&transcoded_path);
                Ok(media_path.to_path_buf())
            }
        }
        Err(_) => Ok(media_path.to_path_buf()),
    }
}

fn generate_video_thumbnail(
    data_dir: &Path,
    media_path: &Path,
    media_file_id: &str,
    content_type: &str,
) -> Result<Option<String>, String> {
    if content_type != "video" {
        return Ok(None);
    }

    let Some(ffmpeg) = find_media_tool("ffmpeg") else {
        return Ok(None);
    };

    let thumbnail_dir = data_dir.join("library").join("thumbnails");
    fs::create_dir_all(&thumbnail_dir).map_err(|error| error.to_string())?;
    let file_stem = safe_path_segment(media_file_id);
    let destination = thumbnail_dir.join(format!("{file_stem}.jpg"));

    if destination
        .metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
    {
        return destination
            .strip_prefix(data_dir)
            .map(|path| Some(path.to_string_lossy().replace('\\', "/")))
            .map_err(|_| "Generated thumbnail escaped the app data directory.".to_string());
    }

    let partial_destination = thumbnail_dir.join(format!("{file_stem}.part.jpg"));
    let output = Command::new(ffmpeg)
        .arg("-nostdin")
        .arg("-y")
        .args(["-ss", "00:00:01"])
        .arg("-i")
        .arg(media_path)
        .args(["-frames:v", "1"])
        .args(["-vf", "scale=640:-2"])
        .args(["-q:v", "4"])
        .arg(&partial_destination)
        .output()
        .map_err(|error| format!("Could not start ffmpeg for thumbnail generation: {error}"))?;

    if !output.status.success() || !partial_destination.is_file() {
        let _ = fs::remove_file(&partial_destination);
        return Ok(None);
    }

    if let Err(error) = fs::rename(&partial_destination, &destination) {
        let _ = fs::remove_file(&partial_destination);
        return Err(error.to_string());
    }

    destination
        .strip_prefix(data_dir)
        .map(|path| Some(path.to_string_lossy().replace('\\', "/")))
        .map_err(|_| "Generated thumbnail escaped the app data directory.".to_string())
}

fn transcode_media_for_webkit(
    ffmpeg: &str,
    media_path: &Path,
    content_type: &str,
    output_dir: &Path,
) -> Result<PathBuf, String> {
    let stem = media_path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(safe_path_segment)
        .unwrap_or_else(|| "media".to_string());
    let destination = match content_type {
        "video" => output_dir.join(format!("{stem}-webkit.mp4")),
        "audio" => output_dir.join(format!("{stem}-webkit.m4a")),
        _ => return Ok(media_path.to_path_buf()),
    };
    let partial_destination = destination.with_extension(format!(
        "{}part",
        destination
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!("{extension}."))
            .unwrap_or_default()
    ));

    let mut command = Command::new(ffmpeg);
    command.arg("-nostdin").arg("-y").arg("-i").arg(media_path);

    match content_type {
        "video" => {
            command
                .args(["-map", "0:v:0", "-map", "0:a:0?"])
                .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "23"])
                .args(["-pix_fmt", "yuv420p"])
                .args(["-c:a", "aac", "-b:a", "128k"])
                .args(["-movflags", "+faststart"]);
        }
        "audio" => {
            command.arg("-vn").args(["-c:a", "aac", "-b:a", "128k"]);
        }
        _ => {}
    }

    let output = command
        .arg(&partial_destination)
        .output()
        .map_err(|error| format!("Could not start ffmpeg: {error}"))?;

    if !output.status.success() {
        let _ = fs::remove_file(&partial_destination);
        return Err(format!(
            "ffmpeg failed while normalizing media: {}",
            output_summary(&output.stderr)
        ));
    }

    if let Err(error) = fs::rename(&partial_destination, &destination) {
        let _ = fs::remove_file(&partial_destination);
        return Err(error.to_string());
    }

    Ok(destination)
}

fn find_media_tool(tool_name: &str) -> Option<String> {
    let mut candidates = vec![
        tool_name.to_string(),
        format!("{tool_name}.exe"),
        format!("/opt/homebrew/bin/{tool_name}"),
        format!("/usr/local/bin/{tool_name}"),
        format!("/usr/bin/{tool_name}"),
    ];

    if cfg!(target_os = "windows") {
        if let Some(program_files) = env::var_os("ProgramFiles") {
            candidates.push(
                PathBuf::from(program_files)
                    .join("ffmpeg")
                    .join("bin")
                    .join(format!("{tool_name}.exe"))
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }

    candidates.into_iter().find(|candidate| {
        Command::new(candidate)
            .arg("-version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    })
}

fn native_player_command_for_app(app: &AppHandle) -> Option<NativePlayerCommand> {
    let search_dirs = media_tools::bundled_media_tool_dirs(app);
    find_native_player_command(&search_dirs)
}

fn find_native_player_command(search_dirs: &[PathBuf]) -> Option<NativePlayerCommand> {
    native_player_candidates(search_dirs)
        .into_iter()
        .find(native_player_command_available)
}

fn native_player_candidates(search_dirs: &[PathBuf]) -> Vec<NativePlayerCommand> {
    let mut candidates = Vec::new();

    for directory in search_dirs {
        candidates.push(NativePlayerCommand {
            name: "mpv".to_string(),
            program: directory.join("mpv").to_string_lossy().to_string(),
            args: vec![
                "--force-window=yes".to_string(),
                "--no-terminal".to_string(),
            ],
        });
        candidates.push(NativePlayerCommand {
            name: "mpv".to_string(),
            program: directory.join("mpv.exe").to_string_lossy().to_string(),
            args: vec![
                "--force-window=yes".to_string(),
                "--no-terminal".to_string(),
            ],
        });
        candidates.push(NativePlayerCommand {
            name: "VLC".to_string(),
            program: directory.join("vlc").to_string_lossy().to_string(),
            args: Vec::new(),
        });
        candidates.push(NativePlayerCommand {
            name: "VLC".to_string(),
            program: directory.join("vlc.exe").to_string_lossy().to_string(),
            args: Vec::new(),
        });
        candidates.push(NativePlayerCommand {
            name: "ffplay".to_string(),
            program: directory.join("ffplay").to_string_lossy().to_string(),
            args: vec![
                "-hide_banner".to_string(),
                "-loglevel".to_string(),
                "warning".to_string(),
            ],
        });
        candidates.push(NativePlayerCommand {
            name: "ffplay".to_string(),
            program: directory.join("ffplay.exe").to_string_lossy().to_string(),
            args: vec![
                "-hide_banner".to_string(),
                "-loglevel".to_string(),
                "warning".to_string(),
            ],
        });
    }

    candidates.extend([
        NativePlayerCommand {
            name: "mpv".to_string(),
            program: "mpv".to_string(),
            args: vec![
                "--force-window=yes".to_string(),
                "--no-terminal".to_string(),
            ],
        },
        NativePlayerCommand {
            name: "mpv".to_string(),
            program: "/Applications/mpv.app/Contents/MacOS/mpv".to_string(),
            args: vec![
                "--force-window=yes".to_string(),
                "--no-terminal".to_string(),
            ],
        },
        NativePlayerCommand {
            name: "mpv".to_string(),
            program: "/usr/bin/mpv".to_string(),
            args: vec![
                "--force-window=yes".to_string(),
                "--no-terminal".to_string(),
            ],
        },
        NativePlayerCommand {
            name: "VLC".to_string(),
            program: "vlc".to_string(),
            args: Vec::new(),
        },
        NativePlayerCommand {
            name: "VLC".to_string(),
            program: "/Applications/VLC.app/Contents/MacOS/VLC".to_string(),
            args: Vec::new(),
        },
        NativePlayerCommand {
            name: "VLC".to_string(),
            program: "/usr/bin/vlc".to_string(),
            args: Vec::new(),
        },
        NativePlayerCommand {
            name: "ffplay".to_string(),
            program: "ffplay".to_string(),
            args: vec![
                "-hide_banner".to_string(),
                "-loglevel".to_string(),
                "warning".to_string(),
            ],
        },
        NativePlayerCommand {
            name: "ffplay".to_string(),
            program: "/opt/homebrew/bin/ffplay".to_string(),
            args: vec![
                "-hide_banner".to_string(),
                "-loglevel".to_string(),
                "warning".to_string(),
            ],
        },
        NativePlayerCommand {
            name: "ffplay".to_string(),
            program: "/usr/local/bin/ffplay".to_string(),
            args: vec![
                "-hide_banner".to_string(),
                "-loglevel".to_string(),
                "warning".to_string(),
            ],
        },
        NativePlayerCommand {
            name: "ffplay".to_string(),
            program: "/usr/bin/ffplay".to_string(),
            args: vec![
                "-hide_banner".to_string(),
                "-loglevel".to_string(),
                "warning".to_string(),
            ],
        },
    ]);

    if cfg!(target_os = "windows") {
        if let Some(program_files) = env::var_os("ProgramFiles") {
            let program_files = PathBuf::from(program_files);
            candidates.extend([
                NativePlayerCommand {
                    name: "mpv".to_string(),
                    program: program_files
                        .join("mpv")
                        .join("mpv.exe")
                        .to_string_lossy()
                        .to_string(),
                    args: vec![
                        "--force-window=yes".to_string(),
                        "--no-terminal".to_string(),
                    ],
                },
                NativePlayerCommand {
                    name: "VLC".to_string(),
                    program: program_files
                        .join("VideoLAN")
                        .join("VLC")
                        .join("vlc.exe")
                        .to_string_lossy()
                        .to_string(),
                    args: Vec::new(),
                },
                NativePlayerCommand {
                    name: "ffplay".to_string(),
                    program: program_files
                        .join("ffmpeg")
                        .join("bin")
                        .join("ffplay.exe")
                        .to_string_lossy()
                        .to_string(),
                    args: vec![
                        "-hide_banner".to_string(),
                        "-loglevel".to_string(),
                        "warning".to_string(),
                    ],
                },
            ]);
        }
    }

    candidates
}

fn native_player_command_available(command: &NativePlayerCommand) -> bool {
    let program_path = Path::new(&command.program);
    if program_path.components().count() > 1 && !program_path.is_file() {
        return false;
    }

    Command::new(&command.program)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn native_player_command_label(command: &NativePlayerCommand) -> String {
    if command.args.is_empty() {
        command.program.clone()
    } else {
        format!("{} {}", command.program, command.args.join(" "))
    }
}

fn spawn_native_player_checked(
    player: &NativePlayerCommand,
    media_path: &Path,
) -> Result<(), String> {
    let stderr_path = env::temp_dir().join(format!(
        "duroos-native-player-{}-stderr.log",
        Uuid::new_v4()
    ));
    let stderr_file = fs::File::create(&stderr_path)
        .map_err(|error| format!("Could not prepare native player diagnostics: {error}"))?;
    let mut command = Command::new(&player.program);
    command
        .args(&player.args)
        .arg(media_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file));
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not launch {}: {error}", player.name))?;

    thread::sleep(Duration::from_millis(650));
    match child
        .try_wait()
        .map_err(|error| format!("Could not inspect {} startup: {error}", player.name))?
    {
        Some(status) => {
            let stderr = fs::read(&stderr_path).unwrap_or_default();
            let _ = fs::remove_file(&stderr_path);
            Err(format!(
                "{} exited immediately with {status}. {}",
                player.name,
                output_summary(&stderr)
            ))
        }
        None => {
            let _ = fs::remove_file(&stderr_path);
            Ok(())
        }
    }
}

fn yt_dlp_cookie_file(data_dir: &Path) -> Option<PathBuf> {
    YT_DLP_COOKIE_FILE_NAMES
        .iter()
        .map(|file_name| data_dir.join(file_name))
        .find(|candidate| candidate.is_file())
}

fn yt_dlp_auth_hint(cookies_file: Option<&Path>, _data_dir: &Path) -> String {
    if cookies_file.is_some() {
        return "Local cookies were used; if this source still failed, refresh the cookies file or import a manually downloaded file.".to_string();
    }

    "If this source requires sign-in or blocks anonymous fetches, export browser cookies in Netscape format to yt-dlp-cookies.txt in the app data folder and retry, or import a manually downloaded file.".to_string()
}

fn existing_completed_media_file(lesson_dir: &Path, content_type: &str) -> Option<PathBuf> {
    let mut media_files = collect_completed_media_files(lesson_dir, content_type);
    media_files.sort_by_key(|path| {
        path.metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
    });

    media_files
        .iter()
        .rev()
        .find(|path| {
            playback_profile_for_media_file(path, content_type)
                .map(|profile| profile.starts_with("webkit-compatible:"))
                .unwrap_or(false)
        })
        .cloned()
        .or_else(|| media_files.pop())
}

fn collect_completed_media_files(path: &Path, content_type: &str) -> Vec<PathBuf> {
    collect_media_files(path)
        .into_iter()
        .filter(|path| validate_downloaded_media_file(path, content_type).is_ok())
        .collect()
}

fn validate_response_media_type(response: &Response, content_type: &str) -> Result<(), String> {
    let Some(header) = response.headers().get(CONTENT_TYPE) else {
        return Ok(());
    };
    let media_type = header
        .to_str()
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    if media_type.is_empty() || response_media_type_matches(&media_type, content_type) {
        return Ok(());
    }

    Err(format!(
        "Source returned {media_type} instead of a {content_type} file. It may be an HTML page, sign-in screen, or unsupported redirect."
    ))
}

fn response_media_type_matches(media_type: &str, content_type: &str) -> bool {
    if matches!(
        media_type,
        "application/octet-stream" | "binary/octet-stream" | "application/download"
    ) {
        return true;
    }

    match content_type {
        "video" => {
            media_type.starts_with("video/")
                || matches!(media_type, "application/mp4" | "application/quicktime")
        }
        "audio" => media_type.starts_with("audio/") || media_type == "application/ogg",
        "pdf" => media_type.contains("pdf"),
        _ => false,
    }
}

fn validate_downloaded_media_file(path: &Path, content_type: &str) -> Result<(), String> {
    if is_probably_partial_media_file(path) {
        return Err(
            "Downloaded file is an incomplete adaptive media fragment, not a playable file."
                .to_string(),
        );
    }

    let metadata = path.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("Downloaded media file is empty or missing.".to_string());
    }

    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .ok_or_else(|| "Downloaded media file has no supported extension.".to_string())?;
    let extension_content_type = content_type_from_extension(&extension);

    if extension_content_type != content_type {
        return Err(format!(
            "Downloaded file extension .{extension} does not match the expected {content_type} content type."
        ));
    }

    let prefix = read_file_prefix(path, 512)?;
    let looks_valid = match content_type {
        "video" => match extension.as_str() {
            "mp4" | "m4v" | "mov" => has_iso_bmff_signature(&prefix),
            "webm" | "mkv" => has_ebml_signature(&prefix),
            _ => false,
        },
        "audio" => match extension.as_str() {
            "mp3" => has_mp3_signature(&prefix),
            "m4a" => has_iso_bmff_signature(&prefix),
            "aac" => has_aac_adts_signature(&prefix),
            "wav" => has_wav_signature(&prefix),
            "flac" => prefix.starts_with(b"fLaC"),
            "ogg" => prefix.starts_with(b"OggS"),
            _ => false,
        },
        "pdf" => prefix.starts_with(b"%PDF-"),
        _ => false,
    };

    if looks_valid {
        return Ok(());
    }

    if content_type == "pdf" {
        Err(format!(
            "Downloaded file does not look like a valid {content_type} file. The source may have returned HTML/XML or an unsupported container."
        ))
    } else if is_probably_text_payload(&prefix) {
        Err(format!(
            "Downloaded file looks like a text or HTML response, not a playable {content_type} file."
        ))
    } else if is_plausible_unrecognized_media_payload(&prefix, metadata.len()) {
        Ok(())
    } else {
        Err(format!(
            "Downloaded file does not look like a playable {content_type} file."
        ))
    }
}

fn playback_profile_for_media_file(path: &Path, content_type: &str) -> Result<String, String> {
    validate_downloaded_media_file(path, content_type)?;

    if let Some(profile) = ffprobe_playback_profile(path, content_type) {
        return Ok(profile);
    }

    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .ok_or_else(|| "Downloaded media file has no supported extension.".to_string())?;
    let probe = read_file_prefix(path, 4 * 1024 * 1024)?;

    let profile = match content_type {
        "video" => match extension.as_str() {
            "mp4" | "m4v" | "mov" => {
                if contains_ascii_marker(&probe, b"avc1") && contains_ascii_marker(&probe, b"mp4a")
                {
                    "webkit-compatible:h264-aac-mp4"
                } else if contains_ascii_marker(&probe, b"avc1") {
                    "webkit-compatible:h264-mp4"
                } else if contains_any_ascii_marker(&probe, &[b"hvc1", b"hev1", b"av01", b"vp09"]) {
                    "webkit-unverified:mp4-codec"
                } else {
                    "webkit-unverified:mp4"
                }
            }
            "webm" => "webkit-unverified:webm",
            "mkv" => "webkit-unverified:mkv",
            _ => "webkit-unverified:video",
        },
        "audio" => match extension.as_str() {
            "mp3" => "webkit-compatible:mp3",
            "m4a" | "aac" => "webkit-compatible:aac",
            "wav" => "webkit-compatible:wav",
            "flac" => "webkit-unverified:flac",
            "ogg" => "webkit-unverified:ogg",
            _ => "webkit-unverified:audio",
        },
        "pdf" => "webkit-compatible:pdf",
        _ => "webkit-unverified:unknown",
    };

    Ok(profile.to_string())
}

fn ffprobe_playback_profile(path: &Path, content_type: &str) -> Option<String> {
    let ffprobe = find_media_tool("ffprobe")?;
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,codec_type",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let streams = parsed.get("streams")?.as_array()?;
    let video_codec = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(json_text).as_deref() == Some("video"))
        .and_then(|stream| stream.get("codec_name"))
        .and_then(json_text)
        .map(|codec| codec.to_ascii_lowercase());
    let audio_codec = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(json_text).as_deref() == Some("audio"))
        .and_then(|stream| stream.get("codec_name"))
        .and_then(json_text)
        .map(|codec| codec.to_ascii_lowercase());

    match content_type {
        "video" => {
            let video_codec = video_codec?;
            if video_codec == "h264"
                && audio_codec
                    .as_deref()
                    .map(is_webkit_audio_codec)
                    .unwrap_or(true)
            {
                Some("webkit-compatible:h264-aac-mp4".to_string())
            } else {
                Some(format!(
                    "webkit-unverified:video-codec-{}",
                    safe_path_segment(&video_codec)
                ))
            }
        }
        "audio" => {
            let audio_codec = audio_codec?;
            if is_webkit_audio_codec(&audio_codec) {
                Some(format!(
                    "webkit-compatible:{}",
                    safe_path_segment(&audio_codec)
                ))
            } else {
                Some(format!(
                    "webkit-unverified:audio-codec-{}",
                    safe_path_segment(&audio_codec)
                ))
            }
        }
        _ => None,
    }
}

fn is_webkit_audio_codec(codec: &str) -> bool {
    matches!(
        codec,
        "aac" | "mp3" | "mp2" | "alac" | "pcm_s16le" | "pcm_s24le" | "pcm_f32le"
    )
}

fn read_file_prefix(path: &Path, byte_count: usize) -> Result<Vec<u8>, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut buffer = vec![0_u8; byte_count];
    let bytes_read = file.read(&mut buffer).map_err(|error| error.to_string())?;
    buffer.truncate(bytes_read);
    Ok(buffer)
}

fn has_iso_bmff_signature(prefix: &[u8]) -> bool {
    prefix
        .windows(4)
        .position(|window| window == b"ftyp")
        .map(|position| (4..=64).contains(&position))
        .unwrap_or(false)
}

fn has_ebml_signature(prefix: &[u8]) -> bool {
    prefix.starts_with(&[0x1A, 0x45, 0xDF, 0xA3])
}

fn has_mp3_signature(prefix: &[u8]) -> bool {
    prefix.starts_with(b"ID3")
        || (prefix.len() >= 2 && prefix[0] == 0xFF && (prefix[1] & 0xE0) == 0xE0)
}

fn has_aac_adts_signature(prefix: &[u8]) -> bool {
    prefix.len() >= 2 && prefix[0] == 0xFF && (prefix[1] & 0xF0) == 0xF0
}

fn has_wav_signature(prefix: &[u8]) -> bool {
    prefix.len() >= 12 && prefix.starts_with(b"RIFF") && &prefix[8..12] == b"WAVE"
}

fn contains_any_ascii_marker<const N: usize>(bytes: &[u8], markers: &[&[u8; N]]) -> bool {
    markers
        .iter()
        .any(|marker| contains_ascii_marker(bytes, marker.as_slice()))
}

fn contains_ascii_marker(bytes: &[u8], marker: &[u8]) -> bool {
    bytes.windows(marker.len()).any(|window| window == marker)
}

fn is_probably_text_payload(prefix: &[u8]) -> bool {
    let trimmed = trim_ascii_whitespace(prefix);
    if trimmed.is_empty() {
        return true;
    }

    if matches!(trimmed[0], b'<' | b'{' | b'[') {
        return true;
    }

    let probe_len = trimmed.len().min(64);
    let probe = String::from_utf8_lossy(&trimmed[..probe_len]).to_ascii_lowercase();
    probe.starts_with("not found")
        || probe.starts_with("forbidden")
        || probe.starts_with("unauthorized")
        || probe.starts_with("error")
}

fn trim_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|index| index + 1)
        .unwrap_or(start);

    &bytes[start..end]
}

fn is_plausible_unrecognized_media_payload(prefix: &[u8], size_bytes: u64) -> bool {
    size_bytes >= MIN_PLAUSIBLE_MEDIA_BYTES
        && prefix.contains(&0)
        && prefix.iter().any(|byte| *byte >= 0x80)
}

fn is_probably_partial_media_file(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if file_name.ends_with(".part") || file_name.contains(".part.") {
        return true;
    }

    Path::new(&file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .and_then(|stem| stem.rsplit('.').next())
        .map(|last_segment| {
            last_segment.len() > 1
                && last_segment.starts_with('f')
                && last_segment[1..]
                    .chars()
                    .all(|character| character.is_ascii_digit())
        })
        .unwrap_or(false)
}

fn safe_path_segment(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(['.', '-', '_'])
        .to_string();

    if normalized.is_empty() {
        "item".to_string()
    } else {
        normalized
    }
}

fn output_summary(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let trimmed = text.trim();

    if trimmed.is_empty() {
        return "no stderr output".to_string();
    }

    trimmed
        .lines()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("Could not resolve app data directory: {error}"))
}

fn run_migrations(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS sources (
              id TEXT PRIMARY KEY,
              platform TEXT NOT NULL,
              label TEXT NOT NULL,
              identifier TEXT NOT NULL,
              feed_format TEXT NOT NULL DEFAULT 'rss',
              feed_transport TEXT NOT NULL DEFAULT 'https',
              trust_state TEXT NOT NULL DEFAULT 'unsigned',
              trusted_curator_id TEXT,
              auth_mode TEXT NOT NULL,
              update_schedule TEXT NOT NULL,
              capability_json TEXT NOT NULL,
              enabled INTEGER NOT NULL,
              last_checked_at TEXT,
              last_verified_at TEXT
            );

            CREATE TABLE IF NOT EXISTS teachers (
              id TEXT PRIMARY KEY,
              display_name TEXT NOT NULL,
              description TEXT,
              source_links_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS teacher_relays (
              id TEXT PRIMARY KEY,
              teacher_id TEXT NOT NULL REFERENCES teachers(id),
              title TEXT NOT NULL,
              feed_url TEXT NOT NULL,
              feed_format TEXT NOT NULL DEFAULT 'rss',
              feed_transport TEXT NOT NULL DEFAULT 'https',
              trust_state TEXT NOT NULL DEFAULT 'unsigned',
              subscriber_count INTEGER NOT NULL,
              visibility TEXT NOT NULL,
              trust_policy TEXT NOT NULL,
              auto_download INTEGER NOT NULL,
              last_published_at TEXT,
              last_verified_at TEXT,
              description TEXT
            );

            CREATE TABLE IF NOT EXISTS trusted_curators (
              id TEXT PRIMARY KEY,
              display_name TEXT NOT NULL,
              public_key TEXT NOT NULL UNIQUE,
              trust_note TEXT,
              added_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS publisher_profiles (
              id TEXT PRIMARY KEY,
              display_name TEXT NOT NULL,
              curator_public_key TEXT NOT NULL,
              nostr_pubkey TEXT NOT NULL,
              relays_json TEXT NOT NULL,
              blossom_servers_json TEXT NOT NULL,
              vault_path TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS publisher_channels (
              id TEXT PRIMARY KEY,
              profile_id TEXT NOT NULL REFERENCES publisher_profiles(id),
              title TEXT NOT NULL,
              description TEXT,
              channel_identifier TEXT NOT NULL UNIQUE,
              naddr TEXT,
              canonical_channel_link TEXT,
              last_manifest_sha256 TEXT,
              last_manifest_url TEXT,
              last_published_at TEXT,
              media_count INTEGER NOT NULL DEFAULT 0,
              post_count INTEGER NOT NULL DEFAULT 0,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS publisher_channel_items (
              id TEXT PRIMARY KEY,
              channel_id TEXT NOT NULL REFERENCES publisher_channels(id),
              item_type TEXT NOT NULL,
              title TEXT NOT NULL,
              content_type TEXT NOT NULL,
              description TEXT,
              origin_url TEXT NOT NULL,
              retrieval_url TEXT,
              retrieval_refs_json TEXT NOT NULL DEFAULT '[]',
              sha256 TEXT NOT NULL,
              size_bytes INTEGER,
              mime_type TEXT,
              published_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS live_sessions (
              id TEXT PRIMARY KEY,
              teacher_id TEXT NOT NULL REFERENCES teachers(id),
              relay_id TEXT NOT NULL REFERENCES teacher_relays(id),
              title TEXT NOT NULL,
              provider TEXT NOT NULL,
              provider_url TEXT NOT NULL,
              status TEXT NOT NULL,
              starts_at TEXT NOT NULL,
              archive_lesson_id TEXT,
              auto_publish_archive INTEGER NOT NULL,
              recording_policy TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS collections (
              id TEXT PRIMARY KEY,
              title TEXT NOT NULL,
              owner_label TEXT NOT NULL,
              sort_order INTEGER NOT NULL,
              lesson_count INTEGER NOT NULL,
              source_ids_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS lessons (
              id TEXT PRIMARY KEY,
              title TEXT NOT NULL,
              content_type TEXT NOT NULL DEFAULT 'video',
              teacher_id TEXT NOT NULL REFERENCES teachers(id),
              collection_id TEXT NOT NULL REFERENCES collections(id),
              source_id TEXT NOT NULL REFERENCES sources(id),
              source_url TEXT NOT NULL,
              retrieval_refs_json TEXT NOT NULL DEFAULT '[]',
              published_at TEXT,
              description TEXT,
              thumbnail_tone TEXT NOT NULL,
              duration_seconds INTEGER,
              media_file_id TEXT,
              provenance_id TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS media_files (
              id TEXT PRIMARY KEY,
              lesson_id TEXT NOT NULL REFERENCES lessons(id),
              relative_path TEXT NOT NULL,
              thumbnail_relative_path TEXT,
              content_hash TEXT NOT NULL,
              size_bytes INTEGER NOT NULL,
              codec TEXT,
              import_status TEXT NOT NULL,
              hash_verification_state TEXT NOT NULL DEFAULT 'not-provided'
            );

            CREATE TABLE IF NOT EXISTS provenance_records (
              id TEXT PRIMARY KEY,
              lesson_id TEXT NOT NULL REFERENCES lessons(id),
              origin_url TEXT NOT NULL,
              permission_note TEXT NOT NULL,
              imported_at TEXT NOT NULL,
              adapter_name TEXT NOT NULL,
              content_hash TEXT
            );

            CREATE TABLE IF NOT EXISTS watch_state (
              lesson_id TEXT PRIMARY KEY REFERENCES lessons(id),
              progress_seconds INTEGER NOT NULL,
              completed INTEGER NOT NULL,
              last_watched_at TEXT
            );

            CREATE TABLE IF NOT EXISTS lesson_notes (
              lesson_id TEXT PRIMARY KEY REFERENCES lessons(id),
              body TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS jobs (
              id TEXT PRIMARY KEY,
              kind TEXT NOT NULL,
              state TEXT NOT NULL,
              source_id TEXT,
              lesson_id TEXT,
              label TEXT NOT NULL,
              detail TEXT NOT NULL,
              retry_count INTEGER NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS lessons_fts USING fts5(
              lesson_id UNINDEXED,
              title,
              description,
              teacher,
              collection_title,
              source_label
            );
            ",
        )
        .map_err(|error| error.to_string())?;

    ensure_column(
        connection,
        "sources",
        "feed_format",
        "ALTER TABLE sources ADD COLUMN feed_format TEXT NOT NULL DEFAULT 'rss'",
    )?;
    ensure_column(
        connection,
        "sources",
        "feed_transport",
        "ALTER TABLE sources ADD COLUMN feed_transport TEXT NOT NULL DEFAULT 'https'",
    )?;
    ensure_column(
        connection,
        "sources",
        "trust_state",
        "ALTER TABLE sources ADD COLUMN trust_state TEXT NOT NULL DEFAULT 'unsigned'",
    )?;
    ensure_column(
        connection,
        "sources",
        "trusted_curator_id",
        "ALTER TABLE sources ADD COLUMN trusted_curator_id TEXT",
    )?;
    ensure_column(
        connection,
        "sources",
        "last_verified_at",
        "ALTER TABLE sources ADD COLUMN last_verified_at TEXT",
    )?;
    ensure_column(
        connection,
        "teacher_relays",
        "feed_format",
        "ALTER TABLE teacher_relays ADD COLUMN feed_format TEXT NOT NULL DEFAULT 'rss'",
    )?;
    ensure_column(
        connection,
        "teacher_relays",
        "feed_transport",
        "ALTER TABLE teacher_relays ADD COLUMN feed_transport TEXT NOT NULL DEFAULT 'https'",
    )?;
    ensure_column(
        connection,
        "teacher_relays",
        "trust_state",
        "ALTER TABLE teacher_relays ADD COLUMN trust_state TEXT NOT NULL DEFAULT 'unsigned'",
    )?;
    ensure_column(
        connection,
        "teacher_relays",
        "last_verified_at",
        "ALTER TABLE teacher_relays ADD COLUMN last_verified_at TEXT",
    )?;
    ensure_column(
        connection,
        "media_files",
        "thumbnail_relative_path",
        "ALTER TABLE media_files ADD COLUMN thumbnail_relative_path TEXT",
    )?;
    ensure_column(
        connection,
        "media_files",
        "hash_verification_state",
        "ALTER TABLE media_files ADD COLUMN hash_verification_state TEXT NOT NULL DEFAULT 'not-provided'",
    )?;
    ensure_column(
        connection,
        "lessons",
        "content_type",
        "ALTER TABLE lessons ADD COLUMN content_type TEXT NOT NULL DEFAULT 'video'",
    )?;
    ensure_column(
        connection,
        "lessons",
        "retrieval_refs_json",
        "ALTER TABLE lessons ADD COLUMN retrieval_refs_json TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_column(
        connection,
        "publisher_channel_items",
        "retrieval_refs_json",
        "ALTER TABLE publisher_channel_items ADD COLUMN retrieval_refs_json TEXT NOT NULL DEFAULT '[]'",
    )?;
    backfill_retrieval_refs_json(connection)?;
    ensure_column(
        connection,
        "publisher_profiles",
        "last_endpoint_tested_at",
        "ALTER TABLE publisher_profiles ADD COLUMN last_endpoint_tested_at TEXT",
    )?;
    ensure_column(
        connection,
        "publisher_profiles",
        "last_endpoint_test_passed",
        "ALTER TABLE publisher_profiles ADD COLUMN last_endpoint_test_passed INTEGER",
    )?;
    ensure_column(
        connection,
        "publisher_profiles",
        "last_endpoint_test_summary",
        "ALTER TABLE publisher_profiles ADD COLUMN last_endpoint_test_summary TEXT",
    )?;
    ensure_column(
        connection,
        "jobs",
        "started_at",
        "ALTER TABLE jobs ADD COLUMN started_at TEXT",
    )?;
    ensure_column(
        connection,
        "jobs",
        "completed_at",
        "ALTER TABLE jobs ADD COLUMN completed_at TEXT",
    )?;
    ensure_column(
        connection,
        "jobs",
        "bytes_expected",
        "ALTER TABLE jobs ADD COLUMN bytes_expected INTEGER",
    )?;
    ensure_column(
        connection,
        "jobs",
        "bytes_downloaded",
        "ALTER TABLE jobs ADD COLUMN bytes_downloaded INTEGER",
    )?;
    ensure_column(
        connection,
        "jobs",
        "bytes_per_second",
        "ALTER TABLE jobs ADD COLUMN bytes_per_second REAL",
    )?;
    ensure_column(
        connection,
        "jobs",
        "elapsed_ms",
        "ALTER TABLE jobs ADD COLUMN elapsed_ms INTEGER",
    )?;
    backfill_lesson_content_types(connection)
}

fn ensure_column(
    connection: &Connection,
    table_name: &str,
    column_name: &str,
    alter_sql: &str,
) -> Result<(), String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table_name})"))
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    if columns.iter().any(|column| column == column_name) {
        return Ok(());
    }

    connection
        .execute(alter_sql, [])
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn backfill_retrieval_refs_json(connection: &Connection) -> Result<(), String> {
    let mut lesson_statement = connection
        .prepare(
            "SELECT id, content_type, source_url
             FROM lessons
             WHERE retrieval_refs_json IS NULL
                OR retrieval_refs_json = ''
                OR retrieval_refs_json = '[]'",
        )
        .map_err(|error| error.to_string())?;
    let lesson_rows = lesson_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?;

    for row in lesson_rows {
        let (id, content_type, source_url) = row.map_err(|error| error.to_string())?;
        if !is_file_backed_content_type(&content_type) || !is_safe_http_url(&source_url) {
            continue;
        }
        let refs = vec![RetrievalRef {
            kind: "direct-url".to_string(),
            url: Some(source_url),
            service: None,
            media_type: None,
            ..Default::default()
        }];
        connection
            .execute(
                "UPDATE lessons SET retrieval_refs_json = ?1 WHERE id = ?2",
                params![retrieval_refs_to_json(&refs)?, id],
            )
            .map_err(|error| error.to_string())?;
    }

    let mut item_statement = connection
        .prepare(
            "SELECT id, item_type, retrieval_url, sha256, size_bytes, mime_type
             FROM publisher_channel_items
             WHERE (retrieval_refs_json IS NULL
                 OR retrieval_refs_json = ''
                 OR retrieval_refs_json = '[]')
               AND retrieval_url IS NOT NULL",
        )
        .map_err(|error| error.to_string())?;
    let item_rows = item_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|error| error.to_string())?;

    for row in item_rows {
        let (id, item_type, retrieval_url, sha256, size_bytes, mime_type) =
            row.map_err(|error| error.to_string())?;
        let Some(url) = retrieval_url.filter(|url| is_safe_http_url(url)) else {
            continue;
        };
        if item_type != "media" {
            continue;
        }
        let refs = vec![RetrievalRef {
            kind: "direct-url".to_string(),
            url: Some(url),
            service: Some("blossom".to_string()),
            sha256: Some(format_sha256(&sha256)),
            size_bytes,
            mime_type: mime_type.clone(),
            media_type: mime_type,
            ..Default::default()
        }];
        connection
            .execute(
                "UPDATE publisher_channel_items SET retrieval_refs_json = ?1 WHERE id = ?2",
                params![retrieval_refs_to_json(&refs)?, id],
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn backfill_lesson_content_types(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT l.id,
                    l.content_type,
                    l.source_url,
                    s.platform,
                    p.adapter_name,
                    (
                      SELECT m.relative_path
                      FROM media_files m
                      WHERE m.lesson_id = l.id
                      ORDER BY m.id
                      LIMIT 1
                    ) AS relative_path
             FROM lessons l
             LEFT JOIN sources s ON s.id = l.source_id
             LEFT JOIN provenance_records p ON p.lesson_id = l.id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|error| error.to_string())?;

    let lessons = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    for (lesson_id, current, source_url, platform, adapter_name, relative_path) in lessons {
        let Some(inference) = infer_existing_content_type(
            relative_path.as_deref(),
            &source_url,
            platform.as_deref(),
            adapter_name.as_deref(),
        ) else {
            continue;
        };

        if should_update_content_type(&current, inference) {
            connection
                .execute(
                    "UPDATE lessons SET content_type = ?1 WHERE id = ?2",
                    params![inference.content_type, lesson_id],
                )
                .map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}

fn backfill_missing_video_thumbnails(
    connection: &Connection,
    data_dir: &Path,
) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT m.id, m.relative_path
             FROM media_files m
             JOIN lessons l ON l.id = m.lesson_id
             WHERE l.content_type = 'video'
               AND (m.thumbnail_relative_path IS NULL OR m.thumbnail_relative_path = '')",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let media_files = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    for (media_file_id, relative_path) in media_files {
        let Some(media_path) = resolve_library_media_path(data_dir, &relative_path) else {
            continue;
        };
        if !media_path.is_file() {
            continue;
        }

        if let Some(thumbnail_relative_path) =
            generate_video_thumbnail(data_dir, &media_path, &media_file_id, "video")?
        {
            connection
                .execute(
                    "UPDATE media_files SET thumbnail_relative_path = ?1 WHERE id = ?2",
                    params![thumbnail_relative_path, media_file_id],
                )
                .map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}

fn ensure_default_records(connection: &mut Connection) -> Result<(), String> {
    remove_demo_seed_data(connection)?;

    let tx = connection
        .transaction()
        .map_err(|error| error.to_string())?;

    let now = Utc::now().to_rfc3339();
    let source_rows = [
        (
            "source-local-files",
            "local-files",
            "Local Files",
            "App Library",
            "none",
            "Manual",
            capability(
                "native",
                "native",
                "blocked",
                false,
                "none",
                "stable",
                "Imports video, audio, and PDF files the user selects and records local source metadata.",
            ),
            true,
            Some(now.as_str()),
        ),
        (
            "source-telegram",
            "telegram",
            "Telegram Public Preview",
            "https://t.me/s/<channel>",
            "none",
            "Manual + daily check",
            capability(
                "supported",
                "limited",
                "supported",
                false,
                "none",
                "best-effort",
                "Public channel previews can be read without sign-in; private channels still require a local session.",
            ),
            true,
            Some(now.as_str()),
        ),
        (
            "source-rss-feed",
            "rss-feed",
            "RSS/Atom/JSON Feed",
            "https://example.com/feed.xml",
            "none",
            "Manual + daily check",
            capability(
                "supported",
                "supported",
                "supported",
                false,
                "none",
                "stable",
                "Custom RSS, Atom, and JSON Feed subscriptions can ingest videos, audio, PDFs, and teacher message posts.",
            ),
            true,
            Some(now.as_str()),
        ),
        (
            "source-archive-org",
            "archive-org",
            "Archive.org",
            "archive.org/details/<identifier>",
            "none",
            "Manual + daily check",
            capability(
                "supported",
                "supported",
                "supported",
                false,
                "none",
                "stable",
                "Uses Archive.org item metadata and file listings.",
            ),
            true,
            Some(now.as_str()),
        ),
        (
            "source-youtube",
            "youtube",
            "YouTube",
            "youtube:not-configured",
            "api-key",
            "Manual until configured",
            capability(
                "supported",
                "limited",
                "limited",
                true,
                "api-key",
                "best-effort",
                "Official API covers metadata; RSS feed URLs work without an API key where available.",
            ),
            false,
            None,
        ),
        (
            "source-x",
            "x",
            "X",
            "x:not-configured",
            "api-key",
            "Manual until configured",
            capability(
                "limited",
                "limited",
                "limited",
                true,
                "api-key",
                "credential-bound",
                "API access is credential-bound and platform-constrained.",
            ),
            false,
            None,
        ),
        (
            "source-rumble",
            "rumble",
            "Rumble",
            "rumble:not-configured",
            "none",
            "Manual until configured",
            capability(
                "limited",
                "limited",
                "limited",
                false,
                "none",
                "best-effort",
                "No broad public catalog API is assumed; URL extraction is best-effort.",
            ),
            false,
            None,
        ),
        (
            "source-odysee",
            "odysee",
            "Odysee",
            "odysee:not-configured",
            "none",
            "Manual until configured",
            capability(
                "limited",
                "limited",
                "limited",
                false,
                "none",
                "best-effort",
                "No broad native catalog adapter is assumed; user-initiated URLs can use local tooling where supported.",
            ),
            false,
            None,
        ),
        (
            "source-teacher-relay",
            "teacher-relay",
            "Channels",
            "https://teacher.example/feed.duroos.json",
            "none",
            "Manual + daily check",
            capability(
                "native",
                "supported",
                "supported",
                false,
                "none",
                "stable",
                "Signed curator feeds publish classes, live archives, posts, hashes, and source metadata.",
            ),
            true,
            Some(now.as_str()),
        ),
    ];

    for source in source_rows {
        let feed_format = match source.1 {
            "archive-org" => "json-feed",
            "teacher-relay" => "duroos-manifest",
            _ => "rss",
        };
        tx.execute(
            "INSERT INTO sources
             (id, platform, label, identifier, feed_format, feed_transport, trust_state,
              trusted_curator_id, auth_mode, update_schedule, capability_json, enabled,
              last_checked_at, last_verified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'https', 'unsigned', NULL, ?6, ?7, ?8, ?9, ?10, NULL)
             ON CONFLICT(id) DO UPDATE SET
               label = excluded.label,
               identifier = excluded.identifier,
               feed_format = excluded.feed_format,
               feed_transport = excluded.feed_transport,
               trust_state = excluded.trust_state,
               auth_mode = excluded.auth_mode,
               update_schedule = excluded.update_schedule,
               capability_json = excluded.capability_json,
               enabled = sources.enabled,
               last_checked_at = COALESCE(sources.last_checked_at, excluded.last_checked_at),
               last_verified_at = sources.last_verified_at",
            params![
                source.0,
                source.1,
                source.2,
                source.3,
                feed_format,
                source.4,
                source.5,
                serde_json::to_string(&source.6).map_err(|error| error.to_string())?,
                if source.7 { 1 } else { 0 },
                source.8
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    tx.execute(
        "INSERT INTO teachers (id, display_name, description, source_links_json)
         VALUES ('teacher-3', 'Personal Library', 'Imported local files on this machine.', '[]')
         ON CONFLICT(id) DO UPDATE SET
           display_name = excluded.display_name,
           description = excluded.description",
        [],
    )
    .map_err(|error| error.to_string())?;

    tx.execute(
        "INSERT INTO collections (id, title, owner_label, sort_order, lesson_count, source_ids_json)
         VALUES ('collection-2', 'Local Imports', 'Local archive', 10, 0, ?1)
         ON CONFLICT(id) DO UPDATE SET
           title = excluded.title,
           owner_label = excluded.owner_label,
           source_ids_json = excluded.source_ids_json,
           lesson_count = (SELECT COUNT(*) FROM lessons WHERE collection_id = 'collection-2')",
        params![serde_json::to_string(&vec!["source-local-files"])
            .map_err(|error| error.to_string())?],
    )
    .map_err(|error| error.to_string())?;

    tx.commit().map_err(|error| error.to_string())
}

fn remove_demo_seed_data(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            DELETE FROM lessons_fts WHERE lesson_id IN ('lesson-1', 'lesson-2', 'lesson-3', 'lesson-4', 'lesson-5');
            DELETE FROM watch_state WHERE lesson_id IN ('lesson-1', 'lesson-2', 'lesson-3', 'lesson-4', 'lesson-5');
            DELETE FROM media_files WHERE id IN ('media-1', 'media-2', 'media-3', 'media-4');
            DELETE FROM provenance_records WHERE id IN ('prov-1', 'prov-2', 'prov-3', 'prov-4', 'prov-5');
            DELETE FROM jobs WHERE id IN ('job-1', 'job-2', 'job-3', 'job-4');
            DELETE FROM live_sessions WHERE id IN ('live-1', 'live-2', 'live-3');
            DELETE FROM teacher_relays WHERE id IN ('relay-1', 'relay-2');
            DELETE FROM lessons WHERE id IN ('lesson-1', 'lesson-2', 'lesson-3', 'lesson-4', 'lesson-5');
            DELETE FROM collections WHERE id IN ('collection-1', 'collection-3');
            DELETE FROM teachers WHERE id IN ('teacher-1', 'teacher-2');
            ",
        )
        .map_err(|error| error.to_string())
}

fn capability(
    metadata: &str,
    download: &str,
    auto_update: &str,
    auth_required: bool,
    auth_mode: &str,
    reliability: &str,
    note: &str,
) -> SourceCapability {
    SourceCapability {
        metadata: metadata.to_string(),
        download: download.to_string(),
        auto_update: auto_update.to_string(),
        auth_required,
        auth_mode: auth_mode.to_string(),
        reliability: reliability.to_string(),
        note: note.to_string(),
    }
}

fn fetch_sources(connection: &Connection) -> Result<Vec<Source>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, platform, label, identifier, feed_format, feed_transport,
                    trust_state, trusted_curator_id, auth_mode, update_schedule,
                    capability_json, enabled, last_checked_at, last_verified_at
             FROM sources
             ORDER BY rowid",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            let capability_json: String = row.get(10)?;
            let capability = serde_json::from_str(&capability_json).unwrap_or_else(|_| {
                capability(
                    "limited",
                    "limited",
                    "limited",
                    false,
                    "none",
                    "best-effort",
                    "Capability metadata could not be parsed.",
                )
            });

            Ok(Source {
                id: row.get(0)?,
                platform: row.get(1)?,
                label: row.get(2)?,
                identifier: row.get(3)?,
                feed_format: row.get(4)?,
                feed_transport: row.get(5)?,
                trust_state: row.get(6)?,
                trusted_curator_id: row.get(7)?,
                auth_mode: row.get(8)?,
                update_schedule: row.get(9)?,
                capability,
                enabled: row.get::<_, i64>(11)? == 1,
                last_checked_at: row.get(12)?,
                last_verified_at: row.get(13)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn fetch_teachers(connection: &Connection) -> Result<Vec<Teacher>, String> {
    let mut statement = connection
        .prepare("SELECT id, display_name, description, source_links_json FROM teachers")
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            let source_links_json: String = row.get(3)?;
            Ok(Teacher {
                id: row.get(0)?,
                display_name: row.get(1)?,
                description: row.get(2)?,
                source_links: serde_json::from_str(&source_links_json).unwrap_or_default(),
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn fetch_teacher_relays(connection: &Connection) -> Result<Vec<TeacherRelay>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, teacher_id, title, feed_url, feed_format, feed_transport,
                    trust_state, subscriber_count, visibility, trust_policy, auto_download,
                    last_published_at, last_verified_at, description
             FROM teacher_relays
             ORDER BY title",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(TeacherRelay {
                id: row.get(0)?,
                teacher_id: row.get(1)?,
                title: row.get(2)?,
                feed_url: row.get(3)?,
                feed_format: row.get(4)?,
                feed_transport: row.get(5)?,
                trust_state: row.get(6)?,
                subscriber_count: row.get(7)?,
                visibility: row.get(8)?,
                trust_policy: row.get(9)?,
                auto_download: row.get::<_, i64>(10)? == 1,
                last_published_at: row.get(11)?,
                last_verified_at: row.get(12)?,
                description: row.get(13)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn fetch_live_sessions(connection: &Connection) -> Result<Vec<LiveSession>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, teacher_id, relay_id, title, provider, provider_url, status,
                    starts_at, archive_lesson_id, auto_publish_archive, recording_policy
             FROM live_sessions
             ORDER BY starts_at DESC",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(LiveSession {
                id: row.get(0)?,
                teacher_id: row.get(1)?,
                relay_id: row.get(2)?,
                title: row.get(3)?,
                provider: row.get(4)?,
                provider_url: row.get(5)?,
                status: row.get(6)?,
                starts_at: row.get(7)?,
                archive_lesson_id: row.get(8)?,
                auto_publish_archive: row.get::<_, i64>(9)? == 1,
                recording_policy: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn fetch_collections(connection: &Connection) -> Result<Vec<Collection>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, title, owner_label, sort_order, lesson_count, source_ids_json
             FROM collections
             ORDER BY sort_order",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            let source_ids_json: String = row.get(5)?;
            Ok(Collection {
                id: row.get(0)?,
                title: row.get(1)?,
                owner_label: row.get(2)?,
                sort_order: row.get(3)?,
                lesson_count: row.get(4)?,
                source_ids: serde_json::from_str(&source_ids_json).unwrap_or_default(),
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn fetch_lessons(connection: &Connection) -> Result<Vec<Lesson>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, title, content_type, teacher_id, collection_id, source_id,
                    source_url, retrieval_refs_json, published_at, description, thumbnail_tone,
                    duration_seconds, media_file_id, provenance_id
             FROM lessons
             ORDER BY published_at DESC, title ASC",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], lesson_from_row)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn lesson_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Lesson> {
    Ok(Lesson {
        id: row.get(0)?,
        title: row.get(1)?,
        content_type: row.get(2)?,
        teacher_id: row.get(3)?,
        collection_id: row.get(4)?,
        source_id: row.get(5)?,
        source_url: row.get(6)?,
        retrieval_refs: parse_retrieval_refs_json(&row.get::<_, String>(7)?),
        published_at: row.get(8)?,
        description: row.get(9)?,
        thumbnail_tone: row.get(10)?,
        duration_seconds: row.get(11)?,
        media_file_id: row.get(12)?,
        provenance_id: row.get(13)?,
    })
}

fn lesson_for_id(connection: &Connection, lesson_id: &str) -> Result<Lesson, String> {
    connection
        .query_row(
            "SELECT id, title, content_type, teacher_id, collection_id, source_id,
                    source_url, retrieval_refs_json, published_at, description, thumbnail_tone,
                    duration_seconds, media_file_id, provenance_id
             FROM lessons
             WHERE id = ?1",
            params![lesson_id],
            lesson_from_row,
        )
        .map_err(|error| error.to_string())
}

fn fetch_media_files(connection: &Connection) -> Result<Vec<MediaFile>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, lesson_id, relative_path, thumbnail_relative_path, content_hash,
                    size_bytes, codec, import_status, hash_verification_state
             FROM media_files",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(MediaFile {
                id: row.get(0)?,
                lesson_id: row.get(1)?,
                relative_path: row.get(2)?,
                thumbnail_relative_path: row.get(3)?,
                content_hash: row.get(4)?,
                size_bytes: row.get(5)?,
                codec: row.get(6)?,
                import_status: row.get(7)?,
                hash_verification_state: row.get(8)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn fetch_trusted_curators(connection: &Connection) -> Result<Vec<TrustedCurator>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, display_name, public_key, trust_note, added_at
             FROM trusted_curators
             ORDER BY display_name",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(TrustedCurator {
                id: row.get(0)?,
                display_name: row.get(1)?,
                public_key: row.get(2)?,
                trust_note: row.get(3)?,
                added_at: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn fetch_provenance_records(connection: &Connection) -> Result<Vec<ProvenanceRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, lesson_id, origin_url, permission_note, imported_at, adapter_name, content_hash
             FROM provenance_records",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(ProvenanceRecord {
                id: row.get(0)?,
                lesson_id: row.get(1)?,
                origin_url: row.get(2)?,
                permission_note: row.get(3)?,
                imported_at: row.get(4)?,
                adapter_name: row.get(5)?,
                content_hash: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn fetch_watch_state(connection: &Connection) -> Result<Vec<WatchState>, String> {
    let mut statement = connection
        .prepare(
            "SELECT lesson_id, progress_seconds, completed, last_watched_at
             FROM watch_state",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(WatchState {
                lesson_id: row.get(0)?,
                progress_seconds: row.get(1)?,
                completed: row.get::<_, i64>(2)? == 1,
                last_watched_at: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn watch_state_for_lesson(connection: &Connection, lesson_id: &str) -> Result<WatchState, String> {
    connection
        .query_row(
            "SELECT lesson_id, progress_seconds, completed, last_watched_at
             FROM watch_state
             WHERE lesson_id = ?1",
            params![lesson_id],
            |row| {
                Ok(WatchState {
                    lesson_id: row.get(0)?,
                    progress_seconds: row.get(1)?,
                    completed: row.get::<_, i64>(2)? == 1,
                    last_watched_at: row.get(3)?,
                })
            },
        )
        .map_err(|error| error.to_string())
}

fn fetch_lesson_notes(connection: &Connection) -> Result<Vec<LessonNote>, String> {
    let mut statement = connection
        .prepare(
            "SELECT lesson_id, body, updated_at
             FROM lesson_notes
             ORDER BY updated_at DESC",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(LessonNote {
                lesson_id: row.get(0)?,
                body: row.get(1)?,
                updated_at: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn lesson_note_for_lesson(connection: &Connection, lesson_id: &str) -> Result<LessonNote, String> {
    connection
        .query_row(
            "SELECT lesson_id, body, updated_at
             FROM lesson_notes
             WHERE lesson_id = ?1",
            params![lesson_id],
            |row| {
                Ok(LessonNote {
                    lesson_id: row.get(0)?,
                    body: row.get(1)?,
                    updated_at: row.get(2)?,
                })
            },
        )
        .map_err(|error| error.to_string())
}

fn fetch_jobs(connection: &Connection) -> Result<Vec<Job>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, kind, state, source_id, lesson_id, label, detail, retry_count, updated_at,
                    started_at, completed_at, bytes_expected, bytes_downloaded,
                    bytes_per_second, elapsed_ms
             FROM jobs
             ORDER BY updated_at DESC",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok(Job {
                id: row.get(0)?,
                kind: row.get(1)?,
                state: row.get(2)?,
                source_id: row.get(3)?,
                lesson_id: row.get(4)?,
                label: row.get(5)?,
                detail: sanitize_job_detail(&row.get::<_, String>(6)?),
                retry_count: row.get(7)?,
                updated_at: row.get(8)?,
                started_at: row.get(9)?,
                completed_at: row.get(10)?,
                bytes_expected: row.get(11)?,
                bytes_downloaded: row.get(12)?,
                bytes_per_second: row.get(13)?,
                elapsed_ms: row.get(14)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn sanitize_job_detail(detail: &str) -> String {
    detail
        .split_whitespace()
        .map(|token| {
            if looks_like_local_path_token(token) {
                "[local path]".to_string()
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn looks_like_local_path_token(token: &str) -> bool {
    let trimmed = token.trim_matches(|character: char| {
        matches!(
            character,
            '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '.'
        )
    });
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return false;
    }

    trimmed.starts_with("file://")
        || trimmed.starts_with("/Users/")
        || trimmed.starts_with("/Volumes/")
        || trimmed.starts_with("/private/")
        || trimmed.starts_with("/var/folders/")
        || trimmed.starts_with("/tmp/")
        || trimmed.contains("\\Users\\")
        || trimmed.contains(":\\Users\\")
}

fn collect_media_files(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return if is_media_file(path) {
            vec![path.to_path_buf()]
        } else {
            Vec::new()
        };
    }

    if !path.is_dir() {
        return Vec::new();
    }

    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|candidate| is_media_file(candidate))
        .collect()
}

fn is_media_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(is_supported_file_extension)
        .unwrap_or(false)
}

fn is_supported_file_extension(extension: &str) -> bool {
    VIDEO_EXTENSIONS
        .iter()
        .chain(AUDIO_EXTENSIONS.iter())
        .chain(PDF_EXTENSIONS.iter())
        .any(|allowed| allowed.eq_ignore_ascii_case(extension))
}

fn content_type_from_path(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(content_type_from_extension)
        .unwrap_or("video")
        .to_string()
}

fn content_type_from_extension(extension: &str) -> &'static str {
    if VIDEO_EXTENSIONS
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(extension))
    {
        return "video";
    }

    if AUDIO_EXTENSIONS
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(extension))
    {
        return "audio";
    }

    if PDF_EXTENSIONS
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(extension))
    {
        return "pdf";
    }

    "post"
}

fn infer_existing_content_type(
    relative_path: Option<&str>,
    source_url: &str,
    platform: Option<&str>,
    adapter_name: Option<&str>,
) -> Option<ContentTypeInference> {
    if let Some(relative_path) = relative_path {
        if let Some(extension) = Path::new(relative_path)
            .extension()
            .and_then(|extension| extension.to_str())
        {
            let content_type = content_type_from_extension(extension);

            if is_file_backed_content_type(content_type) {
                return Some(ContentTypeInference {
                    content_type,
                    evidence: ContentTypeEvidence::FilePath,
                });
            }
        }
    }

    if let Some(extension) = extension_from_url(source_url) {
        return Some(ContentTypeInference {
            content_type: content_type_from_extension(&extension),
            evidence: ContentTypeEvidence::SourceUrlExtension,
        });
    }

    if is_probably_video_page(source_url) {
        return Some(ContentTypeInference {
            content_type: "video",
            evidence: ContentTypeEvidence::VideoPage,
        });
    }

    let platform = platform.unwrap_or_default();
    let adapter_name = adapter_name.unwrap_or_default();
    if platform == "telegram"
        || platform == "rss-feed"
        || platform == "teacher-relay"
        || platform == "x"
        || adapter_name == "TelegramPublicPreviewAdapter"
    {
        return Some(ContentTypeInference {
            content_type: "post",
            evidence: ContentTypeEvidence::TextSource,
        });
    }

    None
}

fn should_update_content_type(current: &str, inference: ContentTypeInference) -> bool {
    if !is_valid_content_type(current) {
        return true;
    }

    match inference.evidence {
        ContentTypeEvidence::FilePath | ContentTypeEvidence::SourceUrlExtension => {
            current != inference.content_type
        }
        ContentTypeEvidence::VideoPage => current == "post",
        ContentTypeEvidence::TextSource => current == "video",
    }
}

fn is_valid_content_type(content_type: &str) -> bool {
    matches!(content_type, "video" | "audio" | "pdf" | "post")
}

fn is_safe_http_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn classify_feed_content(
    source_url: &str,
    mime_type: Option<&str>,
    description: Option<&str>,
) -> String {
    if let Some(mime_type) = mime_type {
        let normalized = mime_type.to_ascii_lowercase();

        if normalized.starts_with("video/") {
            return "video".to_string();
        }
        if normalized.starts_with("audio/") {
            return "audio".to_string();
        }
        if normalized.contains("pdf") {
            return "pdf".to_string();
        }
    }

    if let Some(extension) = extension_from_url(source_url) {
        let content_type = content_type_from_extension(&extension);
        if content_type != "post" {
            return content_type.to_string();
        }
    }

    if is_probably_video_page(source_url) {
        return "video".to_string();
    }

    if description
        .map(|value| value.contains("<video") || value.contains("<audio"))
        .unwrap_or(false)
    {
        return "video".to_string();
    }

    "post".to_string()
}

fn is_file_backed_mime_type(mime_type: &str) -> bool {
    let normalized = mime_type.to_ascii_lowercase();
    normalized.starts_with("video/")
        || normalized.starts_with("audio/")
        || normalized.contains("pdf")
}

fn extension_from_url(source_url: &str) -> Option<String> {
    Url::parse(source_url)
        .ok()
        .and_then(|url| {
            Path::new(url.path())
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.to_ascii_lowercase())
        })
        .filter(|extension| is_supported_file_extension(extension))
}

fn is_probably_video_page(source_url: &str) -> bool {
    if source_url.starts_with("lbry://") {
        return true;
    }

    let Ok(url) = Url::parse(source_url) else {
        return false;
    };
    let host = url
        .host_str()
        .map(|host| host.to_ascii_lowercase())
        .unwrap_or_default();

    host.ends_with("youtube.com")
        || host == "youtu.be"
        || host.ends_with("rumble.com")
        || host.ends_with("odysee.com")
}

fn is_file_backed_content_type(content_type: &str) -> bool {
    matches!(content_type, "video" | "audio" | "pdf")
}

fn copy_media_into_library(
    source_path: &Path,
    library_dir: &Path,
) -> Result<(String, String, i64), String> {
    let content_type = content_type_from_path(source_path);
    validate_downloaded_media_file(source_path, &content_type)?;
    let file_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Could not read file name".to_string())?;
    let safe_name = file_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let destination_name = format!("{}-{safe_name}", Uuid::new_v4());
    let destination = library_dir.join(destination_name);

    fs::copy(source_path, &destination).map_err(|error| error.to_string())?;
    let media_path = ensure_webkit_playable_media(&destination, &content_type, library_dir)?;
    if media_path != destination {
        let _ = fs::remove_file(&destination);
    }

    let hash = hash_file(&media_path)?;
    let size = media_path
        .metadata()
        .map_err(|error| error.to_string())?
        .len() as i64;
    let relative_path = media_path
        .strip_prefix(library_dir.parent().unwrap_or(library_dir))
        .unwrap_or(&media_path)
        .to_string_lossy()
        .replace('\\', "/");

    Ok((
        format!("library/{relative_path}"),
        format!("sha256:{hash}"),
        size,
    ))
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let bytes_read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose, Engine as _};
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::{json, Value};
    use std::thread;
    use tiny_http::{Header, Response, Server};

    fn test_connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        run_migrations(&connection).unwrap();
        connection
    }

    fn test_temp_dir(prefix: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn insert_test_lesson(connection: &Connection) {
        connection
            .execute(
                "INSERT INTO sources
                 (id, platform, label, identifier, auth_mode, update_schedule, capability_json, enabled)
                 VALUES ('source-test', 'rss-feed', 'Test Source', 'https://example.test/feed.xml',
                         'none', 'Manual', ?1, 1)",
                params![
                    serde_json::to_string(&capability(
                        "supported",
                        "limited",
                        "supported",
                        false,
                        "none",
                        "stable",
                        "Test"
                    ))
                    .unwrap()
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO teachers (id, display_name, description, source_links_json)
                 VALUES ('teacher-old', 'Old Teacher', NULL, '[]')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO collections
                 (id, title, owner_label, sort_order, lesson_count, source_ids_json)
                 VALUES ('collection-old', 'Old Course', 'Test', 1, 1, '[\"source-test\"]')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO lessons
                 (id, title, content_type, teacher_id, collection_id, source_id, source_url,
                  description, thumbnail_tone, duration_seconds, media_file_id, provenance_id)
                 VALUES ('lesson-test', 'Opening Class', 'video', 'teacher-old', 'collection-old',
                         'source-test', 'https://example.test/lesson.mp4', 'Intro',
                         'emerald', NULL, NULL, 'prov-test')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO lessons_fts
                 (lesson_id, title, description, teacher, collection_title, source_label)
                 VALUES ('lesson-test', 'Opening Class', 'Intro', 'Old Teacher',
                         'Old Course', 'Test Source')",
                [],
            )
            .unwrap();
    }

    fn attach_test_media_record(
        connection: &Connection,
        media_file_id: &str,
        content_type: &str,
        relative_path: &str,
    ) {
        connection
            .execute(
                "UPDATE lessons
                 SET content_type = ?1, media_file_id = ?2
                 WHERE id = 'lesson-test'",
                params![content_type, media_file_id],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO media_files
                 (id, lesson_id, relative_path, thumbnail_relative_path, content_hash,
                  size_bytes, codec, import_status, hash_verification_state)
                 VALUES (?1, 'lesson-test', ?2, NULL, 'sha256:test', 12, NULL, 'ready', 'verified')",
                params![media_file_id, relative_path],
            )
            .unwrap();
    }

    #[test]
    fn save_watch_state_upserts_progress_duration_and_completion() {
        let connection = test_connection();
        insert_test_lesson(&connection);

        let saved =
            save_watch_state(&connection, "lesson-test".to_string(), 96, Some(100), false).unwrap();

        assert!(saved.completed);
        assert_eq!(saved.progress_seconds, 100);
        let duration: i64 = connection
            .query_row(
                "SELECT duration_seconds FROM lessons WHERE id = 'lesson-test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(duration, 100);

        let updated =
            save_watch_state(&connection, "lesson-test".to_string(), 12, None, false).unwrap();

        assert!(!updated.completed);
        assert_eq!(updated.progress_seconds, 12);
    }

    #[test]
    fn save_lesson_note_upserts_and_removes_empty_notes() {
        let connection = test_connection();
        insert_test_lesson(&connection);

        let saved = save_lesson_note(
            &connection,
            "lesson-test".to_string(),
            "  Review sanad point.  ".to_string(),
        )
        .unwrap();

        assert_eq!(saved.body, "Review sanad point.");
        assert_eq!(fetch_lesson_notes(&connection).unwrap().len(), 1);

        let cleared =
            save_lesson_note(&connection, "lesson-test".to_string(), "   ".to_string()).unwrap();

        assert!(cleared.body.is_empty());
        assert!(fetch_lesson_notes(&connection).unwrap().is_empty());
    }

    #[test]
    fn pdf_open_resolver_accepts_valid_library_pdf() {
        let connection = test_connection();
        insert_test_lesson(&connection);
        let data_dir = test_temp_dir("duroos-pdf-open");
        let relative_path = "library/imports/lesson.pdf";
        let pdf_path = data_dir.join(relative_path);
        fs::create_dir_all(pdf_path.parent().unwrap()).unwrap();
        fs::write(&pdf_path, b"%PDF-1.4\n%test").unwrap();
        attach_test_media_record(&connection, "media-pdf", "pdf", relative_path);

        let (_media_id, resolved_path, lesson_id, title) =
            resolve_validated_pdf_record_from_connection(&connection, &data_dir, "media-pdf")
                .unwrap();

        assert_eq!(resolved_path, pdf_path);
        assert_eq!(lesson_id, "lesson-test");
        assert_eq!(title, "Opening Class");
        fs::remove_dir_all(data_dir).ok();
    }

    #[test]
    fn pdf_open_resolver_rejects_non_pdf_lessons() {
        let connection = test_connection();
        insert_test_lesson(&connection);
        attach_test_media_record(
            &connection,
            "media-video",
            "video",
            "library/imports/lesson.mp4",
        );

        let error = resolve_validated_pdf_record_from_connection(
            &connection,
            Path::new("/tmp"),
            "media-video",
        )
        .unwrap_err();

        assert!(error.contains("only for PDF lessons"));
    }

    #[test]
    fn pdf_open_resolver_rejects_paths_outside_library() {
        let connection = test_connection();
        insert_test_lesson(&connection);
        let data_dir = test_temp_dir("duroos-pdf-outside");
        attach_test_media_record(&connection, "media-pdf", "pdf", "../lesson.pdf");

        let error =
            resolve_validated_pdf_record_from_connection(&connection, &data_dir, "media-pdf")
                .unwrap_err();

        assert!(error.contains("outside the app library"));
        fs::remove_dir_all(data_dir).ok();
    }

    #[test]
    fn update_lesson_organization_rewrites_metadata_counts_and_search() {
        let mut connection = test_connection();
        insert_test_lesson(&connection);

        let updated = update_lesson_organization(
            &mut connection,
            "lesson-test".to_string(),
            "New Teacher".to_string(),
            "New Course".to_string(),
        )
        .unwrap();

        assert_ne!(updated.teacher_id, "teacher-old");
        assert_ne!(updated.collection_id, "collection-old");
        let new_count: i64 = connection
            .query_row(
                "SELECT lesson_count FROM collections WHERE id = ?1",
                params![updated.collection_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(new_count, 1);
        let old_collection: Option<String> = connection
            .query_row(
                "SELECT id FROM collections WHERE id = 'collection-old'",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        assert_eq!(old_collection.as_deref(), Some("collection-old"));
        let matches = search_lessons(&connection, "New Teacher").unwrap();
        assert_eq!(matches[0].id, "lesson-test");
    }

    #[test]
    fn update_lesson_organization_reattaches_existing_feed_owned_rows() {
        let mut connection = test_connection();
        insert_test_lesson(&connection);

        update_lesson_organization(
            &mut connection,
            "lesson-test".to_string(),
            "New Teacher".to_string(),
            "New Course".to_string(),
        )
        .unwrap();
        let restored = update_lesson_organization(
            &mut connection,
            "lesson-test".to_string(),
            " Old   Teacher ".to_string(),
            "Old Course".to_string(),
        )
        .unwrap();

        assert_eq!(restored.teacher_id, "teacher-old");
        assert_eq!(restored.collection_id, "collection-old");
        let duplicate_teachers: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM teachers
                 WHERE id LIKE 'teacher-user-%' AND lower(display_name) = 'old teacher'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let duplicate_collections: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM collections
                 WHERE id LIKE 'collection-user-%' AND lower(title) = 'old course'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(duplicate_teachers, 0);
        assert_eq!(duplicate_collections, 0);
    }

    #[test]
    fn infers_local_organization_from_explicit_labels_only() {
        let bracketed = infer_local_lesson_organization(Path::new(
            "/tmp/[Teacher Name] Course Name - Lesson One.mp4",
        ));
        assert_eq!(bracketed.teacher_label, "Teacher Name");
        assert_eq!(bracketed.collection_title, "Course Name");
        assert_eq!(bracketed.title, "Lesson One");

        let dashed = infer_local_lesson_organization(Path::new(
            "/tmp/Teacher Name - Course Name - Lesson Two.mp3",
        ));
        assert_eq!(dashed.teacher_label, "Teacher Name");
        assert_eq!(dashed.collection_title, "Course Name");
        assert_eq!(dashed.title, "Lesson Two");

        let generic = infer_local_lesson_organization(Path::new(
            "/Users/traveler/Downloads/Loose Lesson.mp4",
        ));
        assert_eq!(generic.teacher_label, "Personal Library");
        assert_eq!(generic.collection_title, "Local Imports");
    }

    fn signed_manifest() -> Value {
        let signing_key = SigningKey::from_bytes(&[9_u8; 32]);
        let mut manifest = json!({
            "schemaVersion": 2,
            "exportedAt": "2026-06-16T05:00:00Z",
            "curator": {
                "id": "curator-foundations",
                "displayName": "Foundations Curator",
                "publicKey": general_purpose::STANDARD.encode(signing_key.verifying_key().to_bytes())
            },
            "collection": {
                "title": "Foundations Class",
                "ownerLabel": "Foundations Curator"
            },
            "lessons": [
                {
                    "title": "Opening lesson",
                    "contentType": "video",
                    "sourceRefs": [
                        {
                            "platform": "youtube",
                            "originUrl": "https://youtube.com/watch?v=abc123"
                        }
                    ],
                    "retrievalRefs": [
                        {
                            "kind": "enclosure-url",
                            "url": "https://example.org/opening.mp4",
                            "mediaType": "video/mp4"
                        },
                        {
                            "kind": "direct-url",
                            "url": "https://blossom.example/opening.mp4",
                            "service": "blossom",
                            "sha256": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                            "sizeBytes": 2048,
                            "mimeType": "video/mp4",
                            "mediaType": "video/mp4"
                        },
                        {
                            "kind": "ipfs-cid",
                            "cid": "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
                            "gatewayUrl": "https://gateway.example/ipfs",
                            "sha256": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                            "sizeBytes": 2048,
                            "mimeType": "video/mp4",
                            "mediaType": "video/mp4"
                        }
                    ],
                    "contentHashes": [
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    ],
                    "provenance": {
                        "adapterName": "DuroosManifestAdapter",
                        "permissionNote": "Redistributable by the curator."
                    }
                }
            ]
        });
        let payload = manifest::canonical_json_for_test(&manifest).unwrap();
        let signature = signing_key.sign(payload.as_bytes());
        manifest.as_object_mut().unwrap().insert(
            "signature".to_string(),
            json!({
                "algorithm": "ed25519",
                "publicKey": general_purpose::STANDARD.encode(signing_key.verifying_key().to_bytes()),
                "value": general_purpose::STANDARD.encode(signature.to_bytes())
            }),
        );
        manifest
    }

    fn insert_signed_source(connection: &Connection, public_key: &str) {
        let feed_url = "https://example.org/foundations.duroos.json";
        connection
            .execute(
                "INSERT INTO sources
                 (id, platform, label, identifier, feed_format, feed_transport, trust_state,
                  trusted_curator_id, auth_mode, update_schedule, capability_json, enabled,
                  last_checked_at, last_verified_at)
                 VALUES (?1, 'teacher-relay', 'Channel: Foundations', ?2, 'duroos-manifest',
                  'https', 'signed-untrusted', NULL, 'none', 'manual', ?3, 1, NULL, ?4)",
                params![
                    "source-teacher-relay-test",
                    feed_url,
                    serde_json::to_string(&capability(
                        "supported",
                        "limited",
                        "supported",
                        false,
                        "none",
                        "stable",
                        "Signed Duroos manifest."
                    ))
                    .unwrap(),
                    Utc::now().to_rfc3339()
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO teachers (id, display_name, description, source_links_json)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    "teacher-curator-test",
                    "Foundations Curator",
                    format!("Signed curator feed key: {public_key}"),
                    serde_json::to_string(&vec![feed_url]).unwrap()
                ],
            )
            .unwrap();
    }

    #[test]
    fn trusted_curator_promotes_database_aware_manifest_validation() {
        let mut connection = test_connection();
        let manifest = signed_manifest();
        let report = validate_collection_manifest(&connection, &manifest.to_string()).unwrap();

        assert!(report.valid, "{:?}", report.errors);
        assert_eq!(report.trust_state.as_deref(), Some("signed-untrusted"));
        let curator = report.curator.unwrap();

        let trusted = add_trusted_curator(
            &mut connection,
            curator.display_name,
            curator.public_key,
            Some("Verified out of band.".to_string()),
        )
        .unwrap();
        let trusted_report =
            validate_collection_manifest(&connection, &manifest.to_string()).unwrap();

        assert_eq!(
            trusted_report.trust_state.as_deref(),
            Some("signed-trusted")
        );
        assert_eq!(
            trusted_report.trusted_curator_id.as_deref(),
            Some(trusted.id.as_str())
        );
    }

    #[test]
    fn duplicate_trusted_curator_public_keys_do_not_duplicate_rows() {
        let mut connection = test_connection();
        let manifest = signed_manifest();
        let curator = manifest::validate_collection_manifest(&manifest.to_string())
            .curator
            .unwrap();

        let first = add_trusted_curator(
            &mut connection,
            curator.display_name.clone(),
            curator.public_key.clone(),
            None,
        )
        .unwrap();
        let second = add_trusted_curator(
            &mut connection,
            "Updated Curator".to_string(),
            curator.public_key,
            Some("Updated note.".to_string()),
        )
        .unwrap();
        let trusted_curators = fetch_trusted_curators(&connection).unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(trusted_curators.len(), 1);
        assert_eq!(trusted_curators[0].display_name, "Updated Curator");
    }

    #[test]
    fn retrieval_refs_backfill_uses_existing_lesson_source_url() {
        let connection = test_connection();
        insert_test_lesson(&connection);

        backfill_retrieval_refs_json(&connection).unwrap();
        let refs_json: String = connection
            .query_row(
                "SELECT retrieval_refs_json FROM lessons WHERE id = 'lesson-test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let refs = parse_retrieval_refs_json(&refs_json);

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, "direct-url");
        assert_eq!(
            refs[0].url.as_deref(),
            Some("https://example.test/lesson.mp4")
        );
    }

    #[test]
    fn removing_trusted_curator_downgrades_matching_sources() {
        let mut connection = test_connection();
        let manifest = signed_manifest();
        let curator = manifest::validate_collection_manifest(&manifest.to_string())
            .curator
            .unwrap();
        insert_signed_source(&connection, &curator.public_key);

        let trusted = add_trusted_curator(
            &mut connection,
            curator.display_name,
            curator.public_key,
            None,
        )
        .unwrap();
        let trusted_source = fetch_sources(&connection)
            .unwrap()
            .into_iter()
            .find(|source| source.id == "source-teacher-relay-test")
            .unwrap();
        assert_eq!(trusted_source.trust_state, "signed-trusted");
        assert_eq!(
            trusted_source.trusted_curator_id.as_deref(),
            Some(trusted.id.as_str())
        );

        let summary = remove_trusted_curator(&mut connection, trusted.id).unwrap();
        let untrusted_source = fetch_sources(&connection)
            .unwrap()
            .into_iter()
            .find(|source| source.id == "source-teacher-relay-test")
            .unwrap();

        assert_eq!(summary.sources_updated, 1);
        assert_eq!(untrusted_source.trust_state, "signed-untrusted");
        assert_eq!(untrusted_source.trusted_curator_id, None);
    }

    #[test]
    fn ipfs_and_magnet_refs_validate_but_do_not_become_download_urls() {
        let manifest = json!({
            "schemaVersion": 2,
            "exportedAt": "2026-06-16T05:00:00Z",
            "curator": {
                "id": "curator",
                "displayName": "Curator",
                "publicKey": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            },
            "collection": {
                "title": "References",
                "ownerLabel": "Curator"
            },
            "lessons": [
                {
                    "title": "Modeled refs",
                    "contentType": "video",
                    "sourceRefs": [
                        {
                            "platform": "youtube",
                            "originUrl": "https://youtube.com/watch?v=abc123"
                        }
                    ],
                    "retrievalRefs": [
                        {
                            "kind": "ipfs-cid",
                            "cid": "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
                            "mediaType": "video/mp4"
                        },
                        {
                            "kind": "magnet",
                            "magnetUri": "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567",
                            "mediaType": "video/mp4"
                        }
                    ],
                    "contentHashes": [
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    ],
                    "provenance": {
                        "adapterName": "DuroosManifestAdapter"
                    }
                }
            ]
        });

        let report = manifest::validate_collection_manifest(&manifest.to_string());
        let parsed = parse_feed_document(
            &manifest.to_string(),
            "https://example.org/references.duroos.json",
        )
        .unwrap();

        assert!(report.valid, "{:?}", report.errors);
        assert_eq!(
            parsed.lessons[0].source_url,
            "https://youtube.com/watch?v=abc123"
        );
    }

    #[test]
    fn nostr_references_remain_modeled_only() {
        assert!(is_nostr_reference("naddr1example"));
        assert!(is_nostr_reference("nostr:naddr1example"));
        assert!(is_nostr_reference("nostr:npub1example"));
        assert!(is_nostr_reference("nostr+ws://relay.example"));
    }

    #[test]
    fn source_normalization_allows_nostr_channel_refs() {
        assert_eq!(
            normalize_source_input(" naddr1example ").unwrap(),
            "naddr1example"
        );
        assert_eq!(
            normalize_source_input("nostr:naddr1example").unwrap(),
            "nostr:naddr1example"
        );
        assert_eq!(
            normalize_source_input(
                "Duroos channel invite\nOpen in Duroos Watcher: nostr:naddr1example"
            )
            .unwrap(),
            "Duroos channel invite\nOpen in Duroos Watcher: nostr:naddr1example"
        );
    }

    #[test]
    fn parses_public_telegram_channel_and_post_urls() {
        let channel = parse_telegram_source("https://t.me/example_channel").unwrap();
        assert_eq!(channel.username, "example_channel");
        assert_eq!(channel.post_id, None);

        let post = parse_telegram_source("https://t.me/s/example_channel/123").unwrap();
        assert_eq!(post.username, "example_channel");
        assert_eq!(post.post_id.as_deref(), Some("123"));
    }

    #[test]
    fn rejects_private_telegram_invite_paths_for_public_scraping() {
        assert!(parse_telegram_source("https://t.me/+privateInvite").is_none());
        assert!(parse_telegram_source("https://t.me/c/123456/7").is_none());
        assert!(is_probably_telegram_invite("https://t.me/c/123456/7"));
    }

    #[test]
    fn normalizes_youtube_playlist_and_channel_urls_to_feeds() {
        assert_eq!(
            normalize_feed_url("https://www.youtube.com/playlist?list=PL123"),
            "https://www.youtube.com/feeds/videos.xml?playlist_id=PL123"
        );
        assert_eq!(
            normalize_feed_url("https://www.youtube.com/channel/UCabc123"),
            "https://www.youtube.com/feeds/videos.xml?channel_id=UCabc123"
        );
    }

    #[test]
    fn routes_youtube_playlist_and_channel_urls_as_feeds_not_direct_videos() {
        assert_eq!(
            direct_source_platform("https://www.youtube.com/playlist?list=PL123"),
            None
        );
        assert_eq!(
            direct_source_platform("https://www.youtube.com/watch?list=PL123"),
            None
        );
        assert_eq!(
            direct_source_platform("https://www.youtube.com/channel/UCabc123"),
            None
        );
        assert_eq!(
            direct_source_platform("https://www.youtube.com/watch?v=abc123"),
            Some(("youtube", "video"))
        );
        assert_eq!(
            direct_source_platform("https://youtu.be/abc123?list=PL123"),
            Some(("youtube", "video"))
        );
    }

    #[test]
    fn explains_unavailable_youtube_playlist_feeds() {
        let detail = feed_ingest_error_detail(
            "https://www.youtube.com/playlist?list=PL123",
            "Could not fetch https://www.youtube.com/feeds/videos.xml?playlist_id=PL123: HTTP 404 Not Found.",
        );

        assert!(detail.contains("public playlist_id or channel_id"));
        assert!(detail.contains("YouTube exposes through its RSS feed"));
    }

    #[test]
    fn unavailable_youtube_refresh_keeps_existing_source_items() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let feed_url = format!(
            "http://{}/feeds/videos.xml?playlist_id=PL123",
            server.server_addr()
        );
        let server_thread = thread::spawn(move || {
            let request = server.recv().unwrap();
            request
                .respond(Response::from_string("missing").with_status_code(404))
                .unwrap();
        });
        let mut connection = test_connection();
        connection
            .execute(
                "INSERT INTO sources
                 (id, platform, label, identifier, feed_format, feed_transport, trust_state,
                  trusted_curator_id, auth_mode, update_schedule, capability_json, enabled,
                  last_checked_at, last_verified_at)
                 VALUES ('source-youtube-test', 'youtube', 'YouTube: Test Playlist', ?1,
                  'atom', 'https', 'unsigned', NULL, 'none', 'Manual + daily check', ?2, 1,
                  NULL, NULL)",
                params![
                    &feed_url,
                    serde_json::to_string(&capability(
                        "supported",
                        "limited",
                        "limited",
                        false,
                        "none",
                        "best-effort",
                        "Test source"
                    ))
                    .unwrap()
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO teachers (id, display_name, description, source_links_json)
                 VALUES ('teacher-youtube-test', 'Test Teacher', NULL, '[]')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO collections (id, title, owner_label, sort_order, lesson_count, source_ids_json)
                 VALUES ('collection-youtube-test', 'Test Playlist', 'Subscribed feed', 500, 1,
                  '[\"source-youtube-test\"]')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO lessons
                 (id, title, content_type, teacher_id, collection_id, source_id, source_url,
                  published_at, description, thumbnail_tone, duration_seconds, media_file_id,
                  provenance_id)
                 VALUES ('lesson-youtube-test', 'Existing lesson', 'video', 'teacher-youtube-test',
                  'collection-youtube-test', 'source-youtube-test',
                  'https://www.youtube.com/watch?v=abc123', NULL, NULL, 'emerald', NULL, NULL,
                  'prov-youtube-test')",
                [],
            )
            .unwrap();
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let source = RefreshableSource {
            id: "source-youtube-test".to_string(),
            platform: "youtube".to_string(),
            label: "YouTube: Test Playlist".to_string(),
            identifier: feed_url,
        };

        let summary = refresh_youtube_source(&mut connection, &client, &source).unwrap();
        server_thread.join().unwrap();
        let (state, source_id, detail): (String, String, String) = connection
            .query_row(
                "SELECT state, source_id, detail FROM jobs WHERE label = 'Refresh: YouTube: Test Playlist'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert_eq!(summary.failed, 0);
        assert_eq!(state, "skipped");
        assert_eq!(source_id, "source-youtube-test");
        assert!(detail.contains("Kept 1 existing item"));
    }

    #[test]
    fn routes_supported_source_inputs_to_expected_ingest_paths() {
        assert!(parse_telegram_source("https://t.me/public_channel").is_some());
        assert!(parse_archive_org_identifier("https://archive.org/details/class-series").is_some());
        assert!(is_nostr_reference("naddr1example"));
        assert!(is_nostr_reference("nostr:npub1example"));
        assert_eq!(
            platform_for_feed("https://example.org/feed.xml"),
            "rss-feed"
        );
        assert_eq!(
            normalize_feed_url("https://www.youtube.com/playlist?list=PL123"),
            "https://www.youtube.com/feeds/videos.xml?playlist_id=PL123"
        );
        assert_eq!(
            direct_source_platform("https://www.youtube.com/watch?v=abc123"),
            Some(("youtube", "video"))
        );
        assert_eq!(
            direct_source_platform("https://rumble.com/v123-class.html"),
            Some(("rumble", "video"))
        );
        assert_eq!(
            direct_source_platform("https://odysee.com/@teacher/class:1"),
            Some(("odysee", "video"))
        );
        assert_eq!(
            direct_source_platform("https://x.com/teacher/status/123"),
            Some(("x", "post"))
        );
    }

    #[test]
    fn parses_archive_org_item_download_and_metadata_urls() {
        assert_eq!(
            parse_archive_org_identifier("https://archive.org/details/class-series").as_deref(),
            Some("class-series")
        );
        assert_eq!(
            parse_archive_org_identifier("https://archive.org/download/class-series/lesson.mp3")
                .as_deref(),
            Some("class-series")
        );
        assert_eq!(
            parse_archive_org_identifier("https://archive.org/metadata/class-series").as_deref(),
            Some("class-series")
        );
        assert!(parse_archive_org_identifier("https://archive.com/details/class-series").is_none());
    }

    #[test]
    fn builds_archive_org_file_download_urls_with_path_encoding() {
        assert_eq!(
            archive_download_url("class series", "folder/Lesson 01 intro.mp3"),
            "https://archive.org/download/class%20series/folder/Lesson%2001%20intro.mp3"
        );
    }

    #[test]
    fn creates_archive_lessons_from_supported_metadata_files() {
        let file = serde_json::json!({
            "name": "audio/Lesson 01.mp3",
            "format": "VBR MP3",
            "size": "12345",
            "sha1": "abc123"
        });
        let lesson = archive_file_lesson(
            "class-series",
            &file,
            "Class Series",
            Some("Introductory course."),
            Some("2026-06-16"),
        )
        .unwrap();

        assert_eq!(lesson.content_type, "audio");
        assert_eq!(
            lesson.source_url,
            "https://archive.org/download/class-series/audio/Lesson%2001.mp3"
        );
        assert_eq!(lesson.content_hash.as_deref(), Some("sha1:abc123"));
        assert_eq!(lesson.adapter_name, "ArchiveOrgMetadataAdapter");
    }

    #[test]
    fn parses_rss_items_without_requiring_permission_notes() {
        let xml = r#"
          <rss version="2.0">
            <channel>
              <title>Class Feed</title>
              <item>
                <title>Opening Class</title>
                <link>https://example.com/lesson-1</link>
                <description>First lesson in the class.</description>
                <pubDate>Tue, 16 Jun 2026 12:00:00 GMT</pubDate>
              </item>
            </channel>
          </rss>
        "#;
        let document = roxmltree::Document::parse(xml).unwrap();
        let lessons = feed_lessons(&document).unwrap();

        assert_eq!(feed_title(&document).as_deref(), Some("Class Feed"));
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].title, "Opening Class");
        assert_eq!(lessons[0].content_type, "post");
        assert_eq!(lessons[0].provenance_note, PUBLIC_SOURCE_PROVENANCE_NOTE);
    }

    #[test]
    fn classifies_feed_enclosures_as_audio_pdf_or_video() {
        let xml = r#"
          <rss version="2.0">
            <channel>
              <title>Mixed Feed</title>
              <item>
                <title>Audio Lesson</title>
                <enclosure url="https://example.com/lesson.mp3" type="audio/mpeg" />
              </item>
              <item>
                <title>Class Handout</title>
                <enclosure url="https://example.com/handout.pdf" type="application/pdf" />
              </item>
              <item>
                <title>Video Class</title>
                <enclosure url="https://example.com/class.mp4" type="video/mp4" />
              </item>
            </channel>
          </rss>
        "#;
        let document = roxmltree::Document::parse(xml).unwrap();
        let lessons = feed_lessons(&document).unwrap();

        assert_eq!(lessons.len(), 3);
        assert_eq!(lessons[0].content_type, "audio");
        assert_eq!(lessons[1].content_type, "pdf");
        assert_eq!(lessons[2].content_type, "video");
    }

    #[test]
    fn parses_atom_feed_entries() {
        let xml = r#"
          <feed xmlns="http://www.w3.org/2005/Atom">
            <title>Atom Class</title>
            <entry>
              <title>Atom Audio</title>
              <link rel="enclosure" href="https://example.com/atom.mp3" type="audio/mpeg" />
              <summary>Audio lesson.</summary>
              <published>2026-06-16T12:00:00Z</published>
            </entry>
          </feed>
        "#;
        let parsed = parse_feed_document(xml, "https://example.com/feed.atom").unwrap();

        assert_eq!(parsed.feed_format, "atom");
        assert_eq!(parsed.title, "Atom Class");
        assert_eq!(parsed.lessons.len(), 1);
        assert_eq!(parsed.lessons[0].content_type, "audio");
        assert_eq!(parsed.lessons[0].source_url, "https://example.com/atom.mp3");
    }

    #[test]
    fn parses_json_feed_attachments() {
        let feed = serde_json::json!({
            "version": "https://jsonfeed.org/version/1.1",
            "title": "JSON Class",
            "items": [
                {
                    "id": "lesson-1",
                    "title": "JSON Audio",
                    "date_published": "2026-06-16T12:00:00Z",
                    "attachments": [
                        {
                            "url": "https://example.com/json.mp3",
                            "mime_type": "audio/mpeg",
                            "duration_in_seconds": 120
                        }
                    ],
                    "contentHash": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                }
            ]
        });
        let parsed =
            parse_feed_document(&feed.to_string(), "https://example.com/feed.json").unwrap();

        assert_eq!(parsed.feed_format, "json-feed");
        assert_eq!(parsed.title, "JSON Class");
        assert_eq!(parsed.lessons.len(), 1);
        assert_eq!(parsed.lessons[0].content_type, "audio");
        assert_eq!(parsed.lessons[0].duration_seconds, Some(120));
        assert_eq!(
            parsed.lessons[0].content_hash.as_deref(),
            Some("sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
    }

    #[test]
    fn parses_duroos_manifest_retrieval_refs() {
        let manifest = serde_json::json!({
            "schemaVersion": 2,
            "exportedAt": "2026-06-16T05:00:00Z",
            "curator": {
                "id": "curator-foundations",
                "displayName": "Foundations Curator",
                "publicKey": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            },
            "collection": {
                "title": "Foundations Class",
                "ownerLabel": "Foundations Curator"
            },
            "lessons": [
                {
                    "title": "Opening lesson",
                    "contentType": "video",
                    "durationSeconds": 240,
                    "sourceRefs": [
                        {
                            "platform": "youtube",
                            "originUrl": "https://youtube.com/watch?v=abc123",
                            "publishedAt": "2026-06-16T12:00:00Z"
                        }
                    ],
                    "retrievalRefs": [
                        {
                            "kind": "enclosure-url",
                            "url": "https://example.org/opening.mp4",
                            "mediaType": "video/mp4"
                        },
                        {
                            "kind": "direct-url",
                            "url": "https://blossom.example/opening.mp4",
                            "service": "blossom",
                            "sha256": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                            "sizeBytes": 2048,
                            "mimeType": "video/mp4",
                            "mediaType": "video/mp4"
                        },
                        {
                            "kind": "ipfs-cid",
                            "cid": "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
                            "gatewayUrl": "https://gateway.example/ipfs",
                            "sha256": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                            "sizeBytes": 2048,
                            "mimeType": "video/mp4",
                            "mediaType": "video/mp4"
                        }
                    ],
                    "contentHashes": [
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    ],
                    "provenance": {
                        "adapterName": "DuroosManifestAdapter",
                        "permissionNote": "Redistributable by the curator."
                    }
                }
            ]
        });
        let parsed = parse_feed_document(
            &manifest.to_string(),
            "https://example.org/foundations.duroos.json",
        )
        .unwrap();

        assert_eq!(parsed.feed_format, "duroos-manifest");
        assert_eq!(parsed.trust_state, "unsigned");
        assert_eq!(
            parsed
                .curator
                .as_ref()
                .map(|curator| curator.display_name.as_str()),
            Some("Foundations Curator")
        );
        assert_eq!(
            parsed.lessons[0].source_url,
            "https://example.org/opening.mp4"
        );
        assert_eq!(parsed.lessons[0].retrieval_refs.len(), 3);
        assert_eq!(
            parsed.lessons[0].retrieval_refs[2].cid.as_deref(),
            Some("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi")
        );
        assert_eq!(parsed.lessons[0].duration_seconds, Some(240));
    }

    #[test]
    fn retrieval_candidates_try_http_before_ipfs_gateway() {
        let lesson = DownloadLesson {
            id: "lesson-ipfs".to_string(),
            title: "IPFS fallback".to_string(),
            content_type: "video".to_string(),
            source_url: "https://youtube.com/watch?v=abc123".to_string(),
            retrieval_refs: vec![
                RetrievalRef {
                    kind: "direct-url".to_string(),
                    url: Some("https://blossom.example/opening.mp4".to_string()),
                    service: Some("blossom".to_string()),
                    ..Default::default()
                },
                RetrievalRef {
                    kind: "ipfs-cid".to_string(),
                    cid: Some(
                        "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string(),
                    ),
                    gateway_url: Some("https://gateway.example/ipfs".to_string()),
                    ..Default::default()
                },
            ],
            expected_content_hash: Some(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
            ),
            has_invalid_media_record: false,
        };

        let candidates = retrieval_download_candidates(&lesson);

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].url, "https://blossom.example/opening.mp4");
        assert_eq!(
            candidates[1].url,
            "https://gateway.example/ipfs/bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"
        );
        assert_eq!(
            candidates[1].file_name.as_deref(),
            Some("ipfs-bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi.mp4")
        );
    }

    #[test]
    fn rejects_hash_mismatches_before_recording_media() {
        assert_eq!(
            verify_downloaded_hash(
                Some("sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .unwrap(),
            "matched"
        );
        assert!(verify_downloaded_hash(
            Some("sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
            "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        )
        .is_err());
        assert_eq!(
            verify_downloaded_hash(None, "sha256:abc").unwrap(),
            "not-provided"
        );
        assert_eq!(
            verify_downloaded_hash(Some("sha1:abc123"), "sha256:abc").unwrap(),
            "unverified"
        );
    }

    #[test]
    fn generic_feeds_are_user_rss_sources_not_teacher_relays() {
        assert_eq!(
            platform_for_feed("https://example.com/classes/feed.xml"),
            "rss-feed"
        );
    }

    #[test]
    fn infers_existing_content_type_from_local_or_direct_file_evidence() {
        assert_eq!(
            infer_existing_content_type(
                Some("library/imports/lesson.MP3"),
                "local-import://lesson",
                Some("local-files"),
                Some("LocalFilesAdapter"),
            )
            .unwrap(),
            ContentTypeInference {
                content_type: "audio",
                evidence: ContentTypeEvidence::FilePath,
            }
        );
        assert_eq!(
            infer_existing_content_type(
                None,
                "https://example.com/handouts/book.pdf",
                Some("rss-feed"),
                Some("FeedAdapter"),
            )
            .unwrap(),
            ContentTypeInference {
                content_type: "pdf",
                evidence: ContentTypeEvidence::SourceUrlExtension,
            }
        );
    }

    #[test]
    fn infers_existing_text_sources_without_overwriting_stronger_types() {
        let inference = infer_existing_content_type(
            None,
            "https://t.me/example/12",
            Some("telegram"),
            Some("TelegramPublicPreviewAdapter"),
        )
        .unwrap();

        assert_eq!(inference.content_type, "post");
        assert!(should_update_content_type("video", inference));
        assert!(!should_update_content_type("audio", inference));
    }

    #[test]
    fn updates_mismatched_file_evidence_even_when_current_type_is_valid() {
        let inference = ContentTypeInference {
            content_type: "pdf",
            evidence: ContentTypeEvidence::FilePath,
        };

        assert!(should_update_content_type("video", inference));
        assert!(should_update_content_type("audio", inference));
        assert!(!should_update_content_type("pdf", inference));
    }

    #[test]
    fn protects_default_source_rows_from_removal() {
        assert!(is_default_source_id("source-youtube"));
        assert!(is_default_source_id("source-odysee"));
        assert!(!is_default_source_id("source-youtube-abc123"));
    }

    #[test]
    fn classifies_odysee_as_best_effort_source() {
        assert_eq!(
            normalize_source_input("odysee.com/@teacher/class:1").unwrap(),
            "https://odysee.com/@teacher/class:1"
        );
        assert_eq!(
            normalize_source_input("rumble.com/v123-class.html").unwrap(),
            "https://rumble.com/v123-class.html"
        );
        assert_eq!(
            normalize_source_input("x.com/teacher/status/123").unwrap(),
            "https://x.com/teacher/status/123"
        );
        assert_eq!(
            normalize_source_input("lbry://@teacher/class-1").unwrap(),
            "lbry://@teacher/class-1"
        );
        assert_eq!(
            platform_for_feed("https://odysee.com/@teacher/class:1"),
            "odysee"
        );
        assert!(is_probably_video_page(
            "https://odysee.com/@teacher/class:1"
        ));
        assert_eq!(
            direct_source_platform("https://odysee.com/@teacher/class:1"),
            Some(("odysee", "video"))
        );
        assert_eq!(
            direct_source_platform("https://rumble.com/v123-class.html"),
            Some(("rumble", "video"))
        );
        assert_eq!(
            direct_source_platform("https://x.com/teacher/status/123"),
            Some(("x", "post"))
        );
    }

    #[test]
    fn detects_duplicate_discovered_lessons_by_url_hash_and_duration() {
        let mut connection = test_connection();
        let transaction = connection.transaction().unwrap();
        transaction
            .execute(
                "INSERT INTO sources
                 (id, platform, label, identifier, feed_format, feed_transport, trust_state,
                  trusted_curator_id, auth_mode, update_schedule, capability_json, enabled,
                  last_checked_at, last_verified_at)
                 VALUES ('source-youtube', 'youtube', 'YouTube', 'youtube:not-configured',
                  'rss', 'https', 'unsigned', NULL, 'api-key', 'Manual', ?1, 1, NULL, NULL)",
                params![serde_json::to_string(&capability(
                    "supported",
                    "limited",
                    "limited",
                    true,
                    "api-key",
                    "best-effort",
                    "Test source"
                ))
                .unwrap()],
            )
            .unwrap();
        transaction
            .execute(
                "INSERT INTO teachers (id, display_name, description, source_links_json)
                 VALUES ('teacher-3', 'Personal Library', NULL, '[]')",
                [],
            )
            .unwrap();
        transaction
            .execute(
                "INSERT INTO collections (id, title, owner_label, sort_order, lesson_count, source_ids_json)
                 VALUES ('collection-2', 'Local Imports', 'Local archive', 10, 0, '[\"source-youtube\"]')",
                [],
            )
            .unwrap();
        transaction
            .execute(
                "INSERT INTO lessons
                 (id, title, content_type, teacher_id, collection_id, source_id, source_url,
                  published_at, description, thumbnail_tone, duration_seconds, media_file_id,
                  provenance_id)
                 VALUES ('lesson-existing', 'Existing Class', 'video', 'teacher-3',
                  'collection-2', 'source-youtube', 'https://youtube.com/watch?v=abc123',
                  NULL, NULL, 'emerald', 3600, NULL, 'prov-existing')",
                [],
            )
            .unwrap();
        transaction
            .execute(
                "INSERT INTO provenance_records
                 (id, lesson_id, origin_url, permission_note, imported_at, adapter_name, content_hash)
                 VALUES ('prov-existing', 'lesson-existing', 'https://youtube.com/watch?v=abc123',
                  'Test', '2026-06-16T00:00:00Z', 'FeedAdapter',
                  'sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef')",
                [],
            )
            .unwrap();

        let url_duplicate = DiscoveredLesson {
            title: "Different title".to_string(),
            content_type: "video".to_string(),
            source_url: "https://youtube.com/watch?v=abc123".to_string(),
            retrieval_refs: Vec::new(),
            published_at: None,
            description: None,
            duration_seconds: None,
            adapter_name: "FeedAdapter".to_string(),
            provenance_note: "Test".to_string(),
            content_hash: None,
        };
        let hash_duplicate = DiscoveredLesson {
            title: "Hash duplicate".to_string(),
            source_url: "https://example.com/new.mp4".to_string(),
            content_hash: Some(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
            ),
            ..url_duplicate.clone()
        };
        let natural_duplicate = DiscoveredLesson {
            title: "Existing Class".to_string(),
            source_url: "https://rumble.com/example".to_string(),
            content_hash: None,
            duration_seconds: Some(3600),
            ..hash_duplicate.clone()
        };

        assert_eq!(
            discovered_duplicate_lesson_title(&transaction, &url_duplicate)
                .unwrap()
                .as_deref(),
            Some("Existing Class")
        );
        assert_eq!(
            discovered_duplicate_lesson_title(&transaction, &hash_duplicate)
                .unwrap()
                .as_deref(),
            Some("Existing Class")
        );
        assert_eq!(
            discovered_duplicate_lesson_title(&transaction, &natural_duplicate)
                .unwrap()
                .as_deref(),
            Some("Existing Class")
        );
    }

    #[test]
    fn sanitizes_source_ids_for_download_paths() {
        assert_eq!(
            safe_path_segment("source-youtube PL 123/../bad"),
            "source-youtube-PL-123-..-bad"
        );
        assert_eq!(safe_path_segment("///"), "item");
    }

    #[test]
    fn detects_app_local_yt_dlp_cookie_file() {
        let data_dir = test_temp_dir("duroos-cookies");
        fs::write(data_dir.join("cookies.txt"), "# Netscape cookies").unwrap();
        fs::write(
            data_dir.join("yt-dlp-cookies.txt"),
            "# Preferred Netscape cookies",
        )
        .unwrap();

        let detected = yt_dlp_cookie_file(&data_dir).unwrap();
        assert_eq!(
            detected
                .file_name()
                .and_then(|file_name| file_name.to_str()),
            Some("yt-dlp-cookies.txt")
        );

        fs::remove_dir_all(data_dir).ok();
    }

    #[test]
    fn yt_dlp_auth_hint_explains_cookie_and_manual_import_workarounds() {
        let data_dir = PathBuf::from("/tmp/Duroos Watcher Test");
        let missing_cookie_hint = yt_dlp_auth_hint(None, &data_dir);
        assert!(missing_cookie_hint.contains("yt-dlp-cookies.txt"));
        assert!(missing_cookie_hint.contains("manual"));

        let configured_cookie_hint =
            yt_dlp_auth_hint(Some(&data_dir.join("cookies.txt")), &data_dir);
        assert!(configured_cookie_hint.contains("Local cookies were used"));
        assert!(configured_cookie_hint.contains("refresh"));
    }

    #[test]
    fn yt_dlp_video_selector_prefers_webkit_playable_codecs() {
        let selector = yt_dlp_video_format_selector();

        assert!(selector.contains("vcodec^=avc1"));
        assert!(selector.contains("acodec^=mp4a"));
        assert!(!selector.contains("vcodec!=none"));
    }

    #[test]
    fn native_player_candidates_prefer_bundled_players() {
        let bundle_dir = PathBuf::from("/tmp/duroos-player-bundle");
        let bundled_mpv = bundle_dir.join("mpv").to_string_lossy().to_string();
        let bundled_mpv_exe = bundle_dir.join("mpv.exe").to_string_lossy().to_string();
        let candidates = native_player_candidates(std::slice::from_ref(&bundle_dir));

        assert_eq!(candidates[0].name, "mpv");
        assert_eq!(candidates[0].program, bundled_mpv);
        assert!(candidates
            .iter()
            .any(|candidate| { candidate.name == "mpv" && candidate.program == bundled_mpv_exe }));
        assert!(candidates.iter().any(|candidate| {
            candidate.name == "VLC"
                && candidate.program == "/Applications/VLC.app/Contents/MacOS/VLC"
        }));
        assert!(candidates.iter().any(|candidate| {
            candidate.name == "ffplay" && candidate.program == "/opt/homebrew/bin/ffplay"
        }));
        assert!(candidates.iter().any(|candidate| {
            candidate.name == "ffplay" && candidate.program == "/usr/bin/ffplay"
        }));
    }

    #[test]
    fn native_player_candidates_do_not_add_vlc_startup_flags() {
        let candidates = native_player_candidates(&[]);

        assert!(candidates
            .iter()
            .filter(|candidate| candidate.name == "VLC")
            .all(|candidate| candidate.args.is_empty()));
    }

    #[test]
    fn native_player_command_label_preserves_arguments() {
        let command = NativePlayerCommand {
            name: "ffplay".to_string(),
            program: "/opt/homebrew/bin/ffplay".to_string(),
            args: vec![
                "-hide_banner".to_string(),
                "-loglevel".to_string(),
                "warning".to_string(),
            ],
        };

        assert_eq!(
            native_player_command_label(&command),
            "/opt/homebrew/bin/ffplay -hide_banner -loglevel warning"
        );
    }

    #[test]
    fn native_player_launch_reports_immediate_exit() {
        #[cfg(target_os = "windows")]
        let (program, args) = (
            "cmd".to_string(),
            vec![
                "/C".to_string(),
                "echo launch failed 1>&2 & exit /B 42".to_string(),
            ],
        );
        #[cfg(not(target_os = "windows"))]
        let (program, args) = (
            "/bin/sh".to_string(),
            vec![
                "-c".to_string(),
                "echo launch failed >&2; exit 42".to_string(),
            ],
        );
        let command = NativePlayerCommand {
            name: "failing-player".to_string(),
            program,
            args,
        };

        let media_path = env::temp_dir().join("duroos-test.mp4");
        let error = spawn_native_player_checked(&command, &media_path).unwrap_err();

        assert!(error.contains("exited immediately"));
        assert!(error.contains("launch failed"));
    }

    #[test]
    fn treats_yt_dlp_fragments_as_partial_media_files() {
        assert!(is_probably_partial_media_file(Path::new(
            "Lecture_10-xGSBvoVScqE.f399.mp4"
        )));
        assert!(is_probably_partial_media_file(Path::new(
            "Lecture_10-xGSBvoVScqE.f251.webm"
        )));
        assert!(is_probably_partial_media_file(Path::new("lesson.mp4.part")));
        assert!(!is_probably_partial_media_file(Path::new(
            "Lecture_10-xGSBvoVScqE.mp4"
        )));
    }

    #[test]
    fn rejects_html_disguised_as_video_file() {
        let data_dir = test_temp_dir("duroos-html-video");
        let video_path = data_dir.join("lesson.mp4");
        fs::write(&video_path, b"<html><body>not video</body></html>").unwrap();

        let error = validate_downloaded_media_file(&video_path, "video").unwrap_err();

        assert!(error.contains("playable video"));
        fs::remove_dir_all(data_dir).ok();
    }

    #[test]
    fn accepts_plausible_unrecognized_binary_video_payloads() {
        let data_dir = test_temp_dir("duroos-binary-video");
        let video_path = data_dir.join("lesson.mp4");
        let mut payload = vec![0_u8; MIN_PLAUSIBLE_MEDIA_BYTES as usize];
        payload[31] = 0x80;
        fs::write(&video_path, payload).unwrap();

        validate_downloaded_media_file(&video_path, "video").unwrap();
        fs::remove_dir_all(data_dir).ok();
    }

    #[test]
    fn profiles_h264_aac_mp4_as_webkit_compatible() {
        let data_dir = test_temp_dir("duroos-compatible-video");
        let video_path = data_dir.join("lesson.mp4");
        let mut payload = vec![0_u8; MIN_PLAUSIBLE_MEDIA_BYTES as usize];
        payload[4..8].copy_from_slice(b"ftyp");
        payload[128..132].copy_from_slice(b"avc1");
        payload[256..260].copy_from_slice(b"mp4a");
        fs::write(&video_path, payload).unwrap();

        let profile = playback_profile_for_media_file(&video_path, "video").unwrap();

        assert_eq!(profile, "webkit-compatible:h264-aac-mp4");
        fs::remove_dir_all(data_dir).ok();
    }

    #[test]
    fn profiles_unknown_binary_video_as_unverified() {
        let data_dir = test_temp_dir("duroos-unverified-video");
        let video_path = data_dir.join("lesson.mp4");
        let mut payload = vec![0_u8; MIN_PLAUSIBLE_MEDIA_BYTES as usize];
        payload[31] = 0x80;
        fs::write(&video_path, payload).unwrap();

        let profile = playback_profile_for_media_file(&video_path, "video").unwrap();

        assert_eq!(profile, "webkit-unverified:mp4");
        fs::remove_dir_all(data_dir).ok();
    }

    #[test]
    fn rejects_tiny_unrecognized_binary_video_payloads() {
        let data_dir = test_temp_dir("duroos-tiny-video");
        let video_path = data_dir.join("lesson.mp4");
        fs::write(&video_path, [0_u8, 0x80, 0x01]).unwrap();

        let error = validate_downloaded_media_file(&video_path, "video").unwrap_err();

        assert!(error.contains("playable video"));
        fs::remove_dir_all(data_dir).ok();
    }

    #[test]
    fn rejects_text_html_direct_download_before_recording_media() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/lesson.mp4", server.server_addr());
        let handle = thread::spawn(move || {
            let request = server.recv().unwrap();
            let header = Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap();
            request
                .respond(Response::from_string("<html>login</html>").with_header(header))
                .unwrap();
        });
        let data_dir = test_temp_dir("duroos-direct-html");
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        let error = download_direct_file(&client, &url, &data_dir, "video").unwrap_err();

        assert!(error.contains("text/html"));
        assert!(collect_media_files(&data_dir).is_empty());
        handle.join().unwrap();
        fs::remove_dir_all(data_dir).ok();
    }

    #[test]
    fn existing_completed_media_file_ignores_adaptive_fragments() {
        let data_dir = test_temp_dir("duroos-fragments");
        fs::write(
            data_dir.join("Lecture_10-xGSBvoVScqE.f399.mp4"),
            b"\0\0\0\x18ftypisom\0\0\0\0",
        )
        .unwrap();

        assert!(existing_completed_media_file(&data_dir, "video").is_none());
        fs::remove_dir_all(data_dir).ok();
    }

    #[test]
    fn existing_completed_media_file_prefers_compatible_outputs() {
        let data_dir = test_temp_dir("duroos-compatible-output");
        let compatible_path = data_dir.join("lesson-webkit.mp4");
        let unverified_path = data_dir.join("lesson-original.mp4");
        let mut compatible_payload = vec![0_u8; MIN_PLAUSIBLE_MEDIA_BYTES as usize];
        compatible_payload[4..8].copy_from_slice(b"ftyp");
        compatible_payload[128..132].copy_from_slice(b"avc1");
        compatible_payload[256..260].copy_from_slice(b"mp4a");
        fs::write(&compatible_path, compatible_payload).unwrap();
        thread::sleep(Duration::from_millis(10));
        let mut unverified_payload = vec![0_u8; MIN_PLAUSIBLE_MEDIA_BYTES as usize];
        unverified_payload[31] = 0x80;
        fs::write(&unverified_path, unverified_payload).unwrap();

        let selected = existing_completed_media_file(&data_dir, "video").unwrap();

        assert_eq!(selected, compatible_path);
        fs::remove_dir_all(data_dir).ok();
    }

    #[test]
    fn source_download_plan_retries_invalid_existing_media_records() {
        let connection = test_connection();
        let data_dir = test_temp_dir("duroos-invalid-existing");
        let media_dir = data_dir.join("library/downloads/source-test/lesson-test");
        fs::create_dir_all(&media_dir).unwrap();
        fs::write(media_dir.join("lesson.mp4"), b"<html>not video</html>").unwrap();

        connection
            .execute(
                "INSERT INTO sources
                 (id, platform, label, identifier, feed_format, feed_transport, trust_state,
                  trusted_curator_id, auth_mode, update_schedule, capability_json, enabled,
                  last_checked_at, last_verified_at)
                 VALUES ('source-test', 'archive-org', 'Archive Test', 'https://archive.org/details/test',
                  'json-feed', 'https', 'unsigned', NULL, 'none', 'Manual', ?1, 1, NULL, NULL)",
                params![serde_json::to_string(&capability(
                    "supported",
                    "supported",
                    "supported",
                    false,
                    "none",
                    "stable",
                    "Test source"
                ))
                .unwrap()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO teachers (id, display_name, description, source_links_json)
                 VALUES ('teacher-test', 'Teacher', NULL, '[]')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO collections (id, title, owner_label, sort_order, lesson_count, source_ids_json)
                 VALUES ('collection-test', 'Collection', 'Owner', 1, 1, '[\"source-test\"]')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO lessons
                 (id, title, content_type, teacher_id, collection_id, source_id, source_url,
                  published_at, description, thumbnail_tone, duration_seconds, media_file_id,
                  provenance_id)
                 VALUES ('lesson-test', 'Lesson', 'video', 'teacher-test', 'collection-test',
                  'source-test', 'https://archive.org/download/test/lesson.mp4', NULL, NULL,
                  'emerald', NULL, 'media-test', 'prov-test')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO media_files
                 (id, lesson_id, relative_path, content_hash, size_bytes, codec, import_status,
                  hash_verification_state)
                 VALUES ('media-test', 'lesson-test',
                  'library/downloads/source-test/lesson-test/lesson.mp4',
                  'sha256:bad', 22, NULL, 'ready', 'not-provided')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO provenance_records
                 (id, lesson_id, origin_url, permission_note, imported_at, adapter_name, content_hash)
                 VALUES ('prov-test', 'lesson-test', 'https://archive.org/download/test/lesson.mp4',
                  'Test', '2026-06-17T00:00:00Z', 'ArchiveOrgMetadataAdapter', NULL)",
                [],
            )
            .unwrap();

        let (lessons, skipped) =
            source_download_plan(&connection, &data_dir, "source-test").unwrap();

        assert_eq!(skipped, 0);
        assert_eq!(lessons.len(), 1);
        assert!(lessons[0].has_invalid_media_record);
        fs::remove_dir_all(data_dir).ok();
    }

    #[test]
    fn summarizes_last_stderr_lines() {
        assert_eq!(output_summary(b""), "no stderr output");
        assert_eq!(output_summary(b"one\ntwo\nthree\nfour"), "two three four");
    }

    #[test]
    fn resolves_only_safe_library_media_paths() {
        let data_dir = Path::new("/tmp/duroos-test");

        assert_eq!(
            resolve_library_media_path(data_dir, "library/imports/lesson.mp4"),
            Some(PathBuf::from("/tmp/duroos-test/library/imports/lesson.mp4"))
        );
        assert_eq!(resolve_library_media_path(data_dir, "../secret.mp4"), None);
        assert_eq!(
            resolve_library_media_path(data_dir, "/tmp/secret.mp4"),
            None
        );
        assert_eq!(
            resolve_library_media_path(data_dir, "outside/lesson.mp4"),
            None
        );
    }

    #[test]
    fn media_storage_audit_reports_unreferenced_library_files_without_absolute_paths() {
        let connection = test_connection();
        insert_test_lesson(&connection);
        let data_dir = test_temp_dir("duroos-storage-audit");
        let media_dir = data_dir.join("library/imports");
        fs::create_dir_all(&media_dir).unwrap();
        fs::write(media_dir.join("referenced.mp4"), vec![1_u8; 2048]).unwrap();
        fs::write(media_dir.join("stale.mp4.part"), vec![2_u8; 512]).unwrap();
        connection
            .execute(
                "INSERT INTO media_files
                 (id, lesson_id, relative_path, content_hash, size_bytes, codec, import_status,
                  hash_verification_state)
                 VALUES ('media-test', 'lesson-test', 'library/imports/referenced.mp4',
                  'sha256:test', 2048, NULL, 'ready', 'not-provided')",
                [],
            )
            .unwrap();

        let detail = media_storage_audit_detail(&connection, &data_dir).unwrap();

        assert_eq!(detail.audit.scanned_files, 2);
        assert_eq!(detail.audit.referenced_files, 1);
        assert_eq!(detail.audit.stale_files, 1);
        assert_eq!(detail.audit.partial_files, 1);
        assert_eq!(
            detail.audit.stale_samples,
            vec!["library/imports/stale.mp4.part".to_string()]
        );
        assert_eq!(detail.audit.stale_items.len(), 1);
        assert_eq!(
            detail.audit.stale_items[0].relative_path,
            "library/imports/stale.mp4.part"
        );
        assert_eq!(detail.audit.stale_items[0].category, "partial-fragment");
        assert!(!detail.audit.messages.join(" ").contains("/Users/"));
        fs::remove_dir_all(data_dir).ok();
    }

    #[test]
    fn cleanup_modes_only_match_their_allowed_stale_categories() {
        assert!(cleanup_mode_matches(
            "partial-fragments",
            "partial-fragment"
        ));
        assert!(!cleanup_mode_matches(
            "partial-fragments",
            "old-source-download"
        ));
        assert!(cleanup_mode_matches(
            "old-source-downloads",
            "old-source-download"
        ));
        assert!(!cleanup_mode_matches(
            "old-source-downloads",
            "unreferenced"
        ));
        assert!(cleanup_mode_matches("all-stale", "unreferenced"));
    }

    #[test]
    fn job_detail_sanitizer_redacts_local_paths_without_touching_urls() {
        let detail = sanitize_job_detail(
            "Saved /Users/traveler/private/lesson.mp4 from https://example.test/watch",
        );

        assert!(detail.contains("[local path]"));
        assert!(detail.contains("https://example.test/watch"));
        assert!(!detail.contains("/Users/traveler"));
    }
}
