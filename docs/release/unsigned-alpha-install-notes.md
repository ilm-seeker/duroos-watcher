# Duroos Watcher Unsigned Alpha

This is an unsigned alpha build for testers. It is not a production release.

## Downloads

- macOS: download `Duroos-Watcher-<tag>-macos-unsigned.app.zip` and `SHA256SUMS-<tag>-macos.txt`.
- Windows: download the `windows-unsigned` `.exe` or `.msi` installer and `SHA256SUMS-<tag>-windows.txt`.
- Linux: download the `linux-unsigned` `.AppImage` or `.deb` package and `SHA256SUMS-<tag>-linux.txt`.

Use files from the same release tag. Do not mix a zip from one tag with a checksum file from another
tag.

Current packaged CPU support: macOS Apple Silicon (`arm64`), Windows `x64`, and Linux `x86_64`.
Intel Macs need a local source build until a universal or Intel macOS release asset exists.

## Install With A Command

These commands download the release asset, verify its SHA-256 checksum, and install it. They execute
installer scripts from this repository. Inspect the scripts first if you want to review the install
path before running it.
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
