use crate::models::{ManifestCuratorIdentity, ManifestValidationReport};
use base64::{engine::general_purpose, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;
use std::collections::BTreeMap;

const CURRENT_SCHEMA_VERSION: i64 = 2;
const FORBIDDEN_KEY_TOKENS: &[&str] = &[
    "credential",
    "credentials",
    "token",
    "cookie",
    "cookies",
    "session",
    "secret",
    "password",
    "command",
    "script",
    "hook",
];
const FORBIDDEN_NORMALIZED_KEYS: &[&str] = &[
    "accesstoken",
    "refreshtoken",
    "telegramsession",
    "apikey",
    "privatekey",
    "localpath",
    "absolutepath",
];

pub fn validate_collection_manifest(manifest_json: &str) -> ManifestValidationReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let parsed = match serde_json::from_str::<Value>(manifest_json) {
        Ok(value) => value,
        Err(_) => {
            return ManifestValidationReport {
                valid: false,
                errors: vec!["Manifest is not valid JSON".to_string()],
                warnings,
                trust_state: None,
                curator: None,
                trusted_curator_id: None,
            };
        }
    };

    walk_for_unsafe_values(&parsed, "manifest", &mut errors);

    let Some(root) = parsed.as_object() else {
        errors.push("Manifest root must be an object".to_string());
        return report(errors, warnings, None, None, None);
    };
    let curator = curator_identity(&parsed);

    let schema_version = root.get("schemaVersion").and_then(Value::as_i64);
    if !matches!(schema_version, Some(1) | Some(CURRENT_SCHEMA_VERSION)) {
        errors.push(format!(
            "schemaVersion must be 1 or {CURRENT_SCHEMA_VERSION}"
        ));
    }

    if root
        .get("exportedAt")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        errors.push("exportedAt is required".to_string());
    }

    if schema_version == Some(CURRENT_SCHEMA_VERSION) {
        validate_curator(root.get("curator"), &mut errors);
    }

    if let Some(publication) = root.get("publication") {
        validate_publication(publication, &mut errors);
    }

    match root.get("collection").and_then(Value::as_object) {
        Some(collection) => {
            require_string(collection.get("title"), "collection.title", &mut errors);
            require_string(
                collection.get("ownerLabel"),
                "collection.ownerLabel",
                &mut errors,
            );
        }
        None => errors.push("collection must be an object".to_string()),
    }

    match root.get("lessons").and_then(Value::as_array) {
        Some(lessons) if !lessons.is_empty() => {
            for (index, lesson) in lessons.iter().enumerate() {
                validate_lesson(index, lesson, &mut errors, &mut warnings);
            }
        }
        _ => errors.push("lessons must contain at least one lesson".to_string()),
    }

    let trust_state = match verify_manifest_signature(&parsed) {
        SignatureCheck::Unsigned => {
            if schema_version == Some(CURRENT_SCHEMA_VERSION) {
                warnings.push("Manifest is unsigned; review before importing.".to_string());
            }
            "unsigned".to_string()
        }
        SignatureCheck::Valid => "signed-untrusted".to_string(),
        SignatureCheck::Invalid(error) => {
            errors.push(error);
            "tampered".to_string()
        }
    };

    report(errors, warnings, Some(trust_state), curator, None)
}

fn validate_curator(value: Option<&Value>, errors: &mut Vec<String>) {
    let Some(curator) = value.and_then(Value::as_object) else {
        errors.push("curator must be an object for schemaVersion 2".to_string());
        return;
    };

    require_string(curator.get("id"), "curator.id", errors);
    require_string(curator.get("displayName"), "curator.displayName", errors);

    let public_key = curator
        .get("publicKey")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if decode_key_or_signature(public_key, 32).is_err() {
        errors.push("curator.publicKey must be an Ed25519 public key".to_string());
    }

    if let Some(nostr_pubkey) = curator.get("nostrPubkey").and_then(Value::as_str) {
        if !looks_like_hex_bytes(nostr_pubkey, 32) {
            errors.push("curator.nostrPubkey must be a 32-byte hex Nostr public key".to_string());
        }
    }
}

