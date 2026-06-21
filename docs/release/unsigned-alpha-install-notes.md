# Duroos Watcher Unsigned Alpha

This is an unsigned alpha build for testers. It is not a production release.

## Downloads

- macOS: download the `macos-unsigned.app.zip` asset.
- Windows: download the `windows-unsigned` installer asset.
- Linux: download the `linux-unsigned` AppImage or deb asset.
- Download the matching `SHA256SUMS` file and verify the checksum before opening the app.

## Why The OS Warns

These alpha artifacts are not Apple-notarized and the Windows installers are not code-signed.
macOS Gatekeeper and Windows SmartScreen may warn because the operating system cannot verify a
publisher signature. That warning is expected for this alpha build.

Only install this build if you trust the repository and are comfortable testing unsigned software.

## Safer Review Steps

Before installing:

1. Verify the downloaded file against the included SHA-256 checksum.
2. Inspect the source code and GitHub Actions workflow for this tag.
3. Scan the downloaded artifact with your operating system security tools or a malware scanning
   service you trust.
4. Optionally ask an AI code-review or security assistant to audit the source and build workflow.
   Treat that as an extra review aid, not as proof that the app is malware-free.

## Current Scope

The app is intended for local-first testing of importing, organizing, playing, downloading, and
sharing educational media. Linux remains alpha-scoped while the upstream Tauri GTK/WebKit `glib`
advisory path is unresolved.
