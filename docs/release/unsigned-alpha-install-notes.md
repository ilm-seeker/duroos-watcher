# Duroos Watcher Unsigned Alpha

This is an unsigned alpha build for testers. It is not a production release.

## Downloads

- macOS: download the `macos-unsigned.app.zip` asset.
- Windows: download the `windows-unsigned` installer asset.
- Linux: download the `linux-unsigned` AppImage or deb asset.
- Download the matching `SHA256SUMS` file and verify the checksum before opening the app.

## Why The OS Warns

These alpha artifacts are not Apple-notarized, not Apple Developer ID signed, and the Windows
installers are not code-signed. The macOS `.app` bundle is ad-hoc signed only so bundle resources are
sealed consistently for testers. macOS Gatekeeper and Windows SmartScreen may still warn because the
operating system cannot verify a publisher signature. That warning is expected for this alpha build.

Only install this build if you trust the repository and are comfortable testing unsigned software.

## macOS Install Notes

1. Verify the downloaded zip against `SHA256SUMS`.
2. Unzip `Duroos-Watcher-*-macos-unsigned.app.zip`.
3. Move `Duroos Watcher.app` to `/Applications`.
4. Open it with Control-click or right-click, then choose `Open`.

If macOS still says the app is damaged after you have verified the checksum and reviewed the source
or workflow, remove the quarantine marker for this app only:

```sh
xattr -dr com.apple.quarantine "/Applications/Duroos Watcher.app"
open "/Applications/Duroos Watcher.app"
```

Do not run that command for apps from sources you do not trust. Older alpha macOS assets may need to
be deleted and replaced with the latest release asset.

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