fn validate_publication(value: &Value, errors: &mut Vec<String>) {
    let Some(publication) = value.as_object() else {
        errors.push("publication must be an object".to_string());
        return;
    };

    if publication
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or_default()
        != "nostr"
    {
        errors.push("publication.transport must be nostr".to_string());
    }

    require_string(publication.get("naddr"), "publication.naddr", errors);
    require_string(
        publication.get("manifestSha256"),
        "publication.manifestSha256",
        errors,
    );
    require_string(
        publication.get("publishedAt"),
        "publication.publishedAt",
        errors,
    );

    if !publication
        .get("relays")
        .and_then(Value::as_array)
        .is_some_and(|relays| !relays.is_empty() && relays.iter().all(is_safe_ws_url))
    {
        errors.push("publication.relays must contain websocket relay URLs".to_string());
    }

    if !publication
        .get("blossomServers")
        .and_then(Value::as_array)
        .is_some_and(|servers| !servers.is_empty() && servers.iter().all(is_safe_http_value))
    {
        errors.push("publication.blossomServers must contain http or https URLs".to_string());
    }

    if let Some(hash) = publication.get("manifestSha256").and_then(Value::as_str) {
        if !looks_like_sha256(hash) {
            errors.push("publication.manifestSha256 must be a sha256 hash".to_string());
        }
    }
}

fn curator_identity(value: &Value) -> Option<ManifestCuratorIdentity> {
    let curator = value.get("curator")?.as_object()?;
    Some(ManifestCuratorIdentity {
        id: curator.get("id")?.as_str()?.to_string(),
        display_name: curator.get("displayName")?.as_str()?.to_string(),
        public_key: curator.get("publicKey")?.as_str()?.to_string(),
    })
}

fn validate_lesson(
    index: usize,
    lesson: &Value,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let prefix = format!("lessons[{index}]");
    let Some(lesson_object) = lesson.as_object() else {
        errors.push(format!("{prefix} must be an object"));
        return;
    };

    require_string(
        lesson_object.get("title"),
        &format!("{prefix}.title"),
        errors,
    );

    if let Some(content_type) = lesson_object.get("contentType").and_then(Value::as_str) {
        if !matches!(content_type, "video" | "audio" | "pdf" | "post") {
            errors.push(format!(
                "{prefix}.contentType must be video, audio, pdf, or post"
            ));
        }
    }

    match lesson_object.get("sourceRefs").and_then(Value::as_array) {
        Some(source_refs) if !source_refs.is_empty() => {
            for (source_index, source_ref) in source_refs.iter().enumerate() {
                let source_prefix = format!("{prefix}.sourceRefs[{source_index}]");
                let Some(source_object) = source_ref.as_object() else {
                    errors.push(format!("{source_prefix} must be an object"));
                    continue;
                };

                require_string(
                    source_object.get("platform"),
                    &format!("{source_prefix}.platform"),
                    errors,
                );

                let origin_url = source_object
                    .get("originUrl")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !is_safe_source_url(origin_url) {
                    errors.push(format!(
                        "{source_prefix}.originUrl must be a safe source URL"
                    ));
                }
            }
        }
        _ => errors.push(format!(
            "{prefix}.sourceRefs must contain at least one source"
        )),
    }

    if let Some(retrieval_refs) = lesson_object.get("retrievalRefs") {
        validate_retrieval_refs(&prefix, retrieval_refs, errors);
    }

    match lesson_object.get("contentHashes").and_then(Value::as_array) {
        Some(content_hashes) if content_hashes.is_empty() => {
            warnings.push(format!(
                "{prefix}.contentHashes is empty; downloads cannot be verified"
            ));
        }
        Some(content_hashes) => {
            for (hash_index, hash) in content_hashes.iter().enumerate() {
                let hash = hash.as_str().unwrap_or_default();
                if !looks_like_sha256(hash) {
                    errors.push(format!(
                        "{prefix}.contentHashes[{hash_index}] must be a sha256 hash"
                    ));
                }
            }
        }
        None => errors.push(format!("{prefix}.contentHashes must be an array")),
    }

    match lesson_object.get("provenance").and_then(Value::as_object) {
        Some(provenance) => {
            require_string(
                provenance.get("adapterName"),
                &format!("{prefix}.provenance.adapterName"),
                errors,
            );
        }
        None => errors.push(format!("{prefix}.provenance is required")),
    }
}

