# Duroos Watcher Release Readiness

## Artifact Labels

- **Alpha/testing:** unsigned, unnotarized, or missing pinned bundled media-tool checksums.
- **Production candidate:** CI passed on macOS, Windows, and Ubuntu; artifact checksums and `artifact-audit.json` generated; no tracked secrets; media-tool manifest populated when tools are bundled or CI-fetched.
- **Production:** production candidate plus macOS signing/notarization, Windows signing, Linux package review, and manual smoke tests on all target OSes.

## Build Commands

- Fetch target media tools: `npm run media-tools:fetch -- --target=<target-triple>`.
- Local app-only macOS verification: `npm run tauri:build:app`.
- Full local packaging: `npm run tauri:build:full`.
- Release blocker preflight: `npm run release:preflight`.
- Production release packaging: tag push through `.github/workflows/release.yml`.
- Production evidence gate: `npm run release:production-gate`.

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
- Verified extraction report from `npm run media-tools:fetch` plus license carry-forward for every packaged entry in `src-tauri/binaries/media-tools.manifest.json`.
- Final release notes that state third-party endpoint presets are editable, public, and not operated by Duroos.

Run `npm run release:preflight` before pushing a release tag. It checks repo-side release rules,
GitHub alert state, expected signing secret names, release/tag presence, the Windows signing wiring
gap, and the production evidence file. It is expected to fail until the external signing credentials,
tag release, artifact audit, and manual QA evidence are present.

Expected GitHub secret names for production preflight:
`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, `APPLE_CERTIFICATE`,
`APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`,
`APPLE_TEAM_ID`, `WINDOWS_CERTIFICATE`, and `WINDOWS_CERTIFICATE_PASSWORD`.

Expected GitHub variable names for Windows signing preflight:
`WINDOWS_CERTIFICATE_THUMBPRINT` and `WINDOWS_TIMESTAMP_URL`. These are not enough by themselves;
the release workflow imports the base64-encoded PFX from `WINDOWS_CERTIFICATE`, writes a generated
`src-tauri/tauri.windows.conf.json` with the thumbprint and timestamp URL, and lets Tauri sign the
NSIS/MSI outputs from the certificate store. `WINDOWS_CERTIFICATE_THUMBPRINT` must be the 40-character
SHA-1 thumbprint of the imported certificate, and `WINDOWS_TIMESTAMP_URL` must be an HTTPS timestamp
server URL.

## Production Evidence Required

- Successful GitHub Actions CI matrix run on macOS, Windows, and Ubuntu for the exact release commit.
- Successful tag release workflow with artifact audit uploads for each platform.
- `media-tools-report.json` for each target whose package includes bundled media tools.
- No open GitHub code scanning or Dependabot alerts for the exact release commit.
- macOS signed and notarized app/DMG evidence.
- Windows signed installer evidence.
- Linux AppImage/deb install and launch evidence.
- Manual smoke-test notes for every item in the platform checklist above.

The current `glib` Dependabot alert is a Linux production blocker, not a local application-code bug:
`cargo update --manifest-path src-tauri/Cargo.toml -p glib --precise 0.20.12 --dry-run` fails because
the Tauri Linux stack requires `gtk 0.18.x`, which requires `glib ^0.18`. Do not mark Linux production
ready until that upstream dependency path is patched or Linux production distribution is explicitly
removed from scope.

Keep real production evidence in `docs/production-release-evidence.json` and downloaded artifact proof
under `release-evidence/`; both are ignored by Git because manual QA notes, signing proof, and package
downloads can include local account names, private paths, certificate metadata, or large binaries. Use
`docs/production-release-evidence.example.json` as the shape, then run
`npm run release:production-gate`. A release is not production-ready until that command passes for the
exact commit being shipped.
