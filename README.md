# Duroos Watcher

Duroos Watcher is a local-first desktop study library for educational content you save, follow, or subscribe to. It is designed as a focused YouTube-like shell without social feeds, comments, recommendations, public accounts, or telemetry.

## Current V1 Scope

- Tauri v2 + React + TypeScript + Vite shell.
- App-managed media library design with SQLite schema in the Rust backend.
- Source adapter registry for Local Files, Telegram, RSS/Atom feeds, Archive.org, YouTube, X, Rumble, Odysee, and signed teacher channels with truthful capability labels.
- User-selected feed subscriptions for dashboard-style updates from custom RSS, Atom, and JSON Feed feeds.
- Channel subscriptions for signed Duroos manifests with source refs, sha256 hashes, and downloadable media enclosures.
- Federated teacher publishing from the desktop app through user-configured Nostr relays and Blossom media servers.
- Live lesson tracking for provider-hosted sessions that can become downloadable archive entries after teacher approval.
- Public Telegram channel preview ingest without sign-in when Telegram exposes a `t.me/s` page.
- RSS/Atom/JSON Feed ingest for videos, audio enclosures, PDFs, and post/message entries.
- Duroos v2 manifest validation with curator identity, safe retrieval refs, and Ed25519 tamper checks in the Rust backend.
- Archive.org item ingest through the official metadata API for listed video, audio, and PDF files.
- Direct user-added YouTube, Rumble, and Odysee video URLs create reviewable library rows; X URLs are saved as credential-bound post references.
- Local import for video, audio, and PDF study files.
- Duplicate local imports are skipped by matching stored content hashes; source ingests skip matching source URLs, matching hashes, and same-title/same-duration feed duplicates.
- Added-source management with clear/delete controls separate from the platform capability matrix.
- Local media downloads for added sources through direct HTTP or `yt-dlp` when the lesson URL is supported by `yt-dlp`.
- Downloaded media is sha256-verified when a source provides a sha256 hash; mismatches are rejected before the file is attached to a lesson.
- Temporary Watch on Phone sharing for downloaded/imported audio and video through same-Wi-Fi VLC playlist links.
- Shared collection manifest validation rejects credentials, local absolute paths, command hooks, and unsafe file paths.
- Offline-friendly dashboard, library search, source capability matrix, import drawer, update queue, and player surface.
- AGPL-3.0-or-later open-source license posture.

## Decentralized Local-First Architecture

Duroos Watcher does not use blockchain in v1. The useful primitives are signed metadata, local storage, source provenance, and optional open feed transports.

- **Local-first shell:** no accounts, no telemetry, no central Duroos server, no automatic sharing of subscriptions, watch state, manifests, or media.
- **Public curator channels:** users subscribe to RSS, Atom, JSON Feed, Duroos manifest URLs, or shared Nostr `naddr` channel links chosen by the user.
- **Signed manifests:** Duroos v2 manifests include curator identity, optional Nostr pubkey binding, source refs, optional retrieval refs, sha256 hashes, and Ed25519 signatures. A valid signature proves tamper resistance for that public key, not automatic trust.
- **Review-first media:** users download locally before viewing. Auto-download and redistribution are not enabled by default.
- **Federated publishing:** teacher publisher profiles keep signing keys in a passphrase-encrypted local vault. Nostr relays carry signed channel announcements with all successful manifest mirror URLs, and Blossom servers store hash-addressed media and manifest blobs. Optional archive mirrors can pin the signed manifest through a teacher-configured local IPFS HTTP API or teacher-supplied public gateway URLs; Duroos announces only archive copies that SHA-256 match the signed manifest. Duroos ships editable third-party starter presets for Nostr and Blossom, but does not operate those endpoints and still has no accounts or central catalog.
- **Future layers:** IPFS CID and BitTorrent magnet refs are accepted only as explicit manifest retrieval references for content the curator marks as redistributable; they are not default media transports.

## Channels And Live Lessons

Teachers and curators are modeled as channel owners. A channel can publish uploaded classes, source provenance, media hashes, and enclosure URLs that subscribers review before downloading into their local library. The protocol design is intentionally feed-like so it can work without platform accounts by default.

Teachers can also publish directly from the desktop app by creating a publisher profile, using editable third-party Nostr/Blossom starter presets or their own endpoints, selecting local video/audio/PDF lessons, and sharing the resulting `naddr` channel link. The app requires one accepting Nostr relay and one uploading Blossom server before publishing. Teachers may also add public archive manifest mirrors, including a local IPFS API plus explicit gateway URL, for durability; archive failures do not publish unsafe links. Learners paste the channel link into Import while online; Duroos resolves the latest channel announcement, tries the advertised manifest mirrors until one hash-verifies, fetches the signed manifest, and keeps media downloads review-first.

Live lessons are provider-specific:

- YouTube Live can be tracked through the official live streaming API when the teacher configures API access.
- Mixlr recordings can be imported or uploaded into a channel after the event; open API automation is not assumed.
- A private RTMP host is the cleaner long-term path for teacher-hosted live lessons and automatic archives.

