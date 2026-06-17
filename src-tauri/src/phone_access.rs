use crate::{
    db,
    models::{PhoneMediaScope, PhoneMediaSession, PhoneMediaShareItem},
};
use chrono::Utc;
use rusqlite::params;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    net::{IpAddr, Ipv4Addr, UdpSocket},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tauri::AppHandle;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use uuid::Uuid;

const PLAYLIST_PATH: &str = "/playlist.m3u";
const MEDIA_PREFIX: &str = "/media/";

#[derive(Default)]
pub struct PhoneAccessState {
    session: Mutex<Option<ActivePhoneSession>>,
}

struct ActivePhoneSession {
    summary: PhoneMediaSession,
    server: Arc<Server>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Clone)]
struct ServedMediaItem {
    share: PhoneMediaShareItem,
    path: PathBuf,
    mime_type: String,
}

#[derive(Debug, PartialEq, Eq)]
struct ByteRange {
    start: u64,
    end: u64,
}

pub fn start_session(
    app: &AppHandle,
    state: &PhoneAccessState,
    scope: Option<PhoneMediaScope>,
) -> Result<PhoneMediaSession, String> {
    let mut guard = state
        .session
        .lock()
        .map_err(|_| "Phone access state is unavailable.".to_string())?;

    if let Some(active) = guard.take() {
        active.stop();
    }

    let media = eligible_media_items(app, scope)?;
    if media.is_empty() {
        return Err("No downloaded audio or video files are ready for phone access.".to_string());
    }

    let server = Arc::new(
        Server::http("0.0.0.0:0")
            .map_err(|error| format!("Could not start phone access: {error}"))?,
    );
    let port = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| "Phone access did not bind to a network port.".to_string())?
        .port();
    let host = local_network_ip().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let base_url = format!("http://{host}:{port}");
    let session_id = format!("phone-session-{}", Uuid::new_v4());
    let token = Uuid::new_v4().to_string();
    let playlist_url = format!("{base_url}{PLAYLIST_PATH}?token={token}");
    let item_count = media.len() as i64;
    let items = media
        .iter()
        .map(|item| item.share.clone())
        .collect::<Vec<_>>();
    let messages = if host.is_loopback() {
        vec![
            "Could not detect a same-Wi-Fi address. The link may only work on this computer."
                .to_string(),
        ]
    } else {
        vec!["Keep this app open while watching on your phone.".to_string()]
    };
    let summary = PhoneMediaSession {
        id: session_id,
        active: true,
        base_url: Some(base_url.clone()),
        playlist_url: Some(playlist_url),
        started_at: Some(Utc::now().to_rfc3339()),
        item_count,
        items,
        messages,
    };

    let stop = Arc::new(AtomicBool::new(false));
    let media_items = Arc::new(media);
    let worker = spawn_server_worker(server.clone(), stop.clone(), media_items, base_url, token);

    *guard = Some(ActivePhoneSession {
        summary: summary.clone(),
        server,
        stop,
        worker: Some(worker),
    });

    Ok(summary)
}

pub fn current_session(state: &PhoneAccessState) -> Result<Option<PhoneMediaSession>, String> {
    let guard = state
        .session
        .lock()
        .map_err(|_| "Phone access state is unavailable.".to_string())?;

    Ok(guard.as_ref().map(|session| session.summary.clone()))
}

pub fn stop_session(
    state: &PhoneAccessState,
    session_id: String,
) -> Result<PhoneMediaSession, String> {
    let mut guard = state
        .session
        .lock()
        .map_err(|_| "Phone access state is unavailable.".to_string())?;
    let active = guard
        .take()
        .ok_or_else(|| "Phone access is not running.".to_string())?;

    if active.summary.id != session_id {
        let current = active.summary.clone();
        *guard = Some(active);
        return Err(format!(
            "Phone access session {} is still running.",
            current.id
        ));
    }

    let mut summary = active.summary.clone();
    active.stop();
    summary.active = false;
    summary.messages = vec!["Phone access stopped.".to_string()];

    Ok(summary)
}

fn spawn_server_worker(
    server: Arc<Server>,
    stop: Arc<AtomicBool>,
    media_items: Arc<Vec<ServedMediaItem>>,
    base_url: String,
    token: String,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match server.recv_timeout(Duration::from_millis(250)) {
                Ok(Some(request)) => {
                    handle_request(request, &media_items, &base_url, &token);
                }
                Ok(None) => {}
                Err(_) => break,
            }
        }
    })
}

