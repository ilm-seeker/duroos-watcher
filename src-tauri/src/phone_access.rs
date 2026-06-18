use crate::{
    db,
    models::{PhoneMediaEndpoint, PhoneMediaScope, PhoneMediaSession, PhoneMediaShareItem},
};
use chrono::Utc;
use rusqlite::params;
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
    process::Command,
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
    let session_id = format!("phone-session-{}", Uuid::new_v4());
    let token = Uuid::new_v4().to_string();
    let endpoints = local_network_endpoints(port, &token);
    let preferred_endpoint = endpoints
        .iter()
        .find(|endpoint| endpoint.preferred)
        .or_else(|| endpoints.first())
        .ok_or_else(|| "Phone access could not create a playback endpoint.".to_string())?;
    let base_url = preferred_endpoint.base_url.clone();
    let playlist_url = preferred_endpoint.playlist_url.clone();
    let item_count = media.len() as i64;
    let items = media
        .iter()
        .map(|item| item.share.clone())
        .collect::<Vec<_>>();
    let messages = if preferred_endpoint.kind == "loopback" {
        vec![
            "Could not detect a same-Wi-Fi address. The link only works on this computer."
                .to_string(),
        ]
    } else if endpoints
        .iter()
        .any(|endpoint| endpoint.kind == "vpn" || endpoint.kind == "tor")
    {
        vec![
            "Keep this app open while watching on your phone.".to_string(),
            "VPN or privacy-network addresses were detected; use the Wi-Fi/LAN endpoint for phone scans."
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
        endpoints,
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
        ("audio", "aif") | ("audio", "aiff") => "audio/aiff",
        ("audio", "amr") => "audio/amr",
        ("audio", "flac") => "audio/flac",
        ("audio", "m4a") => "audio/mp4",
        ("audio", "mp3") => "audio/mpeg",
        ("audio", "ogg") => "audio/ogg",
        ("audio", "opus") => "audio/opus",
        ("audio", "wma") => "audio/x-ms-wma",
        ("audio", "wav") => "audio/wav",
        ("video", "3g2") => "video/3gpp2",
        ("video", "3gp") => "video/3gpp",
        ("video", "avi") => "video/x-msvideo",
        ("video", "flv") => "video/x-flv",
        ("video", "mkv") => "video/x-matroska",
        ("video", "mov") => "video/quicktime",
        ("video", "m2ts") | ("video", "mts") | ("video", "ts") => "video/mp2t",
        ("video", "mpg") | ("video", "mpeg") => "video/mpeg",
        ("video", "vob") => "video/dvd",
        ("video", "webm") => "video/webm",
        ("video", "wmv") => "video/x-ms-wmv",
        ("video", _) => "video/mp4",
        ("audio", _) => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

#[derive(Debug, Clone)]
struct EndpointCandidate {
    host: IpAddr,
    interface_name: Option<String>,
    kind: String,
    score: i32,
    warning: Option<String>,
}

fn local_network_endpoints(port: u16, token: &str) -> Vec<PhoneMediaEndpoint> {
    let mut candidates = interface_ip_candidates();
    candidates.push(endpoint_candidate(None, Ipv4Addr::LOCALHOST));
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.host.to_string().cmp(&right.host.to_string()))
    });

    let mut seen = HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate.host));
    let preferred_index = candidates
        .iter()
        .position(|candidate| candidate.kind == "lan")
        .or_else(|| {
            candidates
                .iter()
                .position(|candidate| candidate.kind != "loopback")
        })
        .unwrap_or(0);

    candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| {
            let host = candidate.host.to_string();
            let base_url = format!("http://{host}:{port}");
            let playlist_url = format!("{base_url}{PLAYLIST_PATH}?token={token}");
            PhoneMediaEndpoint {
                label: endpoint_label(&candidate),
                host,
                kind: candidate.kind,
                base_url,
                playlist_url,
                preferred: index == preferred_index,
                warning: candidate.warning,
            }
        })
        .collect()
}

fn interface_ip_candidates() -> Vec<EndpointCandidate> {
    let mut candidates = if cfg!(target_os = "windows") {
        windows_ip_candidates()
    } else {
        ifconfig_ip_candidates()
    };
    candidates.retain(|candidate| !candidate.host.is_unspecified());
    candidates
}

fn ifconfig_ip_candidates() -> Vec<EndpointCandidate> {
    let Ok(output) = Command::new("ifconfig").arg("-a").output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut current_interface: Option<String> = None;
    let mut candidates = Vec::new();
    for line in text.lines() {
        if line
            .chars()
            .next()
            .map(|character| !character.is_whitespace())
            .unwrap_or(false)
        {
            current_interface = line
                .split(':')
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }

        let tokens = line.split_whitespace().collect::<Vec<_>>();
        for (index, token) in tokens.iter().enumerate() {
            if *token != "inet" {
                continue;
            }
            if let Some(ip) = tokens
                .get(index + 1)
                .and_then(|value| value.parse::<Ipv4Addr>().ok())
            {
                candidates.push(endpoint_candidate(current_interface.clone(), ip));
            }
        }
    }

    candidates
}

