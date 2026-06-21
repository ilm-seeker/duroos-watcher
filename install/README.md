# Duroos Watcher Install Scripts

These scripts install the current unsigned alpha release from GitHub Releases. They verify the
downloaded asset against the matching SHA-256 checksum before installing.

The scripts still install unsigned alpha software. Review the script for your OS before running it,
and only continue if you trust this repository.

## Commands

These commands are pinned to the installer-script revision used for this alpha so raw GitHub cache
lag cannot serve an older script.

macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/ilm-seeker/duroos-watcher/6b93d056ac7d8d8874ad46294c89a08254eb0cc3/install/macos.sh | DUROOS_WATCHER_ACCEPT_UNSIGNED=1 bash
```

Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/ilm-seeker/duroos-watcher/6b93d056ac7d8d8874ad46294c89a08254eb0cc3/install/linux.sh | DUROOS_WATCHER_ACCEPT_UNSIGNED=1 bash
```

Windows PowerShell:

```powershell
$env:DUROOS_WATCHER_ACCEPT_UNSIGNED = "1"
Invoke-WebRequest -UseBasicParsing -Uri "https://raw.githubusercontent.com/ilm-seeker/duroos-watcher/6b93d056ac7d8d8874ad46294c89a08254eb0cc3/install/windows.ps1" -OutFile "$env:TEMP\install-duroos-watcher.ps1"
powershell -ExecutionPolicy Bypass -File "$env:TEMP\install-duroos-watcher.ps1"
```

## Options

- `DUROOS_WATCHER_TAG`: release tag to install, default `v0.1.0-alpha.3`.
- `DUROOS_WATCHER_DRY_RUN=1`: print the selected release URL without downloading or installing.
- Linux: `DUROOS_WATCHER_PACKAGE=appimage` installs the AppImage to `~/.local/bin/duroos-watcher`.
- Linux: `DUROOS_WATCHER_PACKAGE=deb` forces the Debian package path.
- Windows: `$env:DUROOS_WATCHER_PACKAGE = "msi"` uses the MSI instead of the setup EXE.
- macOS: `DUROOS_WATCHER_INSTALL_DIR` changes the app install directory, default `/Applications`.