fn handle_request(request: Request, media_items: &[ServedMediaItem], base_url: &str, token: &str) {
    if !matches!(request.method(), &Method::Get | &Method::Head) {
        respond_text(request, 405, "Only playback requests are supported.");
        return;
    }

    let parsed_url = match url::Url::parse(&format!("http://localhost{}", request.url())) {
        Ok(url) => url,
        Err(_) => {
            respond_text(request, 400, "Invalid phone access URL.");
            return;
        }
    };
    let provided_token = parsed_url
        .query_pairs()
        .find(|(key, _)| key == "token")
        .map(|(_, value)| value.to_string());

    if provided_token.as_deref() != Some(token) {
        respond_text(request, 403, "Phone access link is not valid.");
        return;
    }

    match parsed_url.path() {
        PLAYLIST_PATH => {
            let playlist = build_playlist(base_url, token, media_items.iter());
            let response = Response::from_string(playlist)
                .with_header(header("Content-Type", "audio/x-mpegurl; charset=utf-8"))
                .with_header(header("Cache-Control", "no-store"));
            let _ = request.respond(response);
        }
        path if path.starts_with(MEDIA_PREFIX) => {
            let media_file_id = &path[MEDIA_PREFIX.len()..];
            match media_items
                .iter()
                .find(|item| item.share.media_file_id == media_file_id)
            {
                Some(item) => respond_media(request, item),
                None => respond_text(request, 404, "Media file was not found."),
            }
        }
        _ => respond_text(request, 404, "Phone access item was not found."),
    }
}

fn respond_media(request: Request, item: &ServedMediaItem) {
    let total_length = match item.path.metadata() {
        Ok(metadata) => metadata.len(),
        Err(_) => {
            respond_text(request, 404, "Media file is missing.");
            return;
        }
    };
    let range_header = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Range"))
        .map(|header| header.value.as_str().to_string());

    match parse_byte_range(range_header.as_deref(), total_length) {
        Ok(Some(range)) => respond_media_range(request, item, total_length, range),
        Ok(None) => respond_media_full(request, item),
        Err(_) => {
            let response = Response::from_string("Requested range is not available.")
                .with_status_code(StatusCode(416))
                .with_header(header("Content-Range", &format!("bytes */{total_length}")))
                .with_header(header("Accept-Ranges", "bytes"));
            let _ = request.respond(response);
        }
    }
}

fn respond_media_full(request: Request, item: &ServedMediaItem) {
    match File::open(&item.path) {
        Ok(file) => {
            let response = Response::from_file(file)
                .with_header(header("Content-Type", &item.mime_type))
                .with_header(header("Accept-Ranges", "bytes"))
                .with_header(header("Cache-Control", "no-store"));
            let _ = request.respond(response);
        }
        Err(_) => respond_text(request, 404, "Media file is missing."),
    }
}

fn respond_media_range(
    request: Request,
    item: &ServedMediaItem,
    total_length: u64,
    range: ByteRange,
) {
    match File::open(&item.path) {
        Ok(mut file) => {
            if file.seek(SeekFrom::Start(range.start)).is_err() {
                respond_text(request, 416, "Requested range is not available.");
                return;
            }

            let length = range.end - range.start + 1;
            let Ok(response_length) = usize::try_from(length) else {
                respond_text(request, 416, "Requested range is too large.");
                return;
            };
            let reader = file.take(length);
            let response = Response::new(
                StatusCode(206),
                vec![
                    header("Content-Type", &item.mime_type),
                    header("Accept-Ranges", "bytes"),
                    header(
                        "Content-Range",
                        &format!("bytes {}-{}/{}", range.start, range.end, total_length),
                    ),
                    header("Cache-Control", "no-store"),
                ],
                reader,
                Some(response_length),
                None,
            );
            let _ = request.respond(response);
        }
        Err(_) => respond_text(request, 404, "Media file is missing."),
    }
}

fn respond_text(request: Request, status_code: u16, message: &str) {
    let response = Response::from_string(message.to_string())
        .with_status_code(StatusCode(status_code))
        .with_header(header("Content-Type", "text/plain; charset=utf-8"))
        .with_header(header("Cache-Control", "no-store"));
    let _ = request.respond(response);
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes())
        .expect("static phone access header should be valid ASCII")
}

