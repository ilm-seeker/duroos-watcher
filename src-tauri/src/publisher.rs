use crate::{
    db, manifest,
    models::{
        ArchiveMirrorConfig, ArchiveMirrorResult, BlossomServerConfig, BlossomUploadResult,
        ChannelPublishResult, CreatePublisherProfileRequest, IngestSummary, NostrChannelPreview,
        NostrRelayConfig, NostrRelayPublishResult, PublishTeacherChannelRequest,
        PublishedLessonDraft, PublishedPostDraft, PublisherChannel, PublisherEndpointTestReport,
        PublisherEndpointTestRequest, PublisherProfile,
    },
};
use argon2::Argon2;
use base64::{engine::general_purpose, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use chrono::Utc;
use ed25519_dalek::{Signer as Ed25519Signer, SigningKey};
use rand::{rngs::OsRng, Rng};
use reqwest::blocking::{multipart, Client};
use rusqlite::{params, Connection, OptionalExtension};
use secp256k1::{Keypair, Message as SecpMessage, Secp256k1, SecretKey, XOnlyPublicKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::AppHandle;
use tungstenite::{connect, Message as WsMessage};
use url::Url;
use uuid::Uuid;
use zeroize::Zeroize;

const DUROOS_CHANNEL_KIND: u64 = 30078;
const BLOSSOM_AUTH_KIND: u64 = 24242;
const APP_TAG: &str = "duroos-watcher";
const VAULT_VERSION: u8 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VaultFile {
    version: u8,
    kdf: String,
    cipher: String,
    salt: String,
    nonce: String,
    ciphertext: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VaultPlaintext {
    curator_secret_key: String,
    nostr_secret_key: String,
}

#[derive(Debug, Clone)]
struct PublisherKeys {
    curator_secret_key: [u8; 32],
    nostr_secret_key: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct ResolvedNostrChannel {
    pub naddr: String,
    pub manifest_url: String,
    pub manifest_urls: Vec<String>,
    pub archive_mirrors: Vec<String>,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone)]
struct ParsedNaddr {
    raw: String,
    identifier: String,
    author: String,
    kind: u64,
    relays: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NostrEvent {
    id: String,
    pubkey: String,
    created_at: i64,
    kind: u64,
    tags: Vec<Vec<String>>,
    content: String,
    sig: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChannelEventContent {
    manifest_url: Option<String>,
    manifest_urls: Option<Vec<String>>,
    archive_mirrors: Option<Vec<ArchiveMirrorPointer>>,
    manifest_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveMirrorPointer {
    service: String,
    url: String,
    cid: Option<String>,
    public: bool,
    permanent: bool,
}

#[derive(Debug)]
struct PublishedBlob {
    title: String,
    content_type: String,
    description: Option<String>,
    url: String,
    sha256: String,
    size_bytes: i64,
    mime_type: String,
}

#[derive(Debug, Clone)]
struct PublishedChannelItem {
    id: String,
    channel_id: String,
    item_type: String,
    title: String,
    content_type: String,
    description: Option<String>,
    origin_url: String,
    retrieval_url: Option<String>,
    sha256: String,
    size_bytes: Option<i64>,
    mime_type: Option<String>,
    published_at: String,
}

struct PublisherChannelUpsert<'a> {
    id: &'a str,
    profile_id: &'a str,
    title: &'a str,
    description: Option<&'a str>,
    channel_identifier: &'a str,
    naddr: &'a str,
    canonical_channel_link: &'a str,
    last_manifest_sha256: &'a str,
    last_manifest_url: &'a str,
    last_published_at: &'a str,
    media_count: i64,
    post_count: i64,
}

#[derive(Debug)]
struct SignedManifestInput<'a> {
    profile: &'a PublisherProfile,
    keys: &'a PublisherKeys,
    naddr: &'a str,
    channel_title: &'a str,
    channel_description: Option<&'a str>,
    relays: &'a [NostrRelayConfig],
    blossom_servers: &'a [BlossomServerConfig],
    items: &'a [PublishedChannelItem],
    published_at: &'a str,
}

pub fn list_publisher_profiles(app: &AppHandle) -> Result<Vec<PublisherProfile>, String> {
    let connection = db::open_connection(app)?;
    fetch_publisher_profiles(&connection, app)
}

pub fn list_publisher_channels(app: &AppHandle) -> Result<Vec<PublisherChannel>, String> {
    let connection = db::open_connection(app)?;
    fetch_publisher_channels(&connection)
}

pub fn create_publisher_profile(
    app: &AppHandle,
    request: CreatePublisherProfileRequest,
) -> Result<PublisherProfile, String> {
    let display_name = trimmed_required(&request.display_name, "Teacher display name")?;
    validate_passphrase(&request.passphrase)?;
    let relays = normalize_relays(request.relays)?;
    let blossom_servers = normalize_blossom_servers(request.blossom_servers)?;

    let mut curator_secret = random_32_bytes();
    let mut nostr_secret = random_32_bytes();

    let curator_signing_key = SigningKey::from_bytes(&curator_secret);
    let curator_public_key = general_purpose::STANDARD.encode(curator_signing_key.verifying_key());
    let nostr_pubkey = nostr_pubkey_from_secret(&nostr_secret)?;
    let profile_id = format!(
        "publisher-{}",
        stable_suffix(&format!("{display_name}:{nostr_pubkey}"))
    );
    let now = Utc::now().to_rfc3339();
    let vault_path = publisher_vault_path(app, &profile_id)?;
    let plaintext = VaultPlaintext {
        curator_secret_key: general_purpose::STANDARD.encode(curator_secret),
        nostr_secret_key: hex_lower(&nostr_secret),
    };
    let vault = encrypt_vault(&request.passphrase, &plaintext)?;
    let vault_json = serde_json::to_string_pretty(&vault).map_err(|error| error.to_string())?;

    if let Some(parent) = vault_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&vault_path, vault_json).map_err(|error| error.to_string())?;

    let connection = db::open_connection(app)?;
    connection
        .execute(
            "INSERT INTO publisher_profiles
             (id, display_name, curator_public_key, nostr_pubkey, relays_json,
              blossom_servers_json, vault_path, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &profile_id,
                &display_name,
                &curator_public_key,
                &nostr_pubkey,
                serde_json::to_string(&relays).map_err(|error| error.to_string())?,
                serde_json::to_string(&blossom_servers).map_err(|error| error.to_string())?,
                vault_path.to_string_lossy(),
                &now,
                &now
            ],
        )
        .map_err(|error| error.to_string())?;

    curator_secret.zeroize();
    nostr_secret.zeroize();

    publisher_profile_for_id(&connection, app, &profile_id)?
        .ok_or_else(|| "Publisher profile could not be saved.".to_string())
}

pub fn unlock_publisher_profile(
    app: &AppHandle,
    profile_id: String,
    passphrase: String,
) -> Result<PublisherProfile, String> {
    validate_passphrase(&passphrase)?;
    let connection = db::open_connection(app)?;
    let profile = publisher_profile_for_id(&connection, app, profile_id.trim())?
        .ok_or_else(|| "Publisher profile was not found.".to_string())?;
    let _keys = unlock_profile_keys(app, &profile, &passphrase)?;
    Ok(profile)
}

pub fn publish_teacher_channel(
    app: &AppHandle,
    request: PublishTeacherChannelRequest,
) -> Result<ChannelPublishResult, String> {
    let channel_title = trimmed_required(&request.channel_title, "Channel title")?;
    validate_passphrase(&request.passphrase)?;
    let relays = normalize_relays(request.relays)?;
    let blossom_servers = normalize_blossom_servers(request.blossom_servers)?;
    let archive_mirrors = normalize_archive_mirrors(request.archive_mirrors)?;
    let post_drafts = normalize_post_drafts(request.posts)?;
    if request.lessons.is_empty() && post_drafts.is_empty() {
        return Err(
            "Add at least one video, audio, PDF, or text post before publishing.".to_string(),
        );
    }

    let connection = db::open_connection(app)?;
    let profile = publisher_profile_for_id(&connection, app, request.profile_id.trim())?
        .ok_or_else(|| "Publisher profile was not found.".to_string())?;
    let keys = unlock_profile_keys(app, &profile, &request.passphrase)?;
    let (channel_id, identifier) = resolve_publish_channel_identity(
        &connection,
        &profile,
        request.channel_id.as_deref(),
        &channel_title,
    )?;
    let client = Client::builder()
        .user_agent("DuroosWatcher/0.1 federated-publisher")
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())?;
    let naddr = encode_naddr(
        &identifier,
        &profile.nostr_pubkey,
        DUROOS_CHANNEL_KIND as u32,
        &relays
            .iter()
            .map(|relay| relay.url.clone())
            .collect::<Vec<_>>(),
    )?;
    let published_at = Utc::now().to_rfc3339();
    let mut blossom_results = Vec::new();
    let mut new_items = Vec::new();

    for draft in &request.lessons {
        let blob = publish_lesson_blob(
            &client,
            draft,
            &blossom_servers,
            &keys,
            &mut blossom_results,
        )?;
        new_items.push(channel_item_from_blob(&channel_id, blob, &published_at));
    }

    for post in &post_drafts {
        new_items.push(channel_item_from_post(&channel_id, post, &published_at));
    }

    let mut channel_items = fetch_published_channel_items(&connection, &channel_id)?;
    for item in &new_items {
        if !channel_items.iter().any(|existing| existing.id == item.id) {
            channel_items.push(item.clone());
        }
    }
    channel_items.sort_by(|left, right| {
        left.published_at
            .cmp(&right.published_at)
            .then_with(|| left.title.cmp(&right.title))
    });
    let media_count = channel_items
        .iter()
        .filter(|item| item.item_type == "media")
        .count() as i64;
    let post_count = channel_items
        .iter()
        .filter(|item| item.item_type == "post")
        .count() as i64;

    let (manifest_json, manifest_payload_sha256) = signed_channel_manifest(SignedManifestInput {
        profile: &profile,
        keys: &keys,
        naddr: &naddr,
        channel_title: &channel_title,
        channel_description: request.channel_description.as_deref(),
        relays: &relays,
        blossom_servers: &blossom_servers,
        items: &channel_items,
        published_at: &published_at,
    })?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    let manifest_upload = upload_blob_to_servers(
        &client,
        manifest_json.as_bytes(),
        "application/json",
        "json",
        &blossom_servers,
        &keys,
    );
    blossom_results.extend(manifest_upload.results);

    let manifest_url = manifest_upload.first_url.ok_or_else(|| {
        "Manifest upload failed on all Blossom servers; Nostr event was not published.".to_string()
    })?;
    let mut manifest_urls = manifest_upload.urls;
    let archive_results = publish_archive_mirrors(
        &client,
        manifest_json.as_bytes(),
        &manifest_sha256,
        &archive_mirrors,
    );
    let archive_refs = archive_results
        .iter()
        .filter(|result| result.verified)
        .filter_map(|result| {
            result.url.as_ref().map(|url| ArchiveMirrorPointer {
                service: result.service.clone(),
                url: url.clone(),
                cid: result.cid.clone(),
                public: true,
                permanent: matches!(
                    result.service.as_str(),
                    "ipfs-http-api" | "ipfs-gateway" | "arweave" | "filecoin"
                ),
            })
        })
        .collect::<Vec<_>>();
    for archive_ref in &archive_refs {
        if !manifest_urls
            .iter()
            .any(|existing| existing == &archive_ref.url)
        {
            manifest_urls.push(archive_ref.url.clone());
        }
    }
    let archive_ref_count = archive_refs.len();
    let archive_failure_count = archive_results
        .iter()
        .filter(|result| !result.verified)
        .count();
    let formatted_manifest_sha256 = format!("sha256:{manifest_sha256}");
    let canonical_channel_link = canonical_channel_link(&naddr);
    let verification_code = manifest_verification_code(&manifest_sha256);
    let invite_text = channel_invite_text(
        &channel_title,
        &profile.display_name,
        &canonical_channel_link,
        &formatted_manifest_sha256,
        &verification_code,
    );
    let event_content = json!({
        "app": APP_TAG,
        "schemaVersion": 1,
        "channelId": channel_id,
        "title": channel_title,
        "manifestUrl": manifest_url,
        "manifestUrls": manifest_urls.clone(),
        "manifestSha256": formatted_manifest_sha256.clone(),
        "manifestPayloadSha256": format!("sha256:{manifest_payload_sha256}"),
        "archiveMirrors": archive_refs.clone(),
        "curatorPublicKey": profile.curator_public_key,
        "publishedAt": published_at,
    })
    .to_string();
    let mut tags = vec![
        vec!["d".to_string(), identifier.clone()],
        vec!["client".to_string(), APP_TAG.to_string()],
        vec!["x".to_string(), manifest_sha256.clone()],
        vec![
            "alt".to_string(),
            format!("Duroos channel update: {channel_title}"),
        ],
    ];
    tags.extend(
        manifest_urls
            .iter()
            .map(|url| vec!["r".to_string(), url.clone()]),
    );
    let event = signed_nostr_event(
        &keys.nostr_secret_key,
        DUROOS_CHANNEL_KIND,
        tags,
        event_content,
    )?;
    let relay_results = publish_event_to_relays(&event, &relays);
    if !relay_results.iter().any(|result| result.accepted) {
        return Err(
            "Nostr event was rejected or unreachable on every configured relay.".to_string(),
        );
    }

    upsert_publisher_channel(
        &connection,
        &PublisherChannelUpsert {
            id: &channel_id,
            profile_id: &profile.id,
            title: &channel_title,
            description: request.channel_description.as_deref(),
            channel_identifier: &identifier,
            naddr: &naddr,
            canonical_channel_link: &canonical_channel_link,
            last_manifest_sha256: &formatted_manifest_sha256,
            last_manifest_url: &manifest_url,
            last_published_at: &published_at,
            media_count,
            post_count,
        },
    )?;
    upsert_published_channel_items(&connection, &new_items)?;

    connection
        .execute(
            "UPDATE publisher_profiles
             SET relays_json = ?1, blossom_servers_json = ?2, updated_at = ?3
             WHERE id = ?4",
            params![
                serde_json::to_string(&relays).map_err(|error| error.to_string())?,
                serde_json::to_string(&blossom_servers).map_err(|error| error.to_string())?,
                Utc::now().to_rfc3339(),
                profile.id
            ],
        )
        .map_err(|error| error.to_string())?;

    Ok(ChannelPublishResult {
        channel_id: channel_id.clone(),
        channel_title,
        naddr,
        canonical_channel_link,
        invite_text,
        verification_code,
        manifest_json,
        manifest_sha256: formatted_manifest_sha256,
        manifest_url,
        nostr_event_id: event.id,
        blossom_results,
        archive_results,
        relay_results,
        media_count,
        post_count,
        total_item_count: media_count + post_count,
        messages: vec![
            format!(
                "Published {media_count} media item(s) and {post_count} text post(s) in this channel."
            ),
            format!(
                "Channel feed now advertises {total} signed item(s).",
                total = media_count + post_count
            ),
            format!(
                "Advertised {archive_ref_count} archive manifest mirror(s) after SHA-256 verification; {archive_failure_count} archive mirror(s) were skipped."
            ),
            "Existing subscribers keep the same channel link; no central Duroos catalog was created."
                .to_string(),
        ],
    })
}

pub fn test_publisher_endpoints(
    app: &AppHandle,
    request: PublisherEndpointTestRequest,
) -> Result<PublisherEndpointTestReport, String> {
    validate_passphrase(&request.passphrase)?;
    let relays = normalize_relays(request.relays)?;
    let blossom_servers = normalize_blossom_servers(request.blossom_servers)?;
    let connection = db::open_connection(app)?;
    let profile = publisher_profile_for_id(&connection, app, request.profile_id.trim())?
        .ok_or_else(|| "Publisher profile was not found.".to_string())?;
    let keys = unlock_profile_keys(app, &profile, &request.passphrase)?;
    let client = Client::builder()
        .user_agent("DuroosWatcher/0.1 publisher-endpoint-test")
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| error.to_string())?;
    let probe_body = format!(
        "Duroos Watcher publisher endpoint test\nprofile={}\ntime={}\n",
        profile.id,
        Utc::now().to_rfc3339()
    );
    let blossom_upload = upload_blob_to_servers(
        &client,
        probe_body.as_bytes(),
        "text/plain; charset=utf-8",
        "txt",
        &blossom_servers,
        &keys,
    );
    let relay_event = endpoint_probe_event(&keys, &profile, &blossom_upload.urls)?;
    let relay_results = publish_event_to_relays(&relay_event, &relays);
    let storage_ok = blossom_upload.results.iter().any(|result| result.uploaded);
    let relay_ok = relay_results.iter().any(|result| result.accepted);
    let passed = storage_ok && relay_ok;
    let messages = endpoint_test_messages(passed, &blossom_upload.results, &relay_results);

    Ok(PublisherEndpointTestReport {
        passed,
        blossom_results: blossom_upload.results,
        relay_results,
        messages,
    })
}

fn endpoint_test_messages(
    passed: bool,
    blossom_results: &[BlossomUploadResult],
    relay_results: &[NostrRelayPublishResult],
) -> Vec<String> {
    let uploaded_count = blossom_results
        .iter()
        .filter(|result| result.uploaded)
        .count();
    let accepted_count = relay_results
        .iter()
        .filter(|result| result.accepted)
        .count();
    let has_endpoint_failures =
        uploaded_count < blossom_results.len() || accepted_count < relay_results.len();
    let result_label = if passed && has_endpoint_failures {
        "Endpoint quorum passed with failures"
    } else if passed {
        "Endpoint test passed"
    } else {
        "Endpoint test completed with issues"
    };

    let mut messages = vec![format!(
        "{result_label}: {uploaded_count}/{storage_total} Blossom server(s) uploaded the probe; {accepted_count}/{relay_total} relay(s) accepted the test event.",
        storage_total = blossom_results.len(),
        relay_total = relay_results.len(),
    )];

    if passed && has_endpoint_failures {
        messages.push(
            "Publishing can continue through endpoints that accepted the probe, but failed endpoints should be fixed or removed before relying on them."
                .to_string(),
        );
    } else if !passed {
        messages.push(
            "A real publish still needs at least one working Blossom server and one accepting Nostr relay."
                .to_string(),
        );
    }

    messages.push(
        "The probe is intentionally small and public on any endpoint that accepted it.".to_string(),
    );
    messages
}

pub fn ingest_nostr_channel(app: &AppHandle, channel_ref: String) -> Result<IngestSummary, String> {
    db::ingest_source_url(app, channel_ref)
}

pub fn preview_nostr_channel(
    app: &AppHandle,
    channel_ref: String,
) -> Result<NostrChannelPreview, String> {
    let resolved = resolve_nostr_channel_manifest_url(&channel_ref)?;
    let client = Client::builder()
        .user_agent("DuroosWatcher/0.1 channel-preview")
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(&resolved.manifest_url)
        .send()
        .map_err(|error| format!("Could not fetch channel manifest: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Could not fetch channel manifest: HTTP {status}."));
    }
    let manifest_json = response
        .text()
        .map_err(|error| format!("Could not read channel manifest: {error}"))?;
    let actual_hash = sha256_hex(manifest_json.as_bytes());
    let expected_hash = resolved
        .manifest_sha256
        .strip_prefix("sha256:")
        .unwrap_or(&resolved.manifest_sha256)
        .to_ascii_lowercase();
    if expected_hash != actual_hash {
        return Err(format!(
            "Channel manifest hash mismatch. Expected sha256:{expected_hash}, got sha256:{actual_hash}."
        ));
    }

    let connection = db::open_connection(app)?;
    let report = db::validate_collection_manifest(&connection, &manifest_json)?;
    if !report.valid {
        return Err(format!(
            "Channel manifest did not validate: {}",
            report.errors.join("; ")
        ));
    }
    let value: Value = serde_json::from_str(&manifest_json)
        .map_err(|error| format!("Channel manifest JSON was invalid: {error}"))?;
    Ok(channel_preview_from_manifest(resolved, &value, &report))
}