fn windows_ip_candidates() -> Vec<EndpointCandidate> {
    let Ok(output) = Command::new("ipconfig").output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut current_interface: Option<String> = None;
    let mut candidates = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with(':') {
            current_interface = Some(trimmed.trim_end_matches(':').to_string());
            continue;
        }
        if !trimmed.contains("IPv4") {
            continue;
        }
        if let Some(value) = trimmed.split(':').nth(1) {
            if let Some(ip) = value
                .split_whitespace()
                .next()
                .and_then(|candidate| candidate.parse::<Ipv4Addr>().ok())
            {
                candidates.push(endpoint_candidate(current_interface.clone(), ip));
            }
        }
    }

    candidates
}

fn endpoint_candidate(interface_name: Option<String>, ip: Ipv4Addr) -> EndpointCandidate {
    let interface = interface_name
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut score = 0;
    let mut kind = "other".to_string();
    let mut warning = None;

    if ip.is_loopback() {
        kind = "loopback".to_string();
        score -= 200;
        warning = Some("Only works on this computer.".to_string());
    } else if looks_like_privacy_interface(&interface) {
        kind = if interface.contains("tor") {
            "tor".to_string()
        } else {
            "vpn".to_string()
        };
        score -= 80;
        warning = Some(
            "This looks like a VPN or privacy-network address; phones on Wi-Fi may not reach it."
                .to_string(),
        );
    } else if ip.is_private() {
        kind = "lan".to_string();
        score += 100;
    }

    if interface.starts_with("en")
        || interface.starts_with("eth")
        || interface.starts_with("wlan")
        || interface.contains("wi-fi")
        || interface.contains("wifi")
    {
        score += 80;
        if kind == "other" {
            kind = "lan".to_string();
        }
    }
    if interface.contains("bridge")
        || interface.contains("docker")
        || interface.contains("vmnet")
        || interface.contains("vbox")
        || interface.starts_with("awdl")
        || interface.starts_with("llw")
    {
        score -= 60;
        if kind == "lan" {
            kind = "other".to_string();
            warning =
                Some("This network interface may not be reachable from your phone.".to_string());
        }
    }
    let octets = ip.octets();
    if octets[0] == 192 && octets[1] == 168 {
        score += 30;
    }

    EndpointCandidate {
        host: IpAddr::V4(ip),
        interface_name,
        kind,
        score,
        warning,
    }
}

fn endpoint_label(candidate: &EndpointCandidate) -> String {
    let host = candidate.host.to_string();
    let interface = candidate
        .interface_name
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    match candidate.kind.as_str() {
        "lan" => interface
            .map(|name| format!("Wi-Fi/LAN ({name}) {host}"))
            .unwrap_or_else(|| format!("Wi-Fi/LAN {host}")),
        "vpn" => interface
            .map(|name| format!("VPN/tunnel ({name}) {host}"))
            .unwrap_or_else(|| format!("VPN/tunnel {host}")),
        "tor" => interface
            .map(|name| format!("Privacy network ({name}) {host}"))
            .unwrap_or_else(|| format!("Privacy network {host}")),
        "loopback" => format!("This computer only {host}"),
        _ => interface
            .map(|name| format!("Other network ({name}) {host}"))
            .unwrap_or_else(|| format!("Other network {host}")),
    }
}

fn looks_like_privacy_interface(interface: &str) -> bool {
    [
        "utun",
        "tun",
        "tap",
        "wg",
        "tailscale",
        "vpn",
        "ppp",
        "ipsec",
        "zt",
        "zerotier",
        "tor",
    ]
    .iter()
    .any(|marker| interface.contains(marker))
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
        assert_eq!(
            mime_type_for_path(Path::new("lesson.avi"), "video"),
            "video/x-msvideo"
        );
        assert_eq!(
            mime_type_for_path(Path::new("lesson.wma"), "audio"),
            "audio/x-ms-wma"
        );
    }

    #[test]
    fn endpoint_scoring_prefers_lan_over_vpn_tunnel() {
        let lan = endpoint_candidate(Some("en0".to_string()), Ipv4Addr::new(192, 168, 0, 191));
        let vpn = endpoint_candidate(Some("utun4".to_string()), Ipv4Addr::new(10, 2, 0, 2));
        let loopback = endpoint_candidate(None, Ipv4Addr::LOCALHOST);

        assert_eq!(lan.kind, "lan");
        assert_eq!(vpn.kind, "vpn");
        assert_eq!(loopback.kind, "loopback");
        assert!(lan.score > vpn.score);
        assert!(vpn.score > loopback.score);
        assert!(vpn.warning.is_some());
    }
}
