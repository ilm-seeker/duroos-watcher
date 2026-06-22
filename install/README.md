# Duroos Watcher Install Scripts

These scripts install the current unsigned alpha release from GitHub Releases. They verify the
downloaded asset against the matching SHA-256 checksum before installing.

The scripts still install unsigned alpha software. Review the script for your OS before running it,
and only continue if you trust this repository.

## Commands

These commands fetch the current installer scripts and pin the package tag explicitly, so installer
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

## Options

- `DUROOS_WATCHER_TAG`: release tag to install, default `v0.1.0-alpha.3`.
- `DUROOS_WATCHER_DRY_RUN=1`: print the selected release URL without downloading or installing.
- Linux: `DUROOS_WATCHER_PACKAGE=appimage` installs the AppImage to `~/.local/bin/duroos-watcher`.
- Linux: `DUROOS_WATCHER_PACKAGE=deb` forces the Debian package path.
- Windows: `$env:DUROOS_WATCHER_PACKAGE = "msi"` uses the MSI instead of the setup EXE.
- macOS: `DUROOS_WATCHER_INSTALL_DIR` changes the app install directory, default `/Applications`.