pub fn resolve_nostr_channel_manifest_url(
    channel_ref: &str,
) -> Result<ResolvedNostrChannel, String> {
    let parsed = decode_naddr(channel_ref)?;
    if parsed.kind != DUROOS_CHANNEL_KIND {
        return Err("Nostr address is not a Duroos channel event.".to_string());
    }
    if parsed.relays.is_empty() {
        return Err("Nostr channel link does not include relay hints.".to_string());
    }

    let mut last_error = String::new();
    for relay in &parsed.relays {
        match fetch_channel_event(relay, &parsed) {
            Ok(event) => {
                let content: ChannelEventContent =
                    serde_json::from_str(&event.content).map_err(|error| {
                        format!("Nostr event content was not a Duroos channel pointer: {error}")
                    })?;
                if !looks_like_sha256(&content.manifest_sha256) {
                    return Err("Nostr channel manifest hash is not a sha256 hash.".to_string());
                }
                let archive_mirrors = archive_mirror_urls_from_event_content(&content);
                let manifest_urls = match manifest_urls_from_event_content(&content) {
                    Ok(urls) => urls,
                    Err(error) => {
                        last_error = format!("{relay}: {error}");
                        continue;
                    }
                };
                let manifest_url =
                    match select_verified_manifest_url(&manifest_urls, &content.manifest_sha256) {
                        Ok(url) => url,
                        Err(error) => {
                            last_error = format!("{relay}: {error}");
                            continue;
                        }
                    };
                return Ok(ResolvedNostrChannel {
                    naddr: parsed.raw,
                    manifest_url,
                    manifest_urls,
                    archive_mirrors,
                    manifest_sha256: content.manifest_sha256,
                });
            }
            Err(error) => {
                last_error = format!("{relay}: {error}");
            }
        }
    }

    Err(if last_error.is_empty() {
        "No relays were available for this channel.".to_string()
    } else {
        last_error
    })
}

