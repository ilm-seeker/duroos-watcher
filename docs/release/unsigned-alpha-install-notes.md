# Duroos Watcher Unsigned Alpha

This is an unsigned alpha build for testers. It is not a production release.

## Downloads

- macOS: download `Duroos-Watcher-<tag>-macos-unsigned.app.zip` and `SHA256SUMS-<tag>-macos.txt`.
- Windows: download the `windows-unsigned` `.exe` or `.msi` installer and `SHA256SUMS-<tag>-windows.txt`.
- Linux: download the `linux-unsigned` `.AppImage` or `.deb` package and `SHA256SUMS-<tag>-linux.txt`.

Use files from the same release tag. Do not mix a zip from one tag with a checksum file from another
tag.

## Why The OS Warns

These alpha artifacts are not Apple-notarized, not Apple Developer ID signed, and the Windows
installers are not code-signed. The macOS `.app` bundle is ad-hoc signed only so bundle resources are
sealed consistently for testers. macOS Gatekeeper and Windows SmartScreen may still warn because the
operating system cannot verify a publisher signature. That warning is expected for this alpha build.

Only install this build if you trust the repository and are comfortable testing unsigned software.

## macOS Install Notes

Use `v0.1.0-alpha.3` or newer on macOS. The older `v0.1.0-alpha.2` macOS zip can trigger a
misleading damaged-app warning because the bundle resources were not sealed correctly. Delete older
macOS alpha zips/apps before testing the current release.

1. Download the macOS zip and matching macOS `SHA256SUMS` file from this release.
2. Verify the downloaded zip:

   ```sh
   cd ~/Downloads
   shasum -a 256 -c SHA256SUMS-v0.1.0-alpha.3-macos.txt
   ```

   The expected result ends with `OK`.

3. Unzip `Duroos-Watcher-*-macos-unsigned.app.zip`.
4. Delete any older `/Applications/Duroos Watcher.app`.
5. Move the new `Duroos Watcher.app` to `/Applications`.
6. Open it with Control-click or right-click, then choose `Open`.

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
