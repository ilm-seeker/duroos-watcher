# GLib Linux Alpha Blocker - 2026-06-21

## Status

`GHSA-wrw7-89jp-8q8g` remains a Linux-only production blocker for Duroos Watcher.
Do not treat Linux artifacts as production until the dependency chain moves to
`glib >= 0.20.0` or a release owner explicitly accepts the risk in a signed
release decision.

The GitHub Dependabot alert was dismissed as `tolerable_risk` on 2026-06-22
after verifying that Linux remains alpha-scoped and no compatible dependency
update lifts this Tauri GTK/WebKit path to `glib >= 0.20.0`. The dismissal
closes the current GitHub report; it does not clear Linux for production.

## Advisory

- GitHub Advisory: https://github.com/advisories/GHSA-wrw7-89jp-8q8g
- RustSec: https://rustsec.org/advisories/RUSTSEC-2024-0429.html
- Affected package: `glib`
- Affected versions: `>= 0.15.0, < 0.20.0`
- Patched version: `>= 0.20.0`
- Reported issue: unsound `glib::VariantStrIter` iterator implementations.

## Current Dependency Evidence

Command:

```bash
cargo tree --manifest-path src-tauri/Cargo.toml --target all -i glib
```

Current result:

```text
glib v0.18.5
|-- atk v0.18.2
|   `-- gtk v0.18.2
|       |-- muda v0.19.2
|       |   `-- tauri v2.11.2
|       |       |-- duroos-watcher v0.1.0 (/Users/traveler/Documents/Duroos Watcher/src-tauri)
|       |       |-- tauri-plugin-dialog v2.7.1
|       |       |-- tauri-plugin-notification v2.3.3
|       |       `-- tauri-plugin-opener v2.5.4
|       |-- tao v0.35.3
|       |-- tauri v2.11.2
|       |-- tauri-runtime v2.11.2
|       |-- tauri-runtime-wry v2.11.2
|       |-- webkit2gtk v2.0.2
|       `-- wry v0.55.1
|-- cairo-rs v0.18.5
|-- gdk v0.18.2
|-- gdk-pixbuf v0.18.5
|-- gdkx11 v0.18.2
|-- gio v0.18.4
|-- gtk v0.18.2
|-- javascriptcore-rs v1.1.2
|-- pango v0.18.3
|-- soup3 v0.5.0
`-- webkit2gtk v2.0.2
```

The app does not directly depend on `glib`; the alert came through the Tauri
Linux GTK/WebKit stack.

## GitHub Alert Handling

Dependabot alert `#1` for `glib` in `src-tauri/Cargo.lock` was dismissed with
reason `tolerable_risk`.

Dismissal comment:

```text
Linux alpha only. GHSA-wrw7-89jp-8q8g reaches glib 0.18.5 via Tauri GTK/WebKit; glib and tauri dry-runs do not reach >=0.20.0. Keep knownPlatformBlockers until upstream GTK4/glib fix or Linux prod scope removed.
```

If GitHub reopens the alert because advisory metadata or the dependency graph
changes, re-run the dependency evidence below before dismissing it again.

## Update Attempts

These dry runs did not produce a compatible update to `glib >= 0.20.0`:

```bash
cargo update --manifest-path src-tauri/Cargo.toml -p glib --dry-run
# Locking 0 packages to latest compatible versions

cargo update --manifest-path src-tauri/Cargo.toml -p tauri --recursive --dry-run
# Updates Tauri 2.11.2 -> 2.11.3 and related packages, but does not lift glib.

cargo update --manifest-path src-tauri/Cargo.toml -p gtk --recursive --dry-run
# Updates syn only; does not lift gtk/glib.
```

## Release Handling

Until resolved:

- `docs/production-release-evidence.json` may list `linux` in
  `release.alphaPlatforms`.
- `release.knownPlatformBlockers` may include `GHSA-wrw7-89jp-8q8g` for
  `linux` even when the Dependabot alert is dismissed.
- macOS and Windows production release evidence must still be real and complete:
  signing, notarization, artifact audits, media-tool reports, CI, release
  workflow, and manual QA cannot be replaced by this note.

## Unblock Criteria

Use one of these before making Linux a production platform:

1. Upgrade the Tauri Linux dependency chain so `cargo tree --target all -i glib`
   resolves to `glib >= 0.20.0`.
2. Remove Linux from bundled production targets.
3. Keep Linux artifacts explicitly alpha and include this note in the production
   evidence for the release.
