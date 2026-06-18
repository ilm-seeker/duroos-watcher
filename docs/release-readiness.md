# Duroos Watcher Release Readiness

## Artifact Labels

- **Alpha/testing:** unsigned, unnotarized, or missing pinned bundled media-tool checksums.
- **Production candidate:** CI passed on macOS, Windows, and Ubuntu; artifact checksums generated; no tracked secrets; media-tool manifest populated when tools are bundled or CI-fetched.
- **Production:** production candidate plus macOS signing/notarization, Windows signing, Linux package review, and manual smoke tests on all target OSes.

## Platform Smoke Tests

Run these on macOS, Windows, and Linux before production labeling:

- Install and launch the desktop app.
- Import local video, audio, and PDF files.
- Follow a signed channel or manifest and preview trust state.
- Refresh a followed channel.
- Download missing channel media.
- Publish a channel with one passing Nostr relay and one passing Blossom server.
- Toggle offline mode and confirm remote fetches stop.
- Play native media through the WebView and one native player fallback.
- Start and stop same-Wi-Fi phone sharing.
- Inspect UI, logs, docs, and release artifacts for secrets, cookies, private paths, publisher identity leaks, and local account names.

## External Requirements

- Apple Developer signing identity and notarization credentials.
- Windows code-signing certificate.
- Release-key custody for Tauri signing if updater distribution is enabled later.
- Populated `src-tauri/binaries/media-tools.manifest.json` entries for every bundled or CI-fetched media tool.
- Final release notes that state third-party endpoint presets are editable, public, and not operated by Duroos.
