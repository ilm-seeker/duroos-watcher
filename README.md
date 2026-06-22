# Duroos Watcher

Duroos Watcher is an open-source local media harness for decentralized scholarly media distribution. The reference app is a local-first desktop study library for long-form educational media: a quiet YouTube-like shell for lessons you choose to save, follow, verify, organize, and study without accounts, recommendations, comments, telemetry, or a central Duroos server.

You can download the current alpha package, build the app locally, or fork the source to customize the interface, source adapters, branding, release packaging, and scholarly media workflow. The project direction is not "build another LMS." The useful direction is narrower: a private learner and curator tool for preserving permitted lessons locally, tracking provenance, following signed teacher or curator feeds, and making playback and study flow reliable across video, audio, PDFs, and source posts.

## Download The App

The current packaged build is [Duroos Watcher v0.1.0-alpha.3](https://github.com/ilm-seeker/duroos-watcher/releases/tag/v0.1.0-alpha.3). These are unsigned alpha/testing packages, not production-signed releases. Verify the matching SHA-256 checksum before opening the app.

Current packaged CPU support: macOS Apple Silicon (`arm64`), Windows `x64`, and Linux `x86_64`.
Intel Macs need a local source build until a universal or Intel macOS release asset exists.

### Install With A Command

These commands download the release asset, verify its SHA-256 checksum, and install it. They execute
installer scripts from this repository, so inspect the scripts in [`install/`](./install/) first if
you want to review the install path before running it.
The commands fetch the current installer scripts and pin the package tag explicitly, so installer
fixes can be picked up without changing the alpha package being installed.

macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/ilm-seeker/duroos-watcher/main/install/macos.sh | DUROOS_WATCHER_ACCEPT_UNSIGNED=1 DUROOS_WATCHER_TAG=v0.1.0-alpha.3 bash
```

Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/ilm-seeker/duroos-watcher/main/install/linux.sh | DUROOS_WATCHER_ACCEPT_UNSIGNED=1 DUROOS_WATCHER_TAG=v0.1.0-alpha.3 bash
```

Windows PowerShell:

```powershell
$env:DUROOS_WATCHER_ACCEPT_UNSIGNED = "1"
$env:DUROOS_WATCHER_TAG = "v0.1.0-alpha.3"
Invoke-WebRequest -UseBasicParsing -Uri "https://raw.githubusercontent.com/ilm-seeker/duroos-watcher/main/install/windows.ps1" -OutFile "$env:TEMP\install-duroos-watcher.ps1"
powershell -ExecutionPolicy Bypass -File "$env:TEMP\install-duroos-watcher.ps1"
```

On Linux, set `DUROOS_WATCHER_PACKAGE=appimage` to avoid a system `.deb` install. On Windows, set
`$env:DUROOS_WATCHER_PACKAGE = "msi"` before running the script if you prefer the MSI package.

Linux AppImage installs use a launcher wrapper at `~/.local/bin/duroos-watcher` that sets
`WEBKIT_DISABLE_DMABUF_RENDERER=1` by default. This works around known WebKitGTK renderer crashes on
Fedora/Wayland/Mesa systems while keeping the downloaded AppImage at
`~/.local/bin/duroos-watcher.AppImage`.

### Manual Downloads

| Platform | Download | Checksum |
| --- | --- | --- |
| macOS | [Unsigned `.app.zip`](https://github.com/ilm-seeker/duroos-watcher/releases/download/v0.1.0-alpha.3/Duroos-Watcher-v0.1.0-alpha.3-macos-unsigned.app.zip) | [SHA256SUMS macOS](https://github.com/ilm-seeker/duroos-watcher/releases/download/v0.1.0-alpha.3/SHA256SUMS-v0.1.0-alpha.3-macos.txt) |
| Windows | [Unsigned setup `.exe`](https://github.com/ilm-seeker/duroos-watcher/releases/download/v0.1.0-alpha.3/Duroos-Watcher-v0.1.0-alpha.3-windows-unsigned-Duroos.Watcher_0.1.0_x64-setup.exe) or [unsigned `.msi`](https://github.com/ilm-seeker/duroos-watcher/releases/download/v0.1.0-alpha.3/Duroos-Watcher-v0.1.0-alpha.3-windows-unsigned-Duroos.Watcher_0.1.0_x64_en-US.msi) | [SHA256SUMS Windows](https://github.com/ilm-seeker/duroos-watcher/releases/download/v0.1.0-alpha.3/SHA256SUMS-v0.1.0-alpha.3-windows.txt) |
| Linux | [Unsigned `.AppImage`](https://github.com/ilm-seeker/duroos-watcher/releases/download/v0.1.0-alpha.3/Duroos-Watcher-v0.1.0-alpha.3-linux-unsigned-Duroos.Watcher_0.1.0_amd64.AppImage) or [unsigned `.deb`](https://github.com/ilm-seeker/duroos-watcher/releases/download/v0.1.0-alpha.3/Duroos-Watcher-v0.1.0-alpha.3-linux-unsigned-Duroos.Watcher_0.1.0_amd64.deb) | [SHA256SUMS Linux](https://github.com/ilm-seeker/duroos-watcher/releases/download/v0.1.0-alpha.3/SHA256SUMS-v0.1.0-alpha.3-linux.txt) |

Operating systems may warn because these alpha artifacts are not Apple-notarized and the Windows installers are not code-signed. Only install them if you trust this repository and are comfortable testing unsigned software. See [unsigned alpha install notes](./docs/release/unsigned-alpha-install-notes.md) for safer review steps.

If macOS says the app is damaged, delete any older `v0.1.0-alpha.2` download, download `v0.1.0-alpha.3`, verify the checksum, move `Duroos Watcher.app` to `/Applications`, and open it with Control-click or right-click then **Open**. If the verified `v0.1.0-alpha.3` app is still blocked by quarantine, the release notes include the one-app `xattr` command to clear quarantine for `/Applications/Duroos Watcher.app`.

To build or customize instead of installing a packaged alpha, use the development commands below and read [Harness Customization](./docs/harness-customization.md).

## Project Direction

Duroos Watcher is moving toward a local study-library, trusted-source layer, and forkable media harness:

- **Local learner library first:** imported and downloaded media lives in an app-managed local library backed by SQLite.
- **Source provenance by default:** lessons keep source URLs, adapter names, hashes when available, permission notes, and import/download history.
- **Review-first downloads:** feeds and manifests can discover lessons, but media downloads stay explicit and visible.
- **Signed curator channels:** Duroos manifests can carry curator identity, source references, retrieval references, media hashes, and Ed25519 signatures.
- **Teacher-owned publishing:** teachers can publish signed channel updates through user-configured Nostr relays and Blossom media servers without Duroos running a central catalog.
- **Customizable source harness:** forks can redesign the UI, adjust CSS tokens, add source adapters, change app identity, and package their own local media workflows from the same Tauri + React foundation.
- **Study flow, not social feed:** Smart Library grouping, search, resume progress, lightweight notes, and source-aware cleanup are more important than likes, comments, trending pages, or public profiles.
- **Privacy-preserving defaults:** no telemetry, no accounts, no remote server, offline mode blocks remote source refreshes, and local cookies or credentials are never exported in collection manifests.

That makes the project closest to a cross between a personal study archive, a source-aware media library, a signed feed reader, and a local desktop harness for scholarly media workflows. The core bet is that serious learners and teachers need reliable private access to lessons more than they need another social video platform.

## Does This Already Exist?

Adjacent tools exist, but Duroos Watcher's exact combination is narrower than the existing categories: accountless desktop study library, multi-source lesson ingest, local media playback, provenance records, signed teacher manifests, trusted curator keys, review-first downloads, and temporary same-Wi-Fi phone playback.

| Category | Existing examples | Overlap | Why Duroos Watcher is different |
| --- | --- | --- | --- |
| LMS | [Moodle LMS](https://moodle.com/products/lms/), Canvas-style systems | Courses, learners, hosted content, assignments, quizzes, progress tracking | An LMS is usually institution/course infrastructure. Duroos Watcher is a personal local desktop library with no required accounts, classroom roster, grading, hosted course site, or central server. |
| Offline learning platform | [Kolibri](https://learningequality.org/kolibri/about-kolibri/) | Offline-first education, local access, educator-managed resources | Kolibri is closer than a normal LMS, but it is built around classroom/program deployments, learner accounts, assessments, content channels, and local servers. Duroos Watcher is a private source-following and media-preservation app. |
| YouTube archiver | [Tube Archivist](https://github.com/tubearchivist/tubearchivist), [Pinchflat](https://github.com/kieraneglin/pinchflat), [ytdl-sub](https://github.com/jmbannon/ytdl-sub) | Downloading, archiving, indexing, watched/unwatched state | These are strong references for archival workflows, but they are mostly YouTube/media-center oriented. Duroos Watcher is lesson/source/provenance oriented, supports non-YouTube source records, signed manifests, and teacher publishing. |
| Media server | [Jellyfin](https://jellyfin.org/), Plex, Kodi | Local media management and streaming | Media servers are good at playback libraries. They are not built around source ingest contracts, curator trust, signed manifests, permission notes, or teacher feed publishing. |
| Federated video host | [PeerTube](https://joinpeertube.org/) | Independent video publishing, federation, no central Big Tech platform | PeerTube is server-side public video hosting. Duroos Watcher is client-side private study, download, verification, and library organization. |
| Research or note library | [Zotero](https://www.zotero.org/), Obsidian-style workflows | Personal organization, notes, source awareness | These are better for documents, citations, and writing. Duroos Watcher is media playback, lesson provenance, source refresh, and offline study. |

Verdict: this is not wasted effort, but the product needs disciplined scope. If Duroos Watcher tries to become a full LMS, a general Plex alternative, or a public video network, it will compete with larger mature projects. Its stronger lane is: **follow trusted lesson sources, save permitted media locally, preserve provenance, and study without platform noise.**

## Current Status

Duroos Watcher is pre-1.0 alpha software. The app has working local-first foundations, source ingest paths, local media playback, Smart Library features, manifest validation, and teacher publishing pieces, but production distribution still requires signing, notarization, artifact evidence, and platform smoke tests.

Unsigned or unnotarized builds are testing artifacts only.

## Current Capabilities

- Tauri v2 + React + TypeScript + Vite desktop app.
- SQLite-backed local library for sources, teachers, collections, lessons, media files, provenance, watch state, notes, jobs, and trusted curators.
- Local import for video, audio, and PDF study files.
- Duplicate protection through content hashes, source URLs, and feed duplicate checks.
- Smart Library grouping by teacher, collection, source, content type, and availability.
- Resume progress for in-app video/audio playback.
- Lightweight lesson notes and manual metadata correction for teacher/course organization.
- Source adapter registry for local files, Telegram public previews, RSS/Atom/JSON Feed, Archive.org, YouTube, X, Rumble, Odysee, and signed teacher channels.
- Public Telegram channel preview ingest when Telegram exposes a `t.me/s` page.
- RSS, Atom, and JSON Feed ingest for videos, audio enclosures, PDFs, and post/message entries.
- Archive.org item ingest through the official metadata API.
- Direct user-added YouTube, Rumble, and Odysee URLs create reviewable source rows; X URLs are saved as credential-bound post references.
- Local media downloads through direct HTTP or `yt-dlp` when the lesson URL is supported by local tools and permitted by the user.
- Downloaded media is sha256-verified when a source provides a sha256 hash; mismatches are rejected before the file is attached to a lesson.
- Native player fallback can launch local media through VLC, mpv, or ffplay when available.
- Temporary **Watch on Phone** sharing creates same-Wi-Fi VLC playlist links for ready audio/video files.
- Shared collection manifest validation rejects credentials, local absolute paths, command hooks, unsafe file paths, and other export hazards.

## Trust And Publishing

Duroos Watcher does not use blockchain in v1. The useful primitives are signed metadata, local storage, source provenance, and optional open feed transports.

- **Duroos manifests:** schema v2 manifests include curator identity, source refs, optional retrieval refs, sha256 hashes, and Ed25519 signatures. A valid signature proves integrity for that public key, not automatic trust.
- **Trusted curators:** users can trust a curator key after validating a signed manifest and confirming the curator identity outside the manifest.
- **Review-first media:** even trusted feeds are reviewed before media is downloaded locally.
- **Modeled transport refs:** IPFS CIDs and BitTorrent magnets can be validated as manifest references for redistributable content, but they are not default media transports in v1.
- **Teacher publisher:** publisher profiles keep signing keys in a passphrase-encrypted local vault. Nostr relays announce signed channel updates, Blossom servers store hash-addressed media and manifest blobs, and optional archive mirrors are announced only after SHA-256 verification.

## Non-Goals

- Not a hosted LMS with classes, grades, rosters, assignments, quizzes, or institutional reporting.
- Not a public social network, comment system, recommendation engine, or central discovery catalog.
- Not a general-purpose media server replacement for Jellyfin/Plex/Kodi.
- Not a tool for bypassing source permissions, paid access, platform terms, or copyright rules.
- Not a claim that signed, hashed, or archived content is lawful, accurate, endorsed, safe, or religiously reviewed.
- Not remote phone access outside the local network.

## Privacy Defaults

- No telemetry.
- No accounts.
- No remote Duroos server.
- No automatic sharing.
- Phone access is off by default and only runs during a user-started same-Wi-Fi sharing session.
- Credentials are intended to stay in local OS-protected storage.
- Offline mode blocks remote source subscription fetches.
- Shared collection files must never include credentials, cookies, tokens, Telegram sessions, local absolute paths, or command hooks.

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
npm run tauri:install:local
```

If `cargo` is missing, install Rust from <https://www.rust-lang.org/tools/install> before running Tauri.

`npm run tauri dev` and `npm run tauri:build:app` do not update the app launched from Finder, Spotlight, or `/Applications`. After source changes, run `npm run tauri:install:local` to rebuild and replace `/Applications/Duroos Watcher.app`.

Platform setup:

- macOS: install Xcode Command Line Tools, Rust, Node 22, and optional media tools with Homebrew (`yt-dlp`, `ffmpeg`, `mpv`, or VLC).
- Windows: install Rust MSVC, Node 22, WebView2 Runtime, and optional `yt-dlp.exe`, FFmpeg, VLC, or mpv on `PATH`.
- Linux: install Rust, Node 22, WebKitGTK 4.1 development packages, Ayatana AppIndicator, librsvg, and optional `yt-dlp`, FFmpeg, VLC, mpv, or ffplay.

## Media Tools

Downloading source media uses direct HTTP for feed enclosures such as audio, video, and PDFs when available. Platform video pages use `yt-dlp`; media validation, thumbnails, and WebView-compatible transcodes use `ffmpeg` and `ffprobe`. The app checks bundled release tools first, then common system install paths and `python3 -m yt_dlp`, and surfaces whether required tools are bundled, system-provided, mixed, or missing.

Keep `yt-dlp` current. On macOS with Homebrew:

```bash
brew upgrade yt-dlp
yt-dlp --version
```

For sources that block anonymous fetches or require sign-in, export browser cookies in Netscape format and place them in the app data directory as `yt-dlp-cookies.txt`. The app also accepts `cookies.txt`, but prefers `yt-dlp-cookies.txt` when both files exist. Cookies stay local and are not included in shared collection manifests.

Release media tools are pinned in `src-tauri/binaries/media-tools.manifest.json`. To fetch and verify pinned tools for a packaging target, run:

```bash
npm run media-tools:fetch -- --target=aarch64-apple-darwin
```

The generated `src-tauri/binaries/vendor/<target>/media-tools-report.json` is release evidence and must match the manifest before those binaries are bundled into a production artifact.

## Watch On Phone

The desktop app can temporarily share ready audio and video files on the local Wi-Fi network for playback in VLC on iOS or Android. Use **Watch on Phone** in the library dashboard, scan the QR code with the phone, and open the link in VLC. The desktop app must stay open while the phone is playing.

Phone access is media-only in v1. PDFs and saved posts are not included in the phone playlist. Each sharing session uses a random link token, serves only files already copied into the app library, and stops when the user turns sharing off or closes the desktop app. It is not remote access outside the local network and does not publish media to a Duroos server.

## Release Readiness

The repo prepares cross-platform builds for macOS app/DMG, Windows NSIS/MSI, and Linux AppImage/deb through Tauri configuration and CI workflows. For `v0.1.0`, production labeling is macOS + Windows only; Linux artifacts remain alpha until the upstream `glib` advisory path is resolved or explicitly accepted. Production labeling still requires external proof:

- signed and notarized macOS artifacts
- signed Windows installers
- populated media-tool checksum manifests when tools are bundled or CI-fetched
- clean artifact checksums
- no open release-blocking GitHub alerts for the release commit
- manual smoke tests on macOS and Windows
- Linux alpha artifact audit, bundled media-tool report, and launch smoke evidence

Use `npm run tauri:build:app` for local macOS app-only build verification. Use `npm run tauri:build:full` or the tag release workflow for full package generation; local full macOS packaging can still hit the DMG Finder/AppleScript packaging hang and is not production proof by itself.

Run `npm run release:preflight` before pushing a release tag to list missing signing credentials, open GitHub alerts, release/evidence gaps, and Windows signing workflow gaps.

See [docs/release-readiness.md](./docs/release-readiness.md) for release evidence requirements.

## Content Policy

Duroos Watcher stores source provenance automatically for imported items so shared collections can remain auditable. Download features should still be used only for content you are allowed to save or redistribute.

The project maintainers provide software only. They do not create, host, review, verify, moderate, endorse, or control third-party channels, feeds, manifests, media, lessons, curators, downloads, or external websites. Users and curators are responsible for the legality, permissions, safety, decency, and redistribution rights of any content they access or publish through the app or compatible manifests.

See [DISCLAIMER.md](./DISCLAIMER.md) for the full non-endorsement, third-party content, warranty, and liability disclaimer.

## License

Duroos Watcher is licensed under AGPL-3.0-or-later so public hosted server variants and distributed forks remain open source.