fn validate_retrieval_refs(prefix: &str, value: &Value, errors: &mut Vec<String>) {
    let Some(retrieval_refs) = value.as_array() else {
        errors.push(format!("{prefix}.retrievalRefs must be an array"));
        return;
    };

    for (index, retrieval_ref) in retrieval_refs.iter().enumerate() {
        let ref_prefix = format!("{prefix}.retrievalRefs[{index}]");
        let Some(object) = retrieval_ref.as_object() else {
            errors.push(format!("{ref_prefix} must be an object"));
            continue;
        };
        let kind = object
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match kind {
            "direct-url" | "enclosure-url" => {
                let url = object
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !is_safe_http_url(url) {
                    errors.push(format!("{ref_prefix}.url must be an http or https URL"));
                }

                if let Some(service) = object.get("service").and_then(Value::as_str) {
                    if service != "blossom" {
                        errors.push(format!("{ref_prefix}.service is not supported"));
                    }
                }

                if let Some(hash) = object.get("sha256").and_then(Value::as_str) {
                    if !looks_like_sha256(hash) {
                        errors.push(format!("{ref_prefix}.sha256 must be a sha256 hash"));
                    }
                }

                if let Some(size_bytes) = object.get("sizeBytes").and_then(Value::as_i64) {
                    if size_bytes <= 0 {
                        errors.push(format!("{ref_prefix}.sizeBytes must be positive"));
                    }
                }

                if let Some(mime_type) = object.get("mimeType").and_then(Value::as_str) {
                    if mime_type.trim().is_empty() || mime_type.contains(['\r', '\n']) {
                        errors.push(format!("{ref_prefix}.mimeType must be a MIME type"));
                    }
                }
            }
            "ipfs-cid" => {
                let cid = object
                    .get("cid")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !looks_like_ipfs_cid(cid) {
                    errors.push(format!("{ref_prefix}.cid must be a valid IPFS CID"));
                }
            }
            "magnet" => {
                let magnet_uri = object
                    .get("magnetUri")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !magnet_uri.starts_with("magnet:?") {
                    errors.push(format!("{ref_prefix}.magnetUri must be a magnet URI"));
                }
            }
            _ => errors.push(format!("{ref_prefix}.kind is not supported")),
        }
    }
}

enum SignatureCheck {
    Unsigned,
    Valid,
    Invalid(String),
}

fn verify_manifest_signature(value: &Value) -> SignatureCheck {
    let Some(signature) = value.get("signature") else {
        return SignatureCheck::Unsigned;
    };
    let Some(signature_object) = signature.as_object() else {
        return SignatureCheck::Invalid("signature must be an object".to_string());
    };

    if signature_object
        .get("algorithm")
        .and_then(Value::as_str)
        .unwrap_or_default()
        != "ed25519"
    {
        return SignatureCheck::Invalid("signature.algorithm must be ed25519".to_string());
    }

    let public_key = signature_object
        .get("publicKey")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let signature_value = signature_object
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if let Some(curator_public_key) = value
        .get("curator")
        .and_then(|curator| curator.get("publicKey"))
        .and_then(Value::as_str)
    {
        if curator_public_key != public_key {
            return SignatureCheck::Invalid(
                "signature.publicKey must match curator.publicKey".to_string(),
            );
        }
    }

    let public_key_bytes = match decode_key_or_signature(public_key, 32) {
        Ok(bytes) => bytes,
        Err(error) => return SignatureCheck::Invalid(error),
    };
    let signature_bytes = match decode_key_or_signature(signature_value, 64) {
        Ok(bytes) => bytes,
        Err(error) => return SignatureCheck::Invalid(error),
    };

    let public_key_bytes: [u8; 32] = match public_key_bytes.try_into() {
        Ok(bytes) => bytes,
        Err(_) => {
            return SignatureCheck::Invalid("signature.publicKey must be 32 bytes".to_string())
        }
    };
    let signature_bytes: [u8; 64] = match signature_bytes.try_into() {
        Ok(bytes) => bytes,
        Err(_) => return SignatureCheck::Invalid("signature.value must be 64 bytes".to_string()),
    };
    let verifying_key = match VerifyingKey::from_bytes(&public_key_bytes) {
        Ok(key) => key,
        Err(_) => {
            return SignatureCheck::Invalid(
                "signature.publicKey must be a valid Ed25519 public key".to_string(),
            )
        }
    };
    let signature = Signature::from_bytes(&signature_bytes);
    let payload = match signed_payload(value) {
        Ok(payload) => payload,
        Err(error) => return SignatureCheck::Invalid(error),
    };

    if verifying_key.verify(payload.as_bytes(), &signature).is_ok() {
        SignatureCheck::Valid
    } else {
        SignatureCheck::Invalid("signature is invalid or manifest has been tampered".to_string())
    }
}