fn channel_preview_from_manifest(
    resolved: ResolvedNostrChannel,
    manifest_value: &Value,
    report: &crate::models::ManifestValidationReport,
) -> NostrChannelPreview {
    let collection = manifest_value.get("collection").and_then(Value::as_object);
    let publication = manifest_value.get("publication").and_then(Value::as_object);
    let lessons = manifest_value
        .get("lessons")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let relays = publication
        .and_then(|publication| publication.get("relays"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let blossom_servers = publication
        .and_then(|publication| publication.get("blossomServers"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let media_count = lessons
        .iter()
        .filter(|lesson| {
            lesson
                .get("retrievalRefs")
                .and_then(Value::as_array)
                .map(|refs| {
                    refs.iter().any(|reference| {
                        matches!(
                            reference.get("kind").and_then(Value::as_str),
                            Some("direct-url" | "enclosure-url")
                        )
                    })
                })
                .unwrap_or(false)
        })
        .count() as i64;
    let advertised_manifest_count = resolved.manifest_urls.len();
    let archive_mirrors = resolved.archive_mirrors;
    let archive_mirror_count = archive_mirrors.len();

    NostrChannelPreview {
        naddr: resolved.naddr,
        manifest_url: resolved.manifest_url,
        manifest_sha256: resolved.manifest_sha256,
        title: collection
            .and_then(|collection| collection.get("title"))
            .and_then(Value::as_str)
            .unwrap_or("Duroos channel")
            .to_string(),
        curator_display_name: report
            .curator
            .as_ref()
            .map(|curator| curator.display_name.clone())
            .or_else(|| {
                manifest_value
                    .get("curator")
                    .and_then(|curator| curator.get("displayName"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Unknown curator".to_string()),
        curator_public_key: report
            .curator
            .as_ref()
            .map(|curator| curator.public_key.clone()),
        trust_state: report
            .trust_state
            .clone()
            .unwrap_or_else(|| "unsigned".to_string()),
        published_at: publication
            .and_then(|publication| publication.get("publishedAt"))
            .or_else(|| manifest_value.get("exportedAt"))
            .and_then(Value::as_str)
            .map(str::to_string),
        lesson_count: lessons.len() as i64,
        media_count,
        relay_count: relays.len() as i64,
        blossom_server_count: blossom_servers.len() as i64,
        archive_mirror_count: archive_mirror_count as i64,
        relays,
        blossom_servers,
        archive_mirrors,
        warnings: report.warnings.clone(),
        messages: vec![
            format!(
                "Preview verified the Nostr event pointer, signed manifest hash, {} advertised manifest mirror(s), and {} archive fallback(s).",
                advertised_manifest_count,
                archive_mirror_count
            ),
            "Import remains local-first; media files are only downloaded when requested."
                .to_string(),
        ],
    }
}

fn manifest_urls_from_event_content(content: &ChannelEventContent) -> Result<Vec<String>, String> {
    let mut urls = Vec::new();

    if let Some(url) = content.manifest_url.as_ref() {
        urls.push(url.clone());
    }
    if let Some(manifest_urls) = content.manifest_urls.as_ref() {
        urls.extend(manifest_urls.iter().cloned());
    }
    urls.extend(archive_mirror_urls_from_event_content(content));

    let mut output = Vec::new();
    for url in urls {
        let url = url.trim().to_string();
        if url.is_empty() || output.iter().any(|existing| existing == &url) {
            continue;
        }
        if !is_safe_http_url(&url) {
            return Err("Nostr channel manifest URLs must be http or https.".to_string());
        }
        output.push(url);
    }

    if output.is_empty() {
        Err("Nostr event did not include a manifest URL.".to_string())
    } else {
        Ok(output)
    }
}

fn archive_mirror_urls_from_event_content(content: &ChannelEventContent) -> Vec<String> {
    let Some(archive_mirrors) = content.archive_mirrors.as_ref() else {
        return Vec::new();
    };

    let mut output = Vec::new();
    for archive_mirror in archive_mirrors {
        let url = archive_mirror.url.trim().to_string();
        if url.is_empty()
            || !is_safe_http_url(&url)
            || output.iter().any(|existing| existing == &url)
        {
            continue;
        }
        output.push(url);
    }
    output
}

fn select_verified_manifest_url(urls: &[String], expected_sha256: &str) -> Result<String, String> {
    let client = Client::builder()
        .user_agent("DuroosWatcher/0.1 channel-resolver")
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|error| error.to_string())?;
    let expected = expected_sha256
        .strip_prefix("sha256:")
        .unwrap_or(expected_sha256)
        .to_ascii_lowercase();
    let mut last_error = String::new();

    for url in urls {
        match client.get(url).send() {
            Ok(response) if response.status().is_success() => match response.text() {
                Ok(body) => {
                    let actual = sha256_hex(body.as_bytes());
                    if actual == expected {
                        return Ok(url.clone());
                    }
                    last_error = format!(
                        "{url}: hash mismatch, expected sha256:{expected}, got sha256:{actual}"
                    );
                }
                Err(error) => {
                    last_error = format!("{url}: could not read manifest: {error}");
                }
            },
            Ok(response) => {
                last_error = format!("{url}: HTTP {}", response.status());
            }
            Err(error) => {
                last_error = format!("{url}: {error}");
            }
        }
    }

    Err(if last_error.is_empty() {
        "No manifest URLs were available.".to_string()
    } else {
        format!("No advertised manifest URL could be fetched and hash-verified. {last_error}")
    })
}

fn fetch_publisher_profiles(
    connection: &Connection,
    app: &AppHandle,
) -> Result<Vec<PublisherProfile>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, display_name, curator_public_key, nostr_pubkey, relays_json,
                    blossom_servers_json, vault_path, created_at, updated_at
             FROM publisher_profiles
             ORDER BY updated_at DESC, created_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| publisher_profile_from_row(row, app))
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn fetch_publisher_channels(connection: &Connection) -> Result<Vec<PublisherChannel>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, profile_id, title, description, channel_identifier, naddr,
                    canonical_channel_link, last_manifest_sha256, last_manifest_url,
                    last_published_at, media_count, post_count, created_at, updated_at
             FROM publisher_channels
             ORDER BY updated_at DESC, created_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], publisher_channel_from_row)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn publisher_channel_for_id(
    connection: &Connection,
    channel_id: &str,
) -> Result<Option<PublisherChannel>, String> {
    connection
        .query_row(
            "SELECT id, profile_id, title, description, channel_identifier, naddr,
                    canonical_channel_link, last_manifest_sha256, last_manifest_url,
                    last_published_at, media_count, post_count, created_at, updated_at
             FROM publisher_channels
             WHERE id = ?1",
            params![channel_id],
            publisher_channel_from_row,
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn publisher_channel_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PublisherChannel> {
    Ok(PublisherChannel {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        channel_identifier: row.get(4)?,
        naddr: row.get(5)?,
        canonical_channel_link: row.get(6)?,
        last_manifest_sha256: row.get(7)?,
        last_manifest_url: row.get(8)?,
        last_published_at: row.get(9)?,
        media_count: row.get(10)?,
        post_count: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn resolve_publish_channel_identity(
    connection: &Connection,
    profile: &PublisherProfile,
    requested_channel_id: Option<&str>,
    channel_title: &str,
) -> Result<(String, String), String> {
    if let Some(channel_id) = requested_channel_id
        .map(str::trim)
        .filter(|channel_id| !channel_id.is_empty())
    {
        let channel = publisher_channel_for_id(connection, channel_id)?
            .ok_or_else(|| "Publisher channel was not found.".to_string())?;
        if channel.profile_id != profile.id {
            return Err("Publisher channel belongs to a different profile.".to_string());
        }

        return Ok((channel.id, channel.channel_identifier));
    }

    let channel_id = format!(
        "channel-{}",
        stable_suffix(&format!("{}:{}", profile.id, channel_title))
    );
    if let Some(channel) = publisher_channel_for_id(connection, &channel_id)? {
        if channel.profile_id != profile.id {
            return Err("Publisher channel belongs to a different profile.".to_string());
        }

        return Ok((channel.id, channel.channel_identifier));
    }

    let identifier = format!("duroos-channel:{channel_id}");
    Ok((channel_id, identifier))
}

fn upsert_publisher_channel(
    connection: &Connection,
    channel: &PublisherChannelUpsert<'_>,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT INTO publisher_channels
             (id, profile_id, title, description, channel_identifier, naddr,
              canonical_channel_link, last_manifest_sha256, last_manifest_url,
              last_published_at, media_count, post_count, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)
             ON CONFLICT(id) DO UPDATE SET
               title = excluded.title,
               description = excluded.description,
               channel_identifier = excluded.channel_identifier,
               naddr = excluded.naddr,
               canonical_channel_link = excluded.canonical_channel_link,
               last_manifest_sha256 = excluded.last_manifest_sha256,
               last_manifest_url = excluded.last_manifest_url,
               last_published_at = excluded.last_published_at,
               media_count = excluded.media_count,
               post_count = excluded.post_count,
               updated_at = excluded.updated_at",
            params![
                channel.id,
                channel.profile_id,
                channel.title,
                channel.description,
                channel.channel_identifier,
                channel.naddr,
                channel.canonical_channel_link,
                channel.last_manifest_sha256,
                channel.last_manifest_url,
                channel.last_published_at,
                channel.media_count,
                channel.post_count,
                now
            ],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn fetch_published_channel_items(
    connection: &Connection,
    channel_id: &str,
) -> Result<Vec<PublishedChannelItem>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, channel_id, item_type, title, content_type, description,
                    origin_url, retrieval_url, sha256, size_bytes, mime_type, published_at
             FROM publisher_channel_items
             WHERE channel_id = ?1
             ORDER BY published_at ASC, title ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![channel_id], published_channel_item_from_row)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn published_channel_item_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PublishedChannelItem> {
    Ok(PublishedChannelItem {
        id: row.get(0)?,
        channel_id: row.get(1)?,
        item_type: row.get(2)?,
        title: row.get(3)?,
        content_type: row.get(4)?,
        description: row.get(5)?,
        origin_url: row.get(6)?,
        retrieval_url: row.get(7)?,
        sha256: row.get(8)?,
        size_bytes: row.get(9)?,
        mime_type: row.get(10)?,
        published_at: row.get(11)?,
    })
}

fn upsert_published_channel_items(
    connection: &Connection,
    items: &[PublishedChannelItem],
) -> Result<(), String> {
    for item in items {
        connection
            .execute(
                "INSERT INTO publisher_channel_items
                 (id, channel_id, item_type, title, content_type, description, origin_url,
                  retrieval_url, sha256, size_bytes, mime_type, published_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(id) DO UPDATE SET
                   title = excluded.title,
                   description = excluded.description,
                   origin_url = excluded.origin_url,
                   retrieval_url = excluded.retrieval_url,
                   sha256 = excluded.sha256,
                   size_bytes = excluded.size_bytes,
                   mime_type = excluded.mime_type,
                   published_at = excluded.published_at",
                params![
                    item.id,
                    item.channel_id,
                    item.item_type,
                    item.title,
                    item.content_type,
                    item.description,
                    item.origin_url,
                    item.retrieval_url,
                    item.sha256,
                    item.size_bytes,
                    item.mime_type,
                    item.published_at
                ],
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn channel_item_from_blob(
    channel_id: &str,
    blob: PublishedBlob,
    published_at: &str,
) -> PublishedChannelItem {
    PublishedChannelItem {
        id: format!(
            "channel-item-{}",
            stable_suffix(&format!("{channel_id}:media:{}", blob.sha256))
        ),
        channel_id: channel_id.to_string(),
        item_type: "media".to_string(),
        title: blob.title,
        content_type: blob.content_type,
        description: blob.description,
        origin_url: blob.url.clone(),
        retrieval_url: Some(blob.url),
        sha256: blob.sha256,
        size_bytes: Some(blob.size_bytes),
        mime_type: Some(blob.mime_type),
        published_at: published_at.to_string(),
    }
}

fn normalize_post_drafts(
    posts: Vec<PublishedPostDraft>,
) -> Result<Vec<PublishedPostDraft>, String> {
    posts
        .into_iter()
        .map(|post| {
            let title = clip_publish_text(&trimmed_required(&post.title, "Post title")?, 140);
            let body = clip_publish_text(&trimmed_required(&post.body, "Post body")?, 4000);
            Ok(PublishedPostDraft { title, body })
        })
        .collect()
}

fn channel_item_from_post(
    channel_id: &str,
    post: &PublishedPostDraft,
    published_at: &str,
) -> PublishedChannelItem {
    let canonical_post = json!({
        "title": post.title,
        "body": post.body,
    })
    .to_string();
    let post_hash = sha256_hex(canonical_post.as_bytes());
    let origin_url = format!(
        "https://duroos.local/channels/{}/posts/{}",
        channel_id,
        &post_hash[..16]
    );

    PublishedChannelItem {
        id: format!(
            "channel-item-{}",
            stable_suffix(&format!("{channel_id}:post:{post_hash}"))
        ),
        channel_id: channel_id.to_string(),
        item_type: "post".to_string(),
        title: post.title.clone(),
        content_type: "post".to_string(),
        description: Some(post.body.clone()),
        origin_url,
        retrieval_url: None,
        sha256: post_hash,
        size_bytes: Some(post.body.len() as i64),
        mime_type: Some("text/plain; charset=utf-8".to_string()),
        published_at: published_at.to_string(),
    }
}

fn clip_publish_text(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect::<String>()
}

fn publisher_profile_for_id(
    connection: &Connection,
    app: &AppHandle,
    profile_id: &str,
) -> Result<Option<PublisherProfile>, String> {
    connection
        .query_row(
            "SELECT id, display_name, curator_public_key, nostr_pubkey, relays_json,
                    blossom_servers_json, vault_path, created_at, updated_at
             FROM publisher_profiles
             WHERE id = ?1",
            params![profile_id],
            |row| publisher_profile_from_row(row, app),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn publisher_profile_from_row(
    row: &rusqlite::Row<'_>,
    app: &AppHandle,
) -> rusqlite::Result<PublisherProfile> {
    let relays_json: String = row.get(4)?;
    let blossom_servers_json: String = row.get(5)?;
    let vault_path: String = row.get(6)?;
    Ok(PublisherProfile {
        id: row.get(0)?,
        display_name: row.get(1)?,
        curator_public_key: row.get(2)?,
        nostr_pubkey: row.get(3)?,
        relays: serde_json::from_str(&relays_json).unwrap_or_default(),
        blossom_servers: serde_json::from_str(&blossom_servers_json).unwrap_or_default(),
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        vault_configured: resolve_vault_path(app, &vault_path).is_file(),
    })
}

fn unlock_profile_keys(
    app: &AppHandle,
    profile: &PublisherProfile,
    passphrase: &str,
) -> Result<PublisherKeys, String> {
    let connection = db::open_connection(app)?;
    let vault_path: String = connection
        .query_row(
            "SELECT vault_path FROM publisher_profiles WHERE id = ?1",
            params![&profile.id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let vault_path = resolve_vault_path(app, &vault_path);
    let vault_json = fs::read_to_string(&vault_path)
        .map_err(|error| format!("Could not read publisher vault: {error}"))?;
    let vault: VaultFile = serde_json::from_str(&vault_json)
        .map_err(|error| format!("Publisher vault is not valid JSON: {error}"))?;
    let mut plaintext = decrypt_vault(passphrase, &vault)?;
    let curator_secret = decode_base64_32(&plaintext.curator_secret_key, "curator secret key")?;
    let nostr_secret = decode_hex_32(&plaintext.nostr_secret_key, "Nostr secret key")?;
    plaintext.curator_secret_key.zeroize();
    plaintext.nostr_secret_key.zeroize();

    let expected_curator_public_key =
        general_purpose::STANDARD.encode(SigningKey::from_bytes(&curator_secret).verifying_key());
    if expected_curator_public_key != profile.curator_public_key {
        return Err("Publisher vault does not match this curator profile.".to_string());
    }
    let expected_nostr_pubkey = nostr_pubkey_from_secret(&nostr_secret)?;
    if expected_nostr_pubkey != profile.nostr_pubkey {
        return Err("Publisher vault does not match this Nostr profile.".to_string());
    }

    Ok(PublisherKeys {
        curator_secret_key: curator_secret,
        nostr_secret_key: nostr_secret,
    })
}

fn publish_lesson_blob(
    client: &Client,
    draft: &PublishedLessonDraft,
    blossom_servers: &[BlossomServerConfig],
    keys: &PublisherKeys,
    blossom_results: &mut Vec<BlossomUploadResult>,
) -> Result<PublishedBlob, String> {
    let title = trimmed_required(&draft.title, "Lesson title")?;
    let content_type = valid_content_type(&draft.content_type)?;
    let path = Path::new(draft.path.trim());
    if !path.is_file() {
        return Err(format!("{} is not a readable file.", draft.path));
    }
    let data =
        fs::read(path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let sha256 = sha256_hex(&data);
    let mime_type = mime_type_for_path(path, &content_type);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_else(|| extension_for_content_type(&content_type));
    let upload =
        upload_blob_to_servers(client, &data, &mime_type, extension, blossom_servers, keys);
    blossom_results.extend(upload.results);
    let url = upload
        .first_url
        .ok_or_else(|| format!("Upload failed for \"{title}\" on every Blossom server."))?;

    Ok(PublishedBlob {
        title,
        content_type,
        description: draft
            .description
            .clone()
            .filter(|value| !value.trim().is_empty()),
        url,
        sha256,
        size_bytes: data.len() as i64,
        mime_type,
    })
}

struct BlobUploadAggregate {
    first_url: Option<String>,
    urls: Vec<String>,
    results: Vec<BlossomUploadResult>,
}

fn upload_blob_to_servers(
    client: &Client,
    data: &[u8],
    mime_type: &str,
    extension: &str,
    blossom_servers: &[BlossomServerConfig],
    keys: &PublisherKeys,
) -> BlobUploadAggregate {
    let hash = sha256_hex(data);
    let mut first_url = None;
    let mut urls = Vec::new();
    let mut results = Vec::new();

    for server in blossom_servers {
        let upload_url = format!("{}/upload", server.url.trim_end_matches('/'));
        let blob_url = format!(
            "{}/{}.{}",
            server.url.trim_end_matches('/'),
            hash,
            safe_extension(extension)
        );
        let auth_header = blossom_auth_header(&keys.nostr_secret_key, &server.url, &hash);
        let mut request = client
            .put(&upload_url)
            .header("Content-Type", mime_type)
            .header("X-SHA-256", &hash)
            .body(data.to_vec());

        if let Ok(header) = auth_header {
            request = request.header("Authorization", header);
        }

        match request.send() {
            Ok(response) if response.status().is_success() => {
                if first_url.is_none() {
                    first_url = Some(blob_url.clone());
                }
                urls.push(blob_url.clone());
                results.push(BlossomUploadResult {
                    server_url: server.url.clone(),
                    hash: format!("sha256:{hash}"),
                    url: Some(blob_url),
                    uploaded: true,
                    message: "Blob stored by server.".to_string(),
                });
            }
            Ok(response) => {
                let status = response.status();
                let body = response.text().unwrap_or_default();
                results.push(BlossomUploadResult {
                    server_url: server.url.clone(),
                    hash: format!("sha256:{hash}"),
                    url: None,
                    uploaded: false,
                    message: format!(
                        "Upload failed with HTTP {status}. {}",
                        clip_text(&body, 160)
                    ),
                });
            }
            Err(error) => {
                results.push(BlossomUploadResult {
                    server_url: server.url.clone(),
                    hash: format!("sha256:{hash}"),
                    url: None,
                    uploaded: false,
                    message: error.to_string(),
                });
            }
        }
    }

    BlobUploadAggregate {
        first_url,
        urls,
        results,
    }
}

fn publish_archive_mirrors(
    client: &Client,
    manifest_data: &[u8],
    manifest_sha256: &str,
    archive_mirrors: &[ArchiveMirrorConfig],
) -> Vec<ArchiveMirrorResult> {
    archive_mirrors
        .iter()
        .map(|mirror| {
            if mirror.service == "ipfs-http-api" {
                publish_manifest_to_ipfs(client, manifest_data, manifest_sha256, mirror)
            } else {
                verify_manual_archive_mirror(client, manifest_sha256, mirror)
            }
        })
        .collect()
}

fn publish_manifest_to_ipfs(
    client: &Client,
    manifest_data: &[u8],
    manifest_sha256: &str,
    mirror: &ArchiveMirrorConfig,
) -> ArchiveMirrorResult {
    let Some(gateway_url) = mirror.gateway_url.as_ref() else {
        return ArchiveMirrorResult {
            service: mirror.service.clone(),
            endpoint_url: mirror.url.clone(),
            url: None,
            cid: None,
            archived: false,
            verified: false,
            message: "IPFS archive mirror needs an HTTP gateway URL.".to_string(),
        };
    };
    let add_url = format!(
        "{}/api/v0/add?pin=true&cid-version=1&wrap-with-directory=false",
        mirror.url.trim_end_matches('/')
    );
    let part = match multipart::Part::bytes(manifest_data.to_vec())
        .file_name("duroos-channel-manifest.json")
        .mime_str("application/json")
    {
        Ok(part) => part,
        Err(error) => {
            return ArchiveMirrorResult {
                service: mirror.service.clone(),
                endpoint_url: mirror.url.clone(),
                url: None,
                cid: None,
                archived: false,
                verified: false,
                message: error.to_string(),
            }
        }
    };
    let form = multipart::Form::new().part("file", part);

    match client.post(&add_url).multipart(form).send() {
        Ok(response) if response.status().is_success() => {
            let body = match response.text() {
                Ok(body) => body,
                Err(error) => {
                    return ArchiveMirrorResult {
                        service: mirror.service.clone(),
                        endpoint_url: mirror.url.clone(),
                        url: None,
                        cid: None,
                        archived: true,
                        verified: false,
                        message: format!(
                            "IPFS add succeeded but response could not be read: {error}"
                        ),
                    }
                }
            };
            let Some(cid) = parse_ipfs_add_cid(&body) else {
                return ArchiveMirrorResult {
                    service: mirror.service.clone(),
                    endpoint_url: mirror.url.clone(),
                    url: None,
                    cid: None,
                    archived: true,
                    verified: false,
                    message: "IPFS add response did not include a CID.".to_string(),
                };
            };
            let url = format!("{}/{}", gateway_url.trim_end_matches('/'), cid);
            match verify_manifest_url(client, &url, manifest_sha256) {
                Ok(()) => ArchiveMirrorResult {
                    service: mirror.service.clone(),
                    endpoint_url: mirror.url.clone(),
                    url: Some(url),
                    cid: Some(cid),
                    archived: true,
                    verified: true,
                    message: "Manifest pinned and verified through the configured IPFS gateway."
                        .to_string(),
                },
                Err(error) => ArchiveMirrorResult {
                    service: mirror.service.clone(),
                    endpoint_url: mirror.url.clone(),
                    url: Some(url),
                    cid: Some(cid),
                    archived: true,
                    verified: false,
                    message: format!("Manifest pinned, but gateway verification failed: {error}"),
                },
            }
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            ArchiveMirrorResult {
                service: mirror.service.clone(),
                endpoint_url: mirror.url.clone(),
                url: None,
                cid: None,
                archived: false,
                verified: false,
                message: format!(
                    "IPFS add failed with HTTP {status}. {}",
                    clip_text(&body, 160)
                ),
            }
        }
        Err(error) => ArchiveMirrorResult {
            service: mirror.service.clone(),
            endpoint_url: mirror.url.clone(),
            url: None,
            cid: None,
            archived: false,
            verified: false,
            message: error.to_string(),
        },
    }
}

fn verify_manual_archive_mirror(
    client: &Client,
    manifest_sha256: &str,
    mirror: &ArchiveMirrorConfig,
) -> ArchiveMirrorResult {
    match verify_manifest_url(client, &mirror.url, manifest_sha256) {
        Ok(()) => ArchiveMirrorResult {
            service: mirror.service.clone(),
            endpoint_url: mirror.url.clone(),
            url: Some(mirror.url.clone()),
            cid: None,
            archived: true,
            verified: true,
            message: "Archive mirror matched the signed manifest hash.".to_string(),
        },
        Err(error) => ArchiveMirrorResult {
            service: mirror.service.clone(),
            endpoint_url: mirror.url.clone(),
            url: Some(mirror.url.clone()),
            cid: None,
            archived: false,
            verified: false,
            message: error,
        },
    }
}

fn verify_manifest_url(client: &Client, url: &str, expected_sha256: &str) -> Result<(), String> {
    if !is_safe_http_url(url) {
        return Err("Archive mirror URLs must be http or https.".to_string());
    }
    let expected = expected_sha256
        .strip_prefix("sha256:")
        .unwrap_or(expected_sha256)
        .to_ascii_lowercase();
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("{url}: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("{url}: HTTP {status}"));
    }
    let body = response
        .bytes()
        .map_err(|error| format!("{url}: could not read manifest: {error}"))?;
    let actual = sha256_hex(body.as_ref());
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{url}: hash mismatch, expected sha256:{expected}, got sha256:{actual}"
        ))
    }
}

fn parse_ipfs_add_cid(body: &str) -> Option<String> {
    body.lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|value| {
            value
                .get("Hash")
                .or_else(|| value.get("Cid"))
                .and_then(Value::as_str)
                .filter(|cid| is_safe_archive_identifier(cid))
                .map(str::to_string)
        })
}

fn signed_channel_manifest(input: SignedManifestInput<'_>) -> Result<(String, String), String> {
    let SignedManifestInput {
        profile,
        keys,
        naddr,
        channel_title,
        channel_description,
        relays,
        blossom_servers,
        items,
        published_at,
    } = input;

    let relays_json = relays
        .iter()
        .map(|relay| relay.url.clone())
        .collect::<Vec<_>>();
    let blossom_json = blossom_servers
        .iter()
        .map(|server| server.url.clone())
        .collect::<Vec<_>>();
    let lessons = items
        .iter()
        .map(|item| {
            let mut lesson = json!({
                "title": item.title,
                "contentType": item.content_type,
                "sourceRefs": [{
                    "platform": if item.item_type == "post" { "duroos-post" } else { "blossom" },
                    "originUrl": item.origin_url,
                    "publishedAt": item.published_at,
                }],
                "contentHashes": [format!("sha256:{}", item.sha256)],
                "provenance": {
                    "permissionNote": if item.item_type == "post" {
                        "Published as a signed teacher text post through Duroos federated publishing."
                    } else {
                        "Published by the teacher through Duroos federated publishing; users must still confirm rights before redistribution."
                    },
                    "adapterName": "DuroosFederatedPublisher",
                    "importedAt": item.published_at,
                },
                "description": item.description,
            });

            if let Some(retrieval_url) = &item.retrieval_url {
                lesson["retrievalRefs"] = json!([{
                    "kind": "direct-url",
                    "url": retrieval_url,
                    "service": "blossom",
                    "sha256": format!("sha256:{}", item.sha256),
                    "sizeBytes": item.size_bytes.unwrap_or_default(),
                    "mimeType": item.mime_type.clone().unwrap_or_else(|| "application/octet-stream".to_string()),
                    "mediaType": item.mime_type.clone().unwrap_or_else(|| "application/octet-stream".to_string()),
                }]);
            }

            lesson
        })
        .collect::<Vec<_>>();
    let mut manifest_value = json!({
        "schemaVersion": 2,
        "exportedAt": published_at,
        "curator": {
            "id": profile.id,
            "displayName": profile.display_name,
            "publicKey": profile.curator_public_key,
            "nostrPubkey": profile.nostr_pubkey,
        },
        "publication": {
            "transport": "nostr",
            "naddr": naddr,
            "relays": relays_json,
            "blossomServers": blossom_json,
            "manifestSha256": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "publishedAt": published_at,
        },
        "collection": {
            "title": channel_title,
            "ownerLabel": profile.display_name,
            "description": channel_description,
        },
        "lessons": lessons,
    });
    let payload_hash = sha256_hex(manifest::signed_payload(&manifest_value)?.as_bytes());
    manifest_value["publication"]["manifestSha256"] = json!(format!("sha256:{payload_hash}"));
    let payload = manifest::signed_payload(&manifest_value)?;
    let signing_key = SigningKey::from_bytes(&keys.curator_secret_key);
    let signature = signing_key.sign(payload.as_bytes());
    manifest_value["signature"] = json!({
        "algorithm": "ed25519",
        "publicKey": profile.curator_public_key,
        "value": general_purpose::STANDARD.encode(signature.to_bytes()),
    });
    let report = manifest::validate_collection_manifest(&manifest_value.to_string());
    if !report.valid {
        return Err(format!(
            "Generated manifest did not validate: {}",
            report.errors.join("; ")
        ));
    }
    let json = serde_json::to_string_pretty(&manifest_value).map_err(|error| error.to_string())?;
    Ok((json, payload_hash))
}

fn publish_event_to_relays(
    event: &NostrEvent,
    relays: &[NostrRelayConfig],
) -> Vec<NostrRelayPublishResult> {
    let event_json = match serde_json::to_value(event) {
        Ok(value) => value,
        Err(error) => {
            return relays
                .iter()
                .map(|relay| NostrRelayPublishResult {
                    relay_url: relay.url.clone(),
                    accepted: false,
                    message: error.to_string(),
                })
                .collect()
        }
    };
    let message = json!(["EVENT", event_json]).to_string();

    relays
        .iter()
        .map(|relay| match connect(&relay.url) {
            Ok((mut socket, _)) => {
                if let Err(error) = socket.send(WsMessage::Text(message.clone())) {
                    return NostrRelayPublishResult {
                        relay_url: relay.url.clone(),
                        accepted: false,
                        message: error.to_string(),
                    };
                }

                for _ in 0..8 {
                    match socket.read() {
                        Ok(WsMessage::Text(text)) => {
                            if let Some(result) = parse_ok_message(&text, &event.id) {
                                return NostrRelayPublishResult {
                                    relay_url: relay.url.clone(),
                                    accepted: result.0,
                                    message: result.1,
                                };
                            }
                        }
                        Ok(_) => {}
                        Err(error) => {
                            return NostrRelayPublishResult {
                                relay_url: relay.url.clone(),
                                accepted: false,
                                message: error.to_string(),
                            };
                        }
                    }
                }

                NostrRelayPublishResult {
                    relay_url: relay.url.clone(),
                    accepted: false,
                    message: "Relay did not return an OK message.".to_string(),
                }
            }
            Err(error) => NostrRelayPublishResult {
                relay_url: relay.url.clone(),
                accepted: false,
                message: error.to_string(),
            },
        })
        .collect()
}

fn fetch_channel_event(relay: &str, parsed: &ParsedNaddr) -> Result<NostrEvent, String> {
    let filter = json!({
        "authors": [parsed.author],
        "kinds": [DUROOS_CHANNEL_KIND],
        "#d": [parsed.identifier],
        "limit": 1,
    });
    let request = json!(["REQ", "duroos-channel", filter]).to_string();
    let (mut socket, _) = connect(relay).map_err(|error| error.to_string())?;
    socket
        .send(WsMessage::Text(request))
        .map_err(|error| error.to_string())?;

    let mut latest: Option<NostrEvent> = None;
    for _ in 0..16 {
        if let WsMessage::Text(text) = socket.read().map_err(|error| error.to_string())? {
            let parsed_message: Value =
                serde_json::from_str(&text).map_err(|error| error.to_string())?;
            let Some(items) = parsed_message.as_array() else {
                continue;
            };
            if items.first().and_then(Value::as_str) == Some("EVENT") {
                if let Some(event_value) = items.get(2) {
                    let event: NostrEvent = serde_json::from_value(event_value.clone())
                        .map_err(|error| error.to_string())?;
                    if event.kind == DUROOS_CHANNEL_KIND
                        && event.pubkey == parsed.author
                        && event.tags.iter().any(|tag| {
                            tag.first().map(String::as_str) == Some("d")
                                && tag.get(1).map(String::as_str)
                                    == Some(parsed.identifier.as_str())
                        })
                        && latest
                            .as_ref()
                            .map(|current| event.created_at > current.created_at)
                            .unwrap_or(true)
                    {
                        latest = Some(event);
                    }
                }
            }
            if items.first().and_then(Value::as_str) == Some("EOSE") && latest.is_some() {
                break;
            }
        }
    }

    latest.ok_or_else(|| "Relay did not return a Duroos channel event.".to_string())
}

fn signed_nostr_event(
    secret_key: &[u8; 32],
    kind: u64,
    tags: Vec<Vec<String>>,
    content: String,
) -> Result<NostrEvent, String> {
    let pubkey = nostr_pubkey_from_secret(secret_key)?;
    let created_at = Utc::now().timestamp();
    let serialized = serde_json::to_string(&json!([0, pubkey, created_at, kind, tags, content]))
        .map_err(|error| error.to_string())?;
    let id_bytes = Sha256::digest(serialized.as_bytes());
    let id = hex_lower(&id_bytes);
    let secret = SecretKey::from_slice(secret_key).map_err(|error| error.to_string())?;
    let secp = Secp256k1::new();
    let keypair = Keypair::from_secret_key(&secp, &secret);
    let message = SecpMessage::from_digest_slice(&id_bytes).map_err(|error| error.to_string())?;
    let signature = secp.sign_schnorr_no_aux_rand(&message, &keypair);

    Ok(NostrEvent {
        id,
        pubkey,
        created_at,
        kind,
        tags,
        content,
        sig: signature.to_string(),
    })
}

fn endpoint_probe_event(
    keys: &PublisherKeys,
    profile: &PublisherProfile,
    blossom_urls: &[String],
) -> Result<NostrEvent, String> {
    let probe_id = format!("duroos-endpoint-test:{}", Uuid::new_v4());
    let mut tags = vec![
        vec!["d".to_string(), probe_id],
        vec!["client".to_string(), APP_TAG.to_string()],
        vec!["t".to_string(), "duroos-endpoint-test".to_string()],
        vec![
            "alt".to_string(),
            "Duroos Watcher publisher endpoint test".to_string(),
        ],
    ];
    tags.extend(
        blossom_urls
            .iter()
            .map(|url| vec!["r".to_string(), url.clone()]),
    );
    let content = json!({
        "app": APP_TAG,
        "type": "publisher-endpoint-test",
        "profileId": profile.id,
        "curatorPublicKey": profile.curator_public_key,
        "publishedAt": Utc::now().to_rfc3339(),
        "message": "Small public probe used to verify Duroos Watcher publisher relay/storage configuration."
    })
    .to_string();

    signed_nostr_event(&keys.nostr_secret_key, DUROOS_CHANNEL_KIND, tags, content)
}

fn blossom_auth_header(
    secret_key: &[u8; 32],
    server_url: &str,
    sha256: &str,
) -> Result<String, String> {
    let domain = Url::parse(server_url)
        .map_err(|error| error.to_string())?
        .host_str()
        .ok_or_else(|| "Blossom server URL needs a host.".to_string())?
        .to_ascii_lowercase();
    let event = signed_nostr_event(
        secret_key,
        BLOSSOM_AUTH_KIND,
        vec![
            vec!["t".to_string(), "upload".to_string()],
            vec![
                "expiration".to_string(),
                (Utc::now().timestamp() + 600).to_string(),
            ],
            vec!["server".to_string(), domain],
            vec!["x".to_string(), sha256.to_string()],
        ],
        "Upload Blob".to_string(),
    )?;
    let json = serde_json::to_string(&event).map_err(|error| error.to_string())?;
    Ok(format!(
        "Nostr {}",
        general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes())
    ))
}

fn parse_ok_message(text: &str, event_id: &str) -> Option<(bool, String)> {
    let value: Value = serde_json::from_str(text).ok()?;
    let items = value.as_array()?;
    if items.first()?.as_str()? != "OK" || items.get(1)?.as_str()? != event_id {
        return None;
    }
    Some((
        items.get(2)?.as_bool()?,
        items
            .get(3)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    ))
}

fn encrypt_vault(passphrase: &str, plaintext: &VaultPlaintext) -> Result<VaultFile, String> {
    let salt = random_16_bytes();
    let nonce = random_24_bytes();
    let mut key = derive_vault_key(passphrase, &salt)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|error| error.to_string())?;
    let plaintext_json = serde_json::to_vec(plaintext).map_err(|error| error.to_string())?;
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext_json.as_ref())
        .map_err(|_| "Could not encrypt publisher vault.".to_string())?;
    key.zeroize();

    Ok(VaultFile {
        version: VAULT_VERSION,
        kdf: "argon2id".to_string(),
        cipher: "xchacha20poly1305".to_string(),
        salt: general_purpose::STANDARD.encode(salt),
        nonce: general_purpose::STANDARD.encode(nonce),
        ciphertext: general_purpose::STANDARD.encode(ciphertext),
    })
}

fn random_16_bytes() -> [u8; 16] {
    OsRng.gen()
}

fn random_24_bytes() -> [u8; 24] {
    OsRng.gen()
}

fn random_32_bytes() -> [u8; 32] {
    OsRng.gen()
}

fn decrypt_vault(passphrase: &str, vault: &VaultFile) -> Result<VaultPlaintext, String> {
    if vault.version != VAULT_VERSION
        || vault.kdf != "argon2id"
        || vault.cipher != "xchacha20poly1305"
    {
        return Err("Unsupported publisher vault format.".to_string());
    }
    let salt = general_purpose::STANDARD
        .decode(&vault.salt)
        .map_err(|_| "Publisher vault salt is invalid.".to_string())?;
    let nonce = general_purpose::STANDARD
        .decode(&vault.nonce)
        .map_err(|_| "Publisher vault nonce is invalid.".to_string())?;
    let ciphertext = general_purpose::STANDARD
        .decode(&vault.ciphertext)
        .map_err(|_| "Publisher vault ciphertext is invalid.".to_string())?;
    if nonce.len() != 24 {
        return Err("Publisher vault nonce has the wrong length.".to_string());
    }
    let mut key = derive_vault_key(passphrase, &salt)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|error| error.to_string())?;
    let plaintext = cipher
        .decrypt(XNonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| "Publisher vault could not be unlocked with that passphrase.".to_string())?;
    key.zeroize();
    serde_json::from_slice(&plaintext).map_err(|error| error.to_string())
}

fn derive_vault_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    let mut key = [0_u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|error| error.to_string())?;
    Ok(key)
}

fn encode_naddr(
    identifier: &str,
    author_hex: &str,
    kind: u32,
    relays: &[String],
) -> Result<String, String> {
    let mut payload = Vec::new();
    push_tlv(&mut payload, 0, identifier.as_bytes())?;
    for relay in relays {
        push_tlv(&mut payload, 1, relay.as_bytes())?;
    }
    push_tlv(&mut payload, 2, &decode_hex(author_hex)?)?;
    push_tlv(&mut payload, 3, &kind.to_be_bytes())?;
    bech32_encode("naddr", &payload)
}

fn decode_naddr(input: &str) -> Result<ParsedNaddr, String> {
    let raw = input.trim().strip_prefix("nostr:").unwrap_or(input.trim());
    let (hrp, data) = bech32_decode(raw)?;
    if hrp != "naddr" {
        return Err("Nostr channel link must be an naddr.".to_string());
    }
    let bytes = convert_bits(&data, 5, 8, false)?;
    let mut index = 0;
    let mut identifier = None;
    let mut author = None;
    let mut kind = None;
    let mut relays = Vec::new();

    while index + 2 <= bytes.len() {
        let tag = bytes[index];
        let len = bytes[index + 1] as usize;
        index += 2;
        if index + len > bytes.len() {
            return Err("Nostr naddr TLV is truncated.".to_string());
        }
        let value = &bytes[index..index + len];
        match tag {
            0 => {
                identifier =
                    Some(String::from_utf8(value.to_vec()).map_err(|error| error.to_string())?)
            }
            1 => relays.push(String::from_utf8(value.to_vec()).map_err(|error| error.to_string())?),
            2 if value.len() == 32 => author = Some(hex_lower(value)),
            3 if value.len() == 4 => {
                kind = Some(u32::from_be_bytes([value[0], value[1], value[2], value[3]]) as u64);
            }
            _ => {}
        }
        index += len;
    }

    Ok(ParsedNaddr {
        raw: raw.to_string(),
        identifier: identifier
            .ok_or_else(|| "Nostr naddr is missing a channel identifier.".to_string())?,
        author: author.ok_or_else(|| "Nostr naddr is missing an author.".to_string())?,
        kind: kind.ok_or_else(|| "Nostr naddr is missing an event kind.".to_string())?,
        relays,
    })
}

fn canonical_channel_link(naddr: &str) -> String {
    let raw = naddr.trim().strip_prefix("nostr:").unwrap_or(naddr.trim());
    format!("nostr:{raw}")
}

fn manifest_verification_code(manifest_sha256: &str) -> String {
    let hash = manifest_sha256
        .trim()
        .strip_prefix("sha256:")
        .unwrap_or(manifest_sha256.trim());
    if !looks_like_sha256(manifest_sha256.trim()) {
        return "DW-UNVERIFIED".to_string();
    }
    let prefix = hash[..12].to_ascii_uppercase();
    format!("DW-{}-{}-{}", &prefix[0..4], &prefix[4..8], &prefix[8..12])
}

fn channel_invite_text(
    channel_title: &str,
    teacher_display_name: &str,
    canonical_channel_link: &str,
    manifest_sha256: &str,
    verification_code: &str,
) -> String {
    [
        "Duroos channel invite".to_string(),
        format!("Channel: {channel_title}"),
        format!("Teacher: {teacher_display_name}"),
        format!("Open in Duroos Watcher: {canonical_channel_link}"),
        format!("Manifest: {manifest_sha256}"),
        format!("Check code: {verification_code}"),
        "Preview before trusting this teacher key.".to_string(),
    ]
    .join("\n")
}

fn push_tlv(out: &mut Vec<u8>, tag: u8, value: &[u8]) -> Result<(), String> {
    if value.len() > u8::MAX as usize {
        return Err("Nostr naddr value is too long.".to_string());
    }
    out.push(tag);
    out.push(value.len() as u8);
    out.extend_from_slice(value);
    Ok(())
}

fn bech32_encode(hrp: &str, payload: &[u8]) -> Result<String, String> {
    let mut data = convert_bits(payload, 8, 5, true)?;
    let checksum = bech32_checksum(hrp, &data);
    data.extend(checksum);
    let mut output = String::with_capacity(hrp.len() + 1 + data.len());
    output.push_str(hrp);
    output.push('1');
    for value in data {
        output.push(
            *BECH32_CHARSET
                .get(value as usize)
                .ok_or_else(|| "Invalid bech32 value.".to_string())? as char,
        );
    }
    Ok(output)
}

fn bech32_decode(input: &str) -> Result<(String, Vec<u8>), String> {
    let lower = input.to_ascii_lowercase();
    if input != lower && input != input.to_ascii_uppercase() {
        return Err("Bech32 strings cannot mix casing.".to_string());
    }
    let separator = lower
        .rfind('1')
        .ok_or_else(|| "Bech32 separator is missing.".to_string())?;
    let hrp = lower[..separator].to_string();
    let encoded = &lower[separator + 1..];
    if hrp.is_empty() || encoded.len() < 6 {
        return Err("Bech32 value is incomplete.".to_string());
    }
    let mut data = Vec::new();
    for character in encoded.bytes() {
        let Some(value) = BECH32_CHARSET.iter().position(|item| *item == character) else {
            return Err("Bech32 value contains an unsupported character.".to_string());
        };
        data.push(value as u8);
    }
    if bech32_polymod(&[bech32_hrp_expand(&hrp), data.clone()].concat()) != 1 {
        return Err("Bech32 checksum is invalid.".to_string());
    }
    let payload_len = data.len() - 6;
    data.truncate(payload_len);
    Ok((hrp, data))
}

const BECH32_CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

fn bech32_checksum(hrp: &str, data: &[u8]) -> Vec<u8> {
    let mut values = bech32_hrp_expand(hrp);
    values.extend_from_slice(data);
    values.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    let polymod = bech32_polymod(&values) ^ 1;
    (0..6)
        .map(|index| ((polymod >> (5 * (5 - index))) & 31) as u8)
        .collect()
}

fn bech32_hrp_expand(hrp: &str) -> Vec<u8> {
    let mut expanded = hrp.bytes().map(|byte| byte >> 5).collect::<Vec<_>>();
    expanded.push(0);
    expanded.extend(hrp.bytes().map(|byte| byte & 31));
    expanded
}

fn bech32_polymod(values: &[u8]) -> u32 {
    let generators = [
        0x3b6a57b2_u32,
        0x26508e6d,
        0x1ea119fa,
        0x3d4233dd,
        0x2a1462b3,
    ];
    let mut chk = 1_u32;
    for value in values {
        let top = chk >> 25;
        chk = (chk & 0x1ffffff) << 5 ^ (*value as u32);
        for (index, generator) in generators.iter().enumerate() {
            if (top >> index) & 1 == 1 {
                chk ^= generator;
            }
        }
    }
    chk
}

fn convert_bits(data: &[u8], from: u32, to: u32, pad: bool) -> Result<Vec<u8>, String> {
    let mut acc = 0_u32;
    let mut bits = 0_u32;
    let maxv = (1_u32 << to) - 1;
    let max_acc = (1_u32 << (from + to - 1)) - 1;
    let mut output = Vec::new();

    for value in data {
        let value = *value as u32;
        if value >> from != 0 {
            return Err("Invalid bech32 data range.".to_string());
        }
        acc = ((acc << from) | value) & max_acc;
        bits += from;
        while bits >= to {
            bits -= to;
            output.push(((acc >> bits) & maxv) as u8);
        }
    }

    if pad {
        if bits > 0 {
            output.push(((acc << (to - bits)) & maxv) as u8);
        }
    } else if bits >= from || ((acc << (to - bits)) & maxv) != 0 {
        return Err("Invalid bech32 padding.".to_string());
    }

    Ok(output)
}

fn normalize_relays(relays: Vec<NostrRelayConfig>) -> Result<Vec<NostrRelayConfig>, String> {
    let normalized = relays
        .into_iter()
        .map(|relay| relay.url.trim().trim_end_matches('/').to_string())
        .filter(|url| !url.is_empty())
        .map(|url| {
            if !(url.starts_with("wss://") || url.starts_with("ws://")) {
                Err("Nostr relays must start with ws:// or wss://.".to_string())
            } else {
                Ok(NostrRelayConfig { url })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if normalized.is_empty() {
        return Err("Configure at least one Nostr relay.".to_string());
    }
    Ok(dedupe_relays(normalized))
}

fn normalize_blossom_servers(
    servers: Vec<BlossomServerConfig>,
) -> Result<Vec<BlossomServerConfig>, String> {
    let normalized = servers
        .into_iter()
        .map(|server| server.url.trim().trim_end_matches('/').to_string())
        .filter(|url| !url.is_empty())
        .map(|url| {
            if !is_safe_http_url(&url) {
                Err("Blossom servers must start with http:// or https://.".to_string())
            } else {
                Ok(BlossomServerConfig { url })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if normalized.is_empty() {
        return Err("Configure at least one Blossom server.".to_string());
    }
    Ok(dedupe_blossom(normalized))
}

fn normalize_archive_mirrors(
    mirrors: Vec<ArchiveMirrorConfig>,
) -> Result<Vec<ArchiveMirrorConfig>, String> {
    let mut normalized = Vec::new();

    for mirror in mirrors {
        let url = mirror.url.trim().trim_end_matches('/').to_string();
        if url.is_empty() {
            continue;
        }

        let service = normalize_archive_service(&mirror.service, &url);
        let label = mirror
            .label
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        if service == "ipfs-http-api" {
            let gateway_url = mirror
                .gateway_url
                .as_ref()
                .map(|value| value.trim().trim_end_matches('/').to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    "Local IPFS archive publishing needs an explicit gateway URL.".to_string()
                })?;
            if !is_safe_http_url(&url) || !is_safe_http_url(&gateway_url) {
                return Err(
                    "IPFS archive API and gateway URLs must start with http:// or https://."
                        .to_string(),
                );
            }
            normalized.push(ArchiveMirrorConfig {
                service,
                url,
                gateway_url: Some(gateway_url),
                label,
            });
        } else {
            if !is_safe_http_url(&url) {
                return Err(
                    "Archive mirror URLs must be public http or https gateway URLs.".to_string(),
                );
            }
            normalized.push(ArchiveMirrorConfig {
                service,
                url,
                gateway_url: None,
                label,
            });
        }
    }

    Ok(dedupe_archive_mirrors(normalized))
}

fn normalize_archive_service(service: &str, url: &str) -> String {
    let service = service.trim().to_ascii_lowercase();
    match service.as_str() {
        "ipfs-http-api" | "ipfs-api" | "local-ipfs" => "ipfs-http-api".to_string(),
        "ipfs" | "ipfs-gateway" => "ipfs-gateway".to_string(),
        "arweave" => "arweave".to_string(),
        "filecoin" => "filecoin".to_string(),
        "https" | "http" | "archive" | "manual" => "https".to_string(),
        _ if url.contains("/ipfs/") => "ipfs-gateway".to_string(),
        _ if url.contains("arweave.net/") => "arweave".to_string(),
        _ => "https".to_string(),
    }
}

fn dedupe_relays(relays: Vec<NostrRelayConfig>) -> Vec<NostrRelayConfig> {
    let mut output = Vec::new();
    for relay in relays {
        if !output
            .iter()
            .any(|existing: &NostrRelayConfig| existing.url == relay.url)
        {
            output.push(relay);
        }
    }
    output
}

fn dedupe_blossom(servers: Vec<BlossomServerConfig>) -> Vec<BlossomServerConfig> {
    let mut output = Vec::new();
    for server in servers {
        if !output
            .iter()
            .any(|existing: &BlossomServerConfig| existing.url == server.url)
        {
            output.push(server);
        }
    }
    output
}

fn dedupe_archive_mirrors(mirrors: Vec<ArchiveMirrorConfig>) -> Vec<ArchiveMirrorConfig> {
    let mut output = Vec::new();
    for mirror in mirrors {
        if !output.iter().any(|existing: &ArchiveMirrorConfig| {
            existing.service == mirror.service
                && existing.url == mirror.url
                && existing.gateway_url == mirror.gateway_url
        }) {
            output.push(mirror);
        }
    }
    output
}

fn valid_content_type(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "video" | "audio" | "pdf") {
        Ok(normalized)
    } else {
        Err("Published lessons must be video, audio, or pdf files.".to_string())
    }
}

fn mime_type_for_path(path: &Path, content_type: &str) -> String {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "mp4" => "video/mp4",
        "m4v" => "video/x-m4v",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        "mpg" | "mpeg" => "video/mpeg",
        "ts" | "m2ts" | "mts" => "video/mp2t",
        "vob" => "video/dvd",
        "3gp" => "video/3gpp",
        "3g2" => "video/3gpp2",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "opus" => "audio/opus",
        "wma" => "audio/x-ms-wma",
        "aif" | "aiff" => "audio/aiff",
        "amr" => "audio/amr",
        "pdf" => "application/pdf",
        _ if content_type == "video" => "video/mp4",
        _ if content_type == "audio" => "audio/mpeg",
        _ => "application/pdf",
    }
    .to_string()
}

fn extension_for_content_type(content_type: &str) -> &'static str {
    match content_type {
        "audio" => "mp3",
        "pdf" => "pdf",
        _ => "mp4",
    }
}

fn safe_extension(extension: &str) -> String {
    let clean = extension
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    if clean.is_empty() {
        "bin".to_string()
    } else {
        clean
    }
}

fn trimmed_required(value: &str, label: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{label} is required."))
    } else {
        Ok(trimmed.to_string())
    }
}

fn validate_passphrase(passphrase: &str) -> Result<(), String> {
    if passphrase.len() < 8 {
        return Err("Publisher passphrase must be at least 8 characters.".to_string());
    }
    Ok(())
}

fn publisher_vault_path(app: &AppHandle, profile_id: &str) -> Result<PathBuf, String> {
    Ok(db::app_data_dir(app)?
        .join("publisher-vaults")
        .join(format!("{}.json", safe_path_segment(profile_id))))
}

fn resolve_vault_path(app: &AppHandle, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        db::app_data_dir(app)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn safe_path_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "publisher".to_string()
    } else {
        sanitized
    }
}

fn stable_suffix(value: &str) -> String {
    sha256_hex(value.as_bytes())[..16].to_string()
}

fn sha256_hex(data: &[u8]) -> String {
    hex_lower(&Sha256::digest(data))
}

fn hex_lower(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(data.len() * 2);
    for byte in data {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2)
        || !value.chars().all(|character| character.is_ascii_hexdigit())
    {
        return Err("Hex value is invalid.".to_string());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|chunk| {
            let text = std::str::from_utf8(chunk).map_err(|error| error.to_string())?;
            u8::from_str_radix(text, 16).map_err(|error| error.to_string())
        })
        .collect()
}

fn decode_hex_32(value: &str, label: &str) -> Result<[u8; 32], String> {
    let bytes = decode_hex(value)?;
    bytes
        .try_into()
        .map_err(|_| format!("{label} must be 32 bytes."))
}

fn decode_base64_32(value: &str, label: &str) -> Result<[u8; 32], String> {
    let bytes = general_purpose::STANDARD
        .decode(value)
        .map_err(|_| format!("{label} is not valid base64."))?;
    bytes
        .try_into()
        .map_err(|_| format!("{label} must be 32 bytes."))
}

fn nostr_pubkey_from_secret(secret_key: &[u8; 32]) -> Result<String, String> {
    let secret = SecretKey::from_slice(secret_key).map_err(|error| error.to_string())?;
    let secp = Secp256k1::new();
    let keypair = Keypair::from_secret_key(&secp, &secret);
    let (x_only, _) = XOnlyPublicKey::from_keypair(&keypair);
    Ok(hex_lower(&x_only.serialize()))
}

fn is_safe_http_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn is_safe_archive_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 140
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || character == '-'
                || character == '_'
                || character == '.'
        })
}

fn looks_like_sha256(value: &str) -> bool {
    let hash = value.strip_prefix("sha256:").unwrap_or(value);
    hash.len() == 64 && hash.chars().all(|character| character.is_ascii_hexdigit())
}

fn clip_text(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    value
        .chars()
        .take(max.saturating_sub(1))
        .collect::<String>()
        + "..."
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::schnorr::Signature as SchnorrSignature;
    use std::str::FromStr;
    use std::{net::TcpListener, sync::mpsc, thread};
    use tiny_http::{Response, Server};

    #[test]
    fn naddr_round_trips_channel_coordinates() {
        let author = "11".repeat(32);
        let relays = vec!["wss://relay.example".to_string()];
        let encoded = encode_naddr(
            "duroos-channel:test",
            &author,
            DUROOS_CHANNEL_KIND as u32,
            &relays,
        )
        .unwrap();
        let parsed = decode_naddr(&encoded).unwrap();

        assert_eq!(parsed.identifier, "duroos-channel:test");
        assert_eq!(parsed.author, author);
        assert_eq!(parsed.kind, DUROOS_CHANNEL_KIND);
        assert_eq!(parsed.relays, relays);
    }

    #[test]
    fn channel_invite_uses_canonical_nostr_uri_and_hash_check_code() {
        let naddr = "naddr1qqqqtest";
        let manifest_hash =
            "sha256:83829c50baca669812884d16505873dd9d7318c8ab88e9630c9bfcd1d970570b";
        let canonical = canonical_channel_link(naddr);
        let code = manifest_verification_code(manifest_hash);
        let invite = channel_invite_text(
            "Foundations",
            "Example Teacher",
            &canonical,
            manifest_hash,
            &code,
        );

        assert_eq!(canonical, "nostr:naddr1qqqqtest");
        assert_eq!(code, "DW-8382-9C50-BACA");
        assert!(invite.contains("Channel: Foundations"));
        assert!(invite.contains("Teacher: Example Teacher"));
        assert!(invite.contains("Open in Duroos Watcher: nostr:naddr1qqqqtest"));
        assert!(invite.contains("Check code: DW-8382-9C50-BACA"));
    }

    #[test]
    fn endpoint_test_messages_distinguish_partial_quorum_from_full_pass() {
        let blossom_results = vec![
            BlossomUploadResult {
                server_url: "https://blossom.ok".to_string(),
                hash: "a".repeat(64),
                url: Some("https://blossom.ok/a".to_string()),
                uploaded: true,
                message: "Blob stored by server.".to_string(),
            },
            BlossomUploadResult {
                server_url: "https://blossom.failed".to_string(),
                hash: "b".repeat(64),
                url: None,
                uploaded: false,
                message: "Upload failed.".to_string(),
            },
        ];
        let relay_results = vec![
            NostrRelayPublishResult {
                relay_url: "wss://relay.ok".to_string(),
                accepted: true,
                message: String::new(),
            },
            NostrRelayPublishResult {
                relay_url: "wss://relay.failed".to_string(),
                accepted: false,
                message: "HTTP error.".to_string(),
            },
        ];

        let messages = endpoint_test_messages(true, &blossom_results, &relay_results);

        assert!(messages[0].starts_with("Endpoint quorum passed with failures"));
        assert!(messages[0].contains("1/2 Blossom"));
        assert!(messages[0].contains("1/2 relay"));
        assert!(messages
            .iter()
            .any(|message| message.contains("failed endpoints should be fixed or removed")));
        assert!(!messages[0].starts_with("Endpoint test passed"));
    }

    #[test]
    fn vault_rejects_wrong_passphrase() {
        let plaintext = VaultPlaintext {
            curator_secret_key: general_purpose::STANDARD.encode([1_u8; 32]),
            nostr_secret_key: hex_lower(&[2_u8; 32]),
        };
        let vault = encrypt_vault("correct horse", &plaintext).unwrap();

        assert!(decrypt_vault("wrong horse", &vault).is_err());
        assert!(decrypt_vault("correct horse", &vault).is_ok());
    }

    #[test]
    fn nostr_event_uses_nip01_id_material() {
        let secret = [3_u8; 32];
        let event = signed_nostr_event(
            &secret,
            DUROOS_CHANNEL_KIND,
            vec![vec!["d".to_string(), "duroos-channel:test".to_string()]],
            "{}".to_string(),
        )
        .unwrap();
        let serialized = serde_json::to_string(&json!([
            0,
            event.pubkey,
            event.created_at,
            event.kind,
            event.tags,
            event.content
        ]))
        .unwrap();
        let event_json = serde_json::to_string(&event).unwrap();

        assert_eq!(event.id, sha256_hex(serialized.as_bytes()));
        assert_eq!(event.sig.len(), 128);
        assert!(event_json.contains("\"created_at\""));
        assert!(!event_json.contains("\"createdAt\""));
    }

    #[test]
    fn signed_manifest_does_not_export_local_paths_or_private_keys() {
        let profile = PublisherProfile {
            id: "publisher-test".to_string(),
            display_name: "Test Teacher".to_string(),
            curator_public_key: general_purpose::STANDARD
                .encode(SigningKey::from_bytes(&[4_u8; 32]).verifying_key()),
            nostr_pubkey: nostr_pubkey_from_secret(&[5_u8; 32]).unwrap(),
            relays: vec![],
            blossom_servers: vec![],
            created_at: "2026-06-17T00:00:00Z".to_string(),
            updated_at: "2026-06-17T00:00:00Z".to_string(),
            vault_configured: true,
        };
        let keys = PublisherKeys {
            curator_secret_key: [4_u8; 32],
            nostr_secret_key: [5_u8; 32],
        };
        let local_path = "/Users/example/private/lesson.mp4";
        let blob = PublishedBlob {
            title: "Local lesson".to_string(),
            content_type: "video".to_string(),
            description: Some("A local lesson exported through Blossom.".to_string()),
            url: "https://blossom.example/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.mp4".to_string(),
            sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
            size_bytes: 42,
            mime_type: "video/mp4".to_string(),
        };

        let relays = [NostrRelayConfig {
            url: "wss://relay.example".to_string(),
        }];
        let blossom_servers = [BlossomServerConfig {
            url: "https://blossom.example".to_string(),
        }];
        let items = [channel_item_from_blob(
            "channel-test",
            blob,
            "2026-06-17T00:00:00Z",
        )];
        let (manifest_json, _) = signed_channel_manifest(SignedManifestInput {
            profile: &profile,
            keys: &keys,
            naddr: "naddr1test",
            channel_title: "Channel",
            channel_description: None,
            relays: &relays,
            blossom_servers: &blossom_servers,
            items: &items,
            published_at: "2026-06-17T00:00:00Z",
        })
        .unwrap();

        assert!(!manifest_json.contains("nostr_secret_key"));
        assert!(!manifest_json.contains("curator_secret_key"));
        assert!(!manifest_json.contains("privateKey"));
        assert!(!manifest_json.contains(local_path));
        assert!(manifest_json.contains("https://blossom.example/"));
    }

    #[test]
    fn signed_manifest_includes_text_posts_without_retrieval_refs() {
        let profile = PublisherProfile {
            id: "publisher-test".to_string(),
            display_name: "Test Teacher".to_string(),
            curator_public_key: general_purpose::STANDARD
                .encode(SigningKey::from_bytes(&[8_u8; 32]).verifying_key()),
            nostr_pubkey: nostr_pubkey_from_secret(&[9_u8; 32]).unwrap(),
            relays: vec![],
            blossom_servers: vec![],
            created_at: "2026-06-17T00:00:00Z".to_string(),
            updated_at: "2026-06-17T00:00:00Z".to_string(),
            vault_configured: true,
        };
        let keys = PublisherKeys {
            curator_secret_key: [8_u8; 32],
            nostr_secret_key: [9_u8; 32],
        };
        let post = PublishedPostDraft {
            title: "Class note".to_string(),
            body: "Read the next section before the live session.".to_string(),
        };
        let relays = [NostrRelayConfig {
            url: "wss://relay.example".to_string(),
        }];
        let blossom_servers = [BlossomServerConfig {
            url: "https://blossom.example".to_string(),
        }];
        let items = [channel_item_from_post(
            "channel-test",
            &post,
            "2026-06-17T00:00:00Z",
        )];
        let (manifest_json, _) = signed_channel_manifest(SignedManifestInput {
            profile: &profile,
            keys: &keys,
            naddr: "naddr1test",
            channel_title: "Channel",
            channel_description: None,
            relays: &relays,
            blossom_servers: &blossom_servers,
            items: &items,
            published_at: "2026-06-17T00:00:00Z",
        })
        .unwrap();
        let manifest_value: Value = serde_json::from_str(&manifest_json).unwrap();
        let lessons = manifest_value
            .get("lessons")
            .and_then(Value::as_array)
            .unwrap();
        let lesson = &lessons[0];
        let source_ref = lesson
            .get("sourceRefs")
            .and_then(Value::as_array)
            .and_then(|refs| refs.first())
            .unwrap();

        assert_eq!(lessons.len(), 1);
        assert_eq!(
            lesson.get("contentType").and_then(Value::as_str),
            Some("post")
        );
        assert_eq!(
            source_ref.get("platform").and_then(Value::as_str),
            Some("duroos-post")
        );
        assert!(lesson.get("retrievalRefs").is_none());
        assert!(lesson
            .get("description")
            .and_then(Value::as_str)
            .is_some_and(|description| description.contains("Read the next section")));
        assert!(lesson
            .get("contentHashes")
            .and_then(Value::as_array)
            .is_some_and(|hashes| hashes.len() == 1));
    }

    #[test]
    fn blossom_upload_uses_hash_addressing_and_bud11_auth() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr());
        let (tx, rx) = mpsc::channel();
        let server_thread = thread::spawn(move || {
            let mut request = server.recv().unwrap();
            let method = request.method().as_str().to_string();
            let path = request.url().to_string();
            let headers = request
                .headers()
                .iter()
                .map(|header| {
                    (
                        header.field.as_str().to_string(),
                        header.value.as_str().to_string(),
                    )
                })
                .collect::<Vec<_>>();
            let mut body = Vec::new();
            request.as_reader().read_to_end(&mut body).unwrap();
            request.respond(Response::from_string("ok")).unwrap();
            tx.send((method, path, headers, body)).unwrap();
        });
        let data = b"lesson body".to_vec();
        let hash = sha256_hex(&data);
        let keys = PublisherKeys {
            curator_secret_key: [6_u8; 32],
            nostr_secret_key: [7_u8; 32],
        };
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        let result = upload_blob_to_servers(
            &client,
            &data,
            "application/pdf",
            "pdf",
            &[BlossomServerConfig { url: url.clone() }],
            &keys,
        );
        server_thread.join().unwrap();
        let (method, path, headers, body) = rx.recv().unwrap();

        assert_eq!(method, "PUT");
        assert_eq!(path, "/upload");
        assert_eq!(body, data);
        assert_eq!(result.first_url, Some(format!("{url}/{hash}.pdf")));
        assert_eq!(result.urls, vec![format!("{url}/{hash}.pdf")]);
        assert!(result.results.first().is_some_and(|result| result.uploaded));
        assert!(headers
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("X-SHA-256") && value == &hash));
        assert!(headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("Authorization") && value.starts_with("Nostr ")
        }));
        let auth_header = headers
            .iter()
            .find_map(|(name, value)| {
                name.eq_ignore_ascii_case("Authorization")
                    .then_some(value.strip_prefix("Nostr ").unwrap_or(value))
            })
            .unwrap();
        let auth_json = general_purpose::URL_SAFE_NO_PAD
            .decode(auth_header)
            .unwrap();
        let auth_event: NostrEvent = serde_json::from_slice(&auth_json).unwrap();
        let auth_id_material = serde_json::to_string(&json!([
            0,
            auth_event.pubkey,
            auth_event.created_at,
            auth_event.kind,
            auth_event.tags,
            auth_event.content
        ]))
        .unwrap();
        let auth_id_bytes = Sha256::digest(auth_id_material.as_bytes());
        assert_eq!(auth_event.id, hex_lower(&auth_id_bytes));
        assert_eq!(auth_event.kind, BLOSSOM_AUTH_KIND);
        assert!(auth_event.tags.iter().any(|tag| {
            tag.first().map(String::as_str) == Some("t")
                && tag.get(1).map(String::as_str) == Some("upload")
        }));
        assert!(auth_event.tags.iter().any(|tag| {
            tag.first().map(String::as_str) == Some("x")
                && tag.get(1).map(String::as_str) == Some(hash.as_str())
        }));
        let secp = Secp256k1::new();
        let signature = SchnorrSignature::from_str(&auth_event.sig).unwrap();
        let message = SecpMessage::from_digest_slice(&auth_id_bytes).unwrap();
        let pubkey_bytes = decode_hex_32(&auth_event.pubkey, "Nostr pubkey").unwrap();
        let pubkey = XOnlyPublicKey::from_slice(&pubkey_bytes).unwrap();
        secp.verify_schnorr(&signature, &message, &pubkey).unwrap();
    }

    #[test]
    fn archive_manual_mirror_must_hash_match_before_advertising() {
        let manifest_json =
            "{\"schemaVersion\":2,\"collection\":{\"title\":\"Archive\"},\"lessons\":[]}";
        let manifest_hash = sha256_hex(manifest_json.as_bytes());
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr());
        let bad_url = format!("{base_url}/wrong.json");
        let good_url = format!("{base_url}/manifest.json");
        let manifest_body = manifest_json.to_string();
        let server_thread = thread::spawn(move || {
            for _ in 0..2 {
                let request = server.recv().unwrap();
                if request.url() == "/manifest.json" {
                    request
                        .respond(Response::from_string(manifest_body.clone()))
                        .unwrap();
                } else {
                    request.respond(Response::from_string("wrong")).unwrap();
                }
            }
        });
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        let results = publish_archive_mirrors(
            &client,
            manifest_json.as_bytes(),
            &manifest_hash,
            &[
                ArchiveMirrorConfig {
                    service: "https".to_string(),
                    url: bad_url.clone(),
                    gateway_url: None,
                    label: None,
                },
                ArchiveMirrorConfig {
                    service: "arweave".to_string(),
                    url: good_url.clone(),
                    gateway_url: None,
                    label: None,
                },
            ],
        );
        server_thread.join().unwrap();

        assert_eq!(results.len(), 2);
        assert!(!results[0].verified);
        assert_eq!(results[0].url, Some(bad_url));
        assert!(results[0].message.contains("hash mismatch"));
        assert!(results[1].verified);
        assert_eq!(results[1].url, Some(good_url));
        assert_eq!(results[1].service, "arweave");
    }

    #[test]
    fn ipfs_archive_upload_verifies_gateway_before_advertising() {
        let manifest_json =
            "{\"schemaVersion\":2,\"collection\":{\"title\":\"IPFS\"},\"lessons\":[]}";
        let manifest_hash = sha256_hex(manifest_json.as_bytes());
        let cid = "bafyduroosmanifest";
        let api_server = Server::http("127.0.0.1:0").unwrap();
        let api_url = format!("http://{}", api_server.server_addr());
        let api_thread = thread::spawn(move || {
            let mut request = api_server.recv().unwrap();
            assert!(request.url().starts_with("/api/v0/add"));
            let mut body = Vec::new();
            request.as_reader().read_to_end(&mut body).unwrap();
            assert!(!body.is_empty());
            request
                .respond(Response::from_string(json!({ "Hash": cid }).to_string()))
                .unwrap();
        });
        let gateway_server = Server::http("127.0.0.1:0").unwrap();
        let gateway_url = format!("http://{}/ipfs", gateway_server.server_addr());
        let manifest_body = manifest_json.to_string();
        let gateway_thread = thread::spawn(move || {
            let request = gateway_server.recv().unwrap();
            assert_eq!(request.url(), "/ipfs/bafyduroosmanifest");
            request
                .respond(Response::from_string(manifest_body.clone()))
                .unwrap();
        });
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        let results = publish_archive_mirrors(
            &client,
            manifest_json.as_bytes(),
            &manifest_hash,
            &[ArchiveMirrorConfig {
                service: "ipfs-http-api".to_string(),
                url: api_url,
                gateway_url: Some(gateway_url.clone()),
                label: None,
            }],
        );
        api_thread.join().unwrap();
        gateway_thread.join().unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].archived);
        assert!(results[0].verified);
        assert_eq!(results[0].cid, Some(cid.to_string()));
        assert_eq!(
            results[0].url,
            Some(format!("{gateway_url}/bafyduroosmanifest"))
        );
    }

    #[test]
    fn nostr_channel_resolution_falls_back_to_verified_manifest_mirror() {
        let manifest_json =
            "{\"schemaVersion\":2,\"collection\":{\"title\":\"Mirror\"},\"lessons\":[]}";
        let manifest_hash = sha256_hex(manifest_json.as_bytes());
        let http_server = Server::http("127.0.0.1:0").unwrap();
        let http_base = format!("http://{}", http_server.server_addr());
        let bad_manifest_url = format!("{http_base}/missing.json");
        let good_manifest_url = format!("{http_base}/manifest.json");
        let manifest_body = manifest_json.to_string();
        let http_thread = thread::spawn(move || {
            for _ in 0..2 {
                let request = http_server.recv().unwrap();
                if request.url() == "/manifest.json" {
                    request
                        .respond(Response::from_string(manifest_body.clone()))
                        .unwrap();
                } else {
                    request
                        .respond(Response::from_string("missing").with_status_code(404))
                        .unwrap();
                }
            }
        });

        let secret = [9_u8; 32];
        let identifier = "duroos-channel:mirror-test";
        let event = signed_nostr_event(
            &secret,
            DUROOS_CHANNEL_KIND,
            vec![vec!["d".to_string(), identifier.to_string()]],
            json!({
                "manifestUrl": bad_manifest_url,
                "manifestUrls": [bad_manifest_url, good_manifest_url],
                "manifestSha256": format!("sha256:{manifest_hash}")
            })
            .to_string(),
        )
        .unwrap();
        let relay_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let relay_url = format!("ws://{}", relay_listener.local_addr().unwrap());
        let event_for_relay = event.clone();
        let relay_thread = thread::spawn(move || {
            let (stream, _) = relay_listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let request = socket.read().unwrap().to_text().unwrap().to_string();
            assert!(request.contains("\"REQ\""));
            socket
                .send(WsMessage::Text(
                    json!(["EVENT", "duroos-channel", event_for_relay]).to_string(),
                ))
                .unwrap();
            socket
                .send(WsMessage::Text(
                    json!(["EOSE", "duroos-channel"]).to_string(),
                ))
                .unwrap();
        });
        let naddr = encode_naddr(
            identifier,
            &event.pubkey,
            DUROOS_CHANNEL_KIND as u32,
            std::slice::from_ref(&relay_url),
        )
        .unwrap();

        let resolved = resolve_nostr_channel_manifest_url(&naddr).unwrap();
        relay_thread.join().unwrap();
        http_thread.join().unwrap();

        assert_eq!(resolved.manifest_url, good_manifest_url);
        assert_eq!(resolved.manifest_urls.len(), 2);
        assert_eq!(resolved.manifest_sha256, format!("sha256:{manifest_hash}"));
    }

    #[test]
    fn nostr_channel_resolution_uses_verified_archive_mirror_fallback() {
        let manifest_json =
            "{\"schemaVersion\":2,\"collection\":{\"title\":\"Archive Mirror\"},\"lessons\":[]}";
        let manifest_hash = sha256_hex(manifest_json.as_bytes());
        let http_server = Server::http("127.0.0.1:0").unwrap();
        let http_base = format!("http://{}", http_server.server_addr());
        let bad_manifest_url = format!("{http_base}/missing.json");
        let archive_manifest_url = format!("{http_base}/ipfs/bafyduroos");
        let manifest_body = manifest_json.to_string();
        let http_thread = thread::spawn(move || {
            for _ in 0..2 {
                let request = http_server.recv().unwrap();
                if request.url() == "/ipfs/bafyduroos" {
                    request
                        .respond(Response::from_string(manifest_body.clone()))
                        .unwrap();
                } else {
                    request
                        .respond(Response::from_string("missing").with_status_code(404))
                        .unwrap();
                }
            }
        });

        let secret = [10_u8; 32];
        let identifier = "duroos-channel:archive-test";
        let event = signed_nostr_event(
            &secret,
            DUROOS_CHANNEL_KIND,
            vec![vec!["d".to_string(), identifier.to_string()]],
            json!({
                "manifestUrl": bad_manifest_url,
                "archiveMirrors": [{
                    "service": "ipfs-gateway",
                    "url": archive_manifest_url,
                    "cid": "bafyduroos",
                    "public": true,
                    "permanent": true
                }],
                "manifestSha256": format!("sha256:{manifest_hash}")
            })
            .to_string(),
        )
        .unwrap();
        let relay_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let relay_url = format!("ws://{}", relay_listener.local_addr().unwrap());
        let event_for_relay = event.clone();
        let relay_thread = thread::spawn(move || {
            let (stream, _) = relay_listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let request = socket.read().unwrap().to_text().unwrap().to_string();
            assert!(request.contains("\"REQ\""));
            socket
                .send(WsMessage::Text(
                    json!(["EVENT", "duroos-channel", event_for_relay]).to_string(),
                ))
                .unwrap();
            socket
                .send(WsMessage::Text(
                    json!(["EOSE", "duroos-channel"]).to_string(),
                ))
                .unwrap();
        });
        let naddr = encode_naddr(
            identifier,
            &event.pubkey,
            DUROOS_CHANNEL_KIND as u32,
            std::slice::from_ref(&relay_url),
        )
        .unwrap();

        let resolved = resolve_nostr_channel_manifest_url(&naddr).unwrap();
        relay_thread.join().unwrap();
        http_thread.join().unwrap();

        assert_eq!(resolved.manifest_url, archive_manifest_url);
        assert_eq!(resolved.manifest_urls.len(), 2);
        assert_eq!(resolved.archive_mirrors, vec![archive_manifest_url]);
        assert_eq!(resolved.manifest_sha256, format!("sha256:{manifest_hash}"));
    }

    #[test]
    fn nostr_relay_publish_and_fetch_use_addressable_channel_events() {
        let secret = [8_u8; 32];
        let identifier = "duroos-channel:test";
        let event = signed_nostr_event(
            &secret,
            DUROOS_CHANNEL_KIND,
            vec![vec!["d".to_string(), identifier.to_string()]],
            json!({
                "manifestUrl": "https://blossom.example/manifest.json",
                "manifestSha256": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            })
            .to_string(),
        )
        .unwrap();

        let publish_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let publish_url = format!("ws://{}", publish_listener.local_addr().unwrap());
        let publish_event_id = event.id.clone();
        let publish_thread = thread::spawn(move || {
            let (stream, _) = publish_listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let message = socket.read().unwrap().to_text().unwrap().to_string();
            assert!(message.contains("\"EVENT\""));
            assert!(message.contains(&publish_event_id));
            socket
                .send(WsMessage::Text(
                    json!(["OK", publish_event_id, true, "stored"]).to_string(),
                ))
                .unwrap();
        });

        let relay_results =
            publish_event_to_relays(&event, &[NostrRelayConfig { url: publish_url }]);
        publish_thread.join().unwrap();
        assert_eq!(relay_results.len(), 1);
        assert!(relay_results[0].accepted);

        let fetch_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let fetch_url = format!("ws://{}", fetch_listener.local_addr().unwrap());
        let event_for_fetch = event.clone();
        let fetch_thread = thread::spawn(move || {
            let (stream, _) = fetch_listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let request = socket.read().unwrap().to_text().unwrap().to_string();
            assert!(request.contains("\"REQ\""));
            socket
                .send(WsMessage::Text(
                    json!(["EVENT", "duroos-channel", event_for_fetch]).to_string(),
                ))
                .unwrap();
            socket
                .send(WsMessage::Text(
                    json!(["EOSE", "duroos-channel"]).to_string(),
                ))
                .unwrap();
        });
        let parsed = ParsedNaddr {
            raw: "naddr1test".to_string(),
            identifier: identifier.to_string(),
            author: event.pubkey.clone(),
            kind: DUROOS_CHANNEL_KIND,
            relays: vec![fetch_url.clone()],
        };
        let fetched = fetch_channel_event(&fetch_url, &parsed).unwrap();
        fetch_thread.join().unwrap();

        assert_eq!(fetched.id, event.id);
        assert_eq!(fetched.kind, DUROOS_CHANNEL_KIND);
        assert!(fetched.tags.iter().any(|tag| {
            tag.first().map(String::as_str) == Some("d")
                && tag.get(1).map(String::as_str) == Some(identifier)
        }));
    }

    #[test]
    fn endpoint_probe_event_is_marked_as_test_only() {
        let profile = PublisherProfile {
            id: "publisher-test".to_string(),
            display_name: "Endpoint Teacher".to_string(),
            curator_public_key: general_purpose::STANDARD
                .encode(SigningKey::from_bytes(&[11_u8; 32]).verifying_key()),
            nostr_pubkey: nostr_pubkey_from_secret(&[12_u8; 32]).unwrap(),
            relays: vec![],
            blossom_servers: vec![],
            created_at: "2026-06-17T00:00:00Z".to_string(),
            updated_at: "2026-06-17T00:00:00Z".to_string(),
            vault_configured: true,
        };
        let keys = PublisherKeys {
            curator_secret_key: [11_u8; 32],
            nostr_secret_key: [12_u8; 32],
        };

        let event = endpoint_probe_event(
            &keys,
            &profile,
            &["https://blossom.example/probe.txt".to_string()],
        )
        .unwrap();

        assert_eq!(event.kind, DUROOS_CHANNEL_KIND);
        assert!(event
            .content
            .contains("\"type\":\"publisher-endpoint-test\""));
        assert!(event.tags.iter().any(|tag| {
            tag.first().map(String::as_str) == Some("t")
                && tag.get(1).map(String::as_str) == Some("duroos-endpoint-test")
        }));
        assert!(event.tags.iter().any(|tag| {
            tag.first().map(String::as_str) == Some("r")
                && tag.get(1).map(String::as_str) == Some("https://blossom.example/probe.txt")
        }));
    }
}
