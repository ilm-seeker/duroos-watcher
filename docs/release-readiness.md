# Duroos Watcher Release Readiness

## Artifact Labels

- **Alpha/testing:** unsigned, unnotarized, or missing pinned bundled media-tool checksums.
- **Production candidate:** CI passed on macOS, Windows, and Ubuntu; artifact checksums and `artifact-audit.json` generated; no tracked secrets; media-tool manifest populated when tools are bundled or CI-fetched.
- **Production:** production candidate plus macOS signing/notarization, Windows signing, Linux package review, and manual smoke tests on all target OSes.

## Build Commands

- Local app-only macOS verification: `npm run tauri:build:app`.
- Full local packaging: `npm run tauri:build:full`.
- Production release packaging: tag push through `.github/workflows/release.yml`.

The app-only build avoids the local DMG Finder/AppleScript packaging path. Full macOS packaging still
needs a non-hanging DMG runner plus signing/notarization proof before production labeling.

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
- Verified extraction and license carry-forward for every entry in `src-tauri/binaries/media-tools.manifest.json`.
- Final release notes that state third-party endpoint presets are editable, public, and not operated by Duroos.

## Production Evidence Required

- Successful GitHub Actions CI matrix run on macOS, Windows, and Ubuntu for the exact release commit.
- Successful tag release workflow with artifact audit uploads for each platform.
- macOS signed and notarized app/DMG evidence.
- Windows signed installer evidence.
- Linux AppImage/deb install and launch evidence.
- Manual smoke-test notes for every item in the platform checklist above.