pub(crate) fn signed_payload(value: &Value) -> Result<String, String> {
    let mut payload = value.clone();
    let Some(object) = payload.as_object_mut() else {
        return Err("Manifest root must be an object".to_string());
    };
    object.remove("signature");
    canonical_json(&payload)
}

fn canonical_json(value: &Value) -> Result<String, String> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            serde_json::to_string(value).map_err(|error| error.to_string())
        }
        Value::Array(items) => {
            let parts = items
                .iter()
                .map(canonical_json)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("[{}]", parts.join(",")))
        }
        Value::Object(map) => {
            let mut sorted = BTreeMap::new();
            for (key, nested) in map {
                sorted.insert(key, nested);
            }

            let mut parts = Vec::with_capacity(sorted.len());
            for (key, nested) in sorted {
                let key_json = serde_json::to_string(key).map_err(|error| error.to_string())?;
                parts.push(format!("{key_json}:{}", canonical_json(nested)?));
            }
            Ok(format!("{{{}}}", parts.join(",")))
        }
    }
}

#[cfg(test)]
pub(crate) fn canonical_json_for_test(value: &Value) -> Result<String, String> {
    canonical_json(value)
}

fn decode_key_or_signature(value: &str, expected_len: usize) -> Result<Vec<u8>, String> {
    let trimmed = value.trim();
    if trimmed.len() == expected_len * 2
        && trimmed
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return decode_hex(trimmed);
    }

    for engine in [general_purpose::STANDARD, general_purpose::URL_SAFE_NO_PAD] {
        if let Ok(bytes) = engine.decode(trimmed) {
            if bytes.len() == expected_len {
                return Ok(bytes);
            }
        }
    }

    Err(if expected_len == 32 {
        "signature.publicKey must be an Ed25519 public key".to_string()
    } else {
        "signature.value must be an Ed25519 signature".to_string()
    })
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|chunk| {
            let text = std::str::from_utf8(chunk).map_err(|error| error.to_string())?;
            u8::from_str_radix(text, 16).map_err(|error| error.to_string())
        })
        .collect()
}

fn require_string(value: Option<&Value>, field: &str, errors: &mut Vec<String>) {
    if value
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        errors.push(format!("{field} is required"));
    }
}

fn walk_for_unsafe_values(value: &Value, path: &str, errors: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                walk_for_unsafe_values(item, &format!("{path}[{index}]"), errors);
            }
        }
        Value::Object(map) => {
            for (key, nested) in map {
                let nested_path = format!("{path}.{key}");
                if is_forbidden_manifest_key(key) {
                    errors.push(format!(
                        "{nested_path} is not allowed in shared collection manifests"
                    ));
                }
                walk_for_unsafe_values(nested, &nested_path, errors);
            }
        }
        Value::String(value)
            if !is_encoded_crypto_field(path) && is_absolute_or_file_path(value) =>
        {
            errors.push(format!("{path} contains an absolute or file URL path"));
        }
        _ => {}
    }
}

fn is_forbidden_manifest_key(key: &str) -> bool {
    let tokens = manifest_key_tokens(key);
    let normalized = tokens.join("");
    FORBIDDEN_NORMALIZED_KEYS
        .iter()
        .any(|forbidden| normalized == *forbidden)
        || tokens.iter().any(|token| {
            FORBIDDEN_KEY_TOKENS
                .iter()
                .any(|forbidden| token == forbidden)
        })
}

fn manifest_key_tokens(key: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut previous_was_lower_or_digit = false;

    for character in key.chars() {
        if !character.is_ascii_alphanumeric() {
            if !current.is_empty() {
                tokens.push(current.to_ascii_lowercase());
                current.clear();
            }
            previous_was_lower_or_digit = false;
            continue;
        }

        if character.is_ascii_uppercase() && previous_was_lower_or_digit && !current.is_empty() {
            tokens.push(current.to_ascii_lowercase());
            current.clear();
        }

        current.push(character);
        previous_was_lower_or_digit = character.is_ascii_lowercase() || character.is_ascii_digit();
    }

    if !current.is_empty() {
        tokens.push(current.to_ascii_lowercase());
    }

    tokens
}

fn is_encoded_crypto_field(path: &str) -> bool {
    path.ends_with(".publicKey") || path.ends_with(".signature.value")
}