fn eligible_media_items(
    app: &AppHandle,
    scope: Option<PhoneMediaScope>,
) -> Result<Vec<ServedMediaItem>, String> {
    let data_dir = db::app_data_dir(app)?;
    let connection = db::open_connection(app)?;
    let mut statement = connection
        .prepare(
            "SELECT m.id, m.lesson_id, l.title, l.content_type, m.size_bytes,
                    m.relative_path, l.duration_seconds, t.display_name, c.title,
                    l.source_id, l.collection_id
             FROM media_files m
             JOIN lessons l ON l.id = m.lesson_id
             LEFT JOIN teachers t ON t.id = l.teacher_id
             LEFT JOIN collections c ON c.id = l.collection_id
             WHERE l.media_file_id = m.id
               AND l.content_type IN ('video', 'audio')
               AND m.import_status = 'ready'
             ORDER BY l.published_at DESC, l.title ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let scope = scope.unwrap_or(PhoneMediaScope {
        source_id: None,
        collection_id: None,
    });
    let mut items = Vec::new();

    for row in rows {
        let (
            media_file_id,
            lesson_id,
            title,
            content_type,
            size_bytes,
            relative_path,
            duration_seconds,
            teacher_label,
            collection_title,
            source_id,
            collection_id,
        ) = row.map_err(|error| error.to_string())?;

        if scope
            .source_id
            .as_deref()
            .map(|expected| expected != source_id)
            .unwrap_or(false)
            || scope
                .collection_id
                .as_deref()
                .map(|expected| expected != collection_id)
                .unwrap_or(false)
        {
            continue;
        }

        let Some(path) = db::resolve_library_media_path(&data_dir, &relative_path) else {
            continue;
        };
        if !path.is_file() {
            continue;
        }

        items.push(ServedMediaItem {
            mime_type: mime_type_for_path(&path, &content_type).to_string(),
            path,
            share: PhoneMediaShareItem {
                media_file_id,
                lesson_id,
                title,
                content_type,
                size_bytes,
                duration_seconds,
                teacher_label,
                collection_title,
            },
        });
    }

    Ok(items)
}

fn build_playlist<'a>(
    base_url: &str,
    token: &str,
    items: impl Iterator<Item = &'a ServedMediaItem>,
) -> String {
    let mut playlist = String::from("#EXTM3U\n");

    for item in items {
        let duration = item.share.duration_seconds.unwrap_or(-1);
        playlist.push_str(&format!(
            "#EXTINF:{duration},{}\n",
            sanitize_playlist_title(&item.share.title)
        ));
        playlist.push_str(&format!(
            "{base_url}{MEDIA_PREFIX}{}?token={token}\n",
            item.share.media_file_id
        ));
    }

    playlist
}

fn sanitize_playlist_title(title: &str) -> String {
    title
        .chars()
        .map(|character| match character {
            '\r' | '\n' | ',' => ' ',
            _ => character,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_byte_range(value: Option<&str>, total_length: u64) -> Result<Option<ByteRange>, ()> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some(range_value) = value.trim().strip_prefix("bytes=") else {
        return Err(());
    };
    let Some((start, end)) = range_value.split_once('-') else {
        return Err(());
    };

    if total_length == 0 {
        return Err(());
    }

    if start.is_empty() {
        let suffix_length = end.parse::<u64>().map_err(|_| ())?;
        if suffix_length == 0 {
            return Err(());
        }
        let start = total_length.saturating_sub(suffix_length);
        return Ok(Some(ByteRange {
            start,
            end: total_length - 1,
        }));
    }

    let start = start.parse::<u64>().map_err(|_| ())?;
    if start >= total_length {
        return Err(());
    }

    let end = if end.is_empty() {
        total_length - 1
    } else {
        end.parse::<u64>().map_err(|_| ())?.min(total_length - 1)
    };

    if end < start {
        return Err(());
    }

    Ok(Some(ByteRange { start, end }))
}

fn mime_type_for_path(path: &Path, content_type: &str) -> &'static str {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match (content_type, extension.as_str()) {
        ("audio", "aac") => "audio/aac",
        ("audio", "flac") => "audio/flac",
        ("audio", "m4a") => "audio/mp4",
        ("audio", "mp3") => "audio/mpeg",
        ("audio", "ogg") => "audio/ogg",
        ("audio", "wav") => "audio/wav",
        ("video", "mkv") => "video/x-matroska",
        ("video", "mov") => "video/quicktime",
        ("video", "webm") => "video/webm",
        ("video", _) => "video/mp4",
        ("audio", _) => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

fn local_network_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let ip = socket.local_addr().ok()?.ip();

    if ip.is_loopback() {
        None
    } else {
        Some(ip)
    }
}

impl ActivePhoneSession {
    fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.server.unblock();

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_ended_byte_range() {
        assert_eq!(
            parse_byte_range(Some("bytes=10-"), 100),
            Ok(Some(ByteRange { start: 10, end: 99 }))
        );
    }

    #[test]
    fn parses_bounded_byte_range() {
        assert_eq!(
            parse_byte_range(Some("bytes=10-19"), 100),
            Ok(Some(ByteRange { start: 10, end: 19 }))
        );
    }

    #[test]
    fn parses_suffix_byte_range() {
        assert_eq!(
            parse_byte_range(Some("bytes=-10"), 100),
            Ok(Some(ByteRange { start: 90, end: 99 }))
        );
    }

    #[test]
    fn rejects_unsatisfied_byte_range() {
        assert_eq!(parse_byte_range(Some("bytes=100-120"), 100), Err(()));
    }

    #[test]
    fn playlist_titles_do_not_break_rows() {
        assert_eq!(
            sanitize_playlist_title("Lesson,\nPart 1"),
            "Lesson Part 1".to_string()
        );
    }

    #[test]
    fn chooses_media_mime_types() {
        assert_eq!(
            mime_type_for_path(Path::new("lesson.webm"), "video"),
            "video/webm"
        );
        assert_eq!(
            mime_type_for_path(Path::new("lesson.mp3"), "audio"),
            "audio/mpeg"
        );
    }
}