## Development

```bash
npm install
npm run dev
npm run test
npm run build
```

Tauri desktop commands require Rust and Cargo:

```bash
npm run tauri dev
npm run tauri:build:app
```

If `cargo` is missing, install Rust from <https://www.rust-lang.org/tools/install> before running Tauri.

Platform setup:

- macOS: install Xcode Command Line Tools, Rust, Node 22, and optional media tools with Homebrew (`yt-dlp`, `ffmpeg`, `mpv`, or VLC).
- Windows: install Rust MSVC, Node 22, WebView2 Runtime, and optional `yt-dlp.exe`, FFmpeg, VLC, or mpv on `PATH`.
- Linux: install Rust, Node 22, WebKitGTK 4.1 development packages, Ayatana AppIndicator, librsvg, and optional `yt-dlp`, FFmpeg, VLC, mpv, or ffplay.

Downloading source media uses direct HTTP for feed enclosures such as audio, video, and PDFs when
available. Platform video pages use `yt-dlp`; the app checks common install paths plus
`python3 -m yt_dlp` and surfaces readiness in the UI. When a subscribed feed or manifest provides a
sha256 hash, the app verifies the downloaded file before marking it ready.

Keep `yt-dlp` current. On macOS with Homebrew:

```bash
brew upgrade yt-dlp
yt-dlp --version
```

On Windows and Linux, use the package manager or pinned binary source you will use for release
builds, then verify `yt-dlp --version` from the same shell that launches Tauri.

Archive.org item URLs such as `https://archive.org/details/<identifier>` are expanded through
`https://archive.org/metadata/<identifier>`, then supported files are downloaded from direct
`archive.org/download` links.

Rumble and Odysee are treated as best-effort direct URL sources in v1. The app does not assume a
broad official catalog API for either platform; it creates local source rows from user-provided URLs
and uses local tooling only when the user starts a download.

## Watch On Phone

The desktop app can temporarily share ready audio and video files on the local Wi-Fi network for
playback in VLC on iOS or Android. Use **Watch on Phone** in the library dashboard, scan the QR code
with the phone, and open the link in VLC. The desktop app must stay open while the phone is playing.

Phone access is media-only in v1. PDFs and saved posts are not included in the phone playlist. Each
sharing session uses a random link token, serves only files already copied into the app library, and
stops when the user turns sharing off or closes the desktop app. It is not remote access outside the
local network and does not publish media to a Duroos server.

For sources that block anonymous fetches or require sign-in, export browser cookies in Netscape
format and place them in the app data directory as `yt-dlp-cookies.txt`. The app also accepts
`cookies.txt`, but prefers `yt-dlp-cookies.txt` when both files exist. Cookies stay local and are not
included in shared collection manifests. If a platform still blocks `yt-dlp`, manually download the
allowed media and import the local file; duplicate checks will prevent a second library copy when the
same content hash is already present.

## Privacy Defaults

- No telemetry.
- No accounts.
- No remote server.
- No automatic sharing.
- Phone access is off by default and only runs during a user-started same-Wi-Fi sharing session.
- Credentials are intended to stay in local OS-protected storage.
- Offline mode blocks remote source subscription fetches.
- Shared collection files must never include credentials, cookies, tokens, Telegram sessions, local absolute paths, or command hooks.

## Release Readiness

The repo prepares cross-platform builds for macOS app/DMG, Windows NSIS/MSI, and Linux AppImage/deb through Tauri configuration and CI workflows. Production labeling still requires external proof: signed and notarized macOS artifacts, signed Windows installers, populated media-tool checksum manifests when tools are bundled or CI-fetched, clean artifact checksums, and manual smoke tests on macOS, Windows, and Linux.

Use `npm run tauri:build:app` for local macOS app-only build verification. Use `npm run tauri:build:full` or the tag release workflow for full package generation; local full macOS packaging can still hit the DMG Finder/AppleScript packaging hang and is not production proof by itself.

Unsigned artifacts, unnotarized builds, and builds without pinned bundled media-tool checksums are alpha/testing artifacts only.

## Content Policy

Duroos Watcher stores source provenance automatically for imported items so shared collections can remain auditable. Download features should still be used only for content you are allowed to save or redistribute.

The project maintainers provide software only. They do not create, host, review, verify, moderate, endorse, or control third-party channels, feeds, manifests, media, lessons, curators, downloads, or external websites. Users and curators are solely responsible for the legality, permissions, safety, decency, and redistribution rights of any content they access or publish through the app or compatible manifests.

Official project spaces may remove links, examples, issues, discussions, or manifests that appear to promote unlawful, infringing, abusive, explicit, exploitative, hateful, harassing, privacy-invasive, or otherwise inappropriate material. This does not mean the maintainers can monitor or control third-party forks, mirrors, relays, feeds, private collections, or external communities.

See [DISCLAIMER.md](./DISCLAIMER.md) for the full non-endorsement, third-party content, warranty, and liability disclaimer.

## License

Duroos Watcher is licensed under AGPL-3.0-or-later so public hosted server variants and distributed forks remain open source.