fn is_safe_source_url(value: &str) -> bool {
    value.starts_with("https://")
        || value.starts_with("http://")
        || value.starts_with("tg://")
        || value.starts_with("telegram://")
        || value.starts_with("lbry://")
}

fn is_safe_http_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn is_safe_http_value(value: &Value) -> bool {
    value.as_str().is_some_and(is_safe_http_url)
}

fn is_safe_ws_url(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|url| url.starts_with("wss://") || url.starts_with("ws://"))
}

fn is_absolute_or_file_path(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with("~/")
        || value.starts_with("file://")
        || value.chars().take(3).collect::<String>().ends_with(":\\")
}

fn looks_like_sha256(value: &str) -> bool {
    let hash = value.strip_prefix("sha256:").unwrap_or(value);
    hash.len() == 64 && hash.chars().all(|character| character.is_ascii_hexdigit())
}

fn looks_like_hex_bytes(value: &str, byte_len: usize) -> bool {
    value.len() == byte_len * 2 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn looks_like_ipfs_cid(value: &str) -> bool {
    (value.starts_with("Qm") && value.len() == 46) || (value.starts_with('b') && value.len() >= 20)
}

fn report(
    errors: Vec<String>,
    warnings: Vec<String>,
    trust_state: Option<String>,
    curator: Option<ManifestCuratorIdentity>,
    trusted_curator_id: Option<String>,
) -> ManifestValidationReport {
    ManifestValidationReport {
        valid: errors.is_empty(),
        errors,
        warnings,
        trust_state,
        curator,
        trusted_curator_id,
    }
}

pub fn validate_ed25519_public_key(public_key: &str) -> Result<(), String> {
    decode_key_or_signature(public_key, 32).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;

    fn signed_manifest() -> Value {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
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
        let payload = canonical_json(&manifest).unwrap();
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

    #[test]
    fn accepts_signed_v2_manifest() {
        let manifest = signed_manifest();
        let report = validate_collection_manifest(&manifest.to_string());

        assert!(report.valid, "{:?}", report.errors);
        assert_eq!(report.trust_state.as_deref(), Some("signed-untrusted"));
    }

    #[test]
    fn rejects_tampered_signed_manifest() {
        let mut manifest = signed_manifest();
        manifest["lessons"][0]["title"] = json!("Changed lesson");

        let report = validate_collection_manifest(&manifest.to_string());

        assert!(!report.valid);
        assert_eq!(report.trust_state.as_deref(), Some("tampered"));
        assert!(report.errors.join(" ").contains("tampered"));
    }

    #[test]
    fn rejects_unsafe_retrieval_refs_and_secrets() {
        let manifest = json!({
            "schemaVersion": 2,
            "exportedAt": "2026-06-16T05:00:00Z",
            "curator": {
                "id": "curator",
                "displayName": "Curator",
                "publicKey": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            },
            "telegramSession": "secret",
            "collection": {
                "title": "Unsafe",
                "ownerLabel": "Curator"
            },
            "lessons": [
                {
                    "title": "Unsafe lesson",
                    "sourceRefs": [{"platform": "local", "originUrl": "file:///tmp/lesson.mp4"}],
                    "retrievalRefs": [{"kind": "direct-url", "url": "file:///tmp/lesson.mp4"}],
                    "contentHashes": [],
                    "provenance": {"adapterName": "DuroosManifestAdapter"}
                }
            ]
        });

        let report = validate_collection_manifest(&manifest.to_string());

        assert!(!report.valid);
        let errors = report.errors.join(" ");
        assert!(errors.contains("telegramSession"));
        assert!(errors.contains("originUrl"));
        assert!(errors.contains("retrievalRefs"));
    }

    #[test]
    fn accepts_description_without_treating_it_as_script() {
        let manifest = json!({
            "schemaVersion": 2,
            "exportedAt": "2026-06-16T05:00:00Z",
            "curator": {
                "id": "curator",
                "displayName": "Curator",
                "publicKey": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            },
            "collection": {
                "title": "Safe descriptions",
                "ownerLabel": "Curator",
                "description": "Weekly lessons from the teacher."
            },
            "lessons": [
                {
                    "title": "Opening lesson",
                    "description": "Recorded class notes.",
                    "contentType": "video",
                    "sourceRefs": [{"platform": "youtube", "originUrl": "https://youtube.com/watch?v=abc123"}],
                    "contentHashes": [],
                    "provenance": {"adapterName": "DuroosManifestAdapter"}
                }
            ]
        });

        let report = validate_collection_manifest(&manifest.to_string());

        assert!(report.valid, "{:?}", report.errors);
    }

    #[test]
    fn accepts_nostr_publication_and_blossom_refs() {
        let manifest = json!({
            "schemaVersion": 2,
            "exportedAt": "2026-06-16T05:00:00Z",
            "curator": {
                "id": "curator",
                "displayName": "Curator",
                "publicKey": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "nostrPubkey": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            },
            "publication": {
                "transport": "nostr",
                "naddr": "naddr1example",
                "relays": ["wss://relay.example"],
                "blossomServers": ["https://blossom.example"],
                "manifestSha256": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "publishedAt": "2026-06-16T05:00:00Z"
            },
            "collection": {
                "title": "Federated collection",
                "ownerLabel": "Curator"
            },
            "lessons": [
                {
                    "title": "Blossom lesson",
                    "contentType": "video",
                    "sourceRefs": [{"platform": "blossom", "originUrl": "https://blossom.example/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.mp4"}],
                    "retrievalRefs": [{
                        "kind": "direct-url",
                        "url": "https://blossom.example/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.mp4",
                        "service": "blossom",
                        "sha256": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                        "sizeBytes": 2048,
                        "mimeType": "video/mp4",
                        "mediaType": "video/mp4"
                    }],
                    "contentHashes": ["sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"],
                    "provenance": {"adapterName": "DuroosFederatedPublisher"}
                }
            ]
        });

        let report = validate_collection_manifest(&manifest.to_string());

        assert!(report.valid, "{:?}", report.errors);
    }

    #[test]
    fn rejects_invalid_nostr_publication() {
        let manifest = json!({
            "schemaVersion": 2,
            "exportedAt": "2026-06-16T05:00:00Z",
            "curator": {
                "id": "curator",
                "displayName": "Curator",
                "publicKey": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "nostrPubkey": "not-a-key"
            },
            "publication": {
                "transport": "nostr",
                "naddr": "naddr1example",
                "relays": ["https://relay.example"],
                "blossomServers": ["file:///tmp/blossom"],
                "manifestSha256": "sha256:not-a-hash",
                "publishedAt": "2026-06-16T05:00:00Z"
            },
            "collection": {
                "title": "Federated collection",
                "ownerLabel": "Curator"
            },
            "lessons": [
                {
                    "title": "Blossom lesson",
                    "sourceRefs": [{"platform": "blossom", "originUrl": "https://blossom.example/file.mp4"}],
                    "contentHashes": [],
                    "provenance": {"adapterName": "DuroosFederatedPublisher"}
                }
            ]
        });

        let report = validate_collection_manifest(&manifest.to_string());
        let errors = report.errors.join(" ");

        assert!(!report.valid);
        assert!(errors.contains("nostrPubkey"));
        assert!(errors.contains("publication.relays"));
        assert!(errors.contains("publication.blossomServers"));
    }

    #[test]
    fn preserves_v1_manifest_compatibility() {
        let manifest = json!({
            "schemaVersion": 1,
            "exportedAt": "2026-06-16T05:00:00Z",
            "collection": {
                "title": "Legacy collection",
                "ownerLabel": "Local"
            },
            "lessons": [
                {
                    "title": "Legacy lesson",
                    "sourceRefs": [{"platform": "rss-feed", "originUrl": "https://example.org/lesson"}],
                    "contentHashes": [],
                    "provenance": {"adapterName": "FeedAdapter"}
                }
            ]
        });

        let report = validate_collection_manifest(&manifest.to_string());

        assert!(report.valid, "{:?}", report.errors);
        assert_eq!(report.trust_state.as_deref(), Some("unsigned"));
    }

    #[test]
    fn accepts_lbry_source_refs_without_retrieval_download_support() {
        let manifest = json!({
            "schemaVersion": 1,
            "exportedAt": "2026-06-16T05:00:00Z",
            "collection": {
                "title": "Odysee references",
                "ownerLabel": "Local"
            },
            "lessons": [
                {
                    "title": "Odysee class",
                    "sourceRefs": [{"platform": "odysee", "originUrl": "lbry://@teacher/class-1"}],
                    "contentHashes": [],
                    "provenance": {"adapterName": "OdyseeReferenceAdapter"}
                }
            ]
        });

        let report = validate_collection_manifest(&manifest.to_string());

        assert!(report.valid, "{:?}", report.errors);
    }
}
