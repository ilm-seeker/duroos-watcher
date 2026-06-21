# Contributing

Duroos Watcher is an open-source local media harness and pre-1.0 desktop app. Contributions are welcome when they improve the app, the harness, release safety, source adapters, documentation, or test coverage without weakening the local-first security and content boundaries.

This is a low-maintainer project. Small, focused pull requests with clear evidence are the best fit.

## Good Fits

- Bug fixes with reproducible steps.
- Documentation that makes downloads, builds, customization, release evidence, or security boundaries clearer.
- Focused UI or accessibility fixes that preserve the existing product direction.
- Source-adapter improvements for public, permission-respecting feeds and manifests.
- Tests for manifest validation, source parsing, local media handling, release checks, or pure TypeScript domain behavior.
- Release workflow hardening that does not leak credentials or overstate artifact readiness.

## Not A Fit

- Requests to bypass paid access, private content, platform terms, DRM, copyright controls, or source permissions.
- Issues asking maintainers to endorse, verify, moderate, host, or distribute third-party lessons, channels, feeds, curators, manifests, or media.
- Pull requests that add telemetry, accounts, a central Duroos server, automatic sharing, or credential export.
- Large redesigns mixed with backend changes and release workflow changes in one PR.
- Secret-bearing logs, cookies, tokens, Telegram sessions, private keys, local absolute paths, or personal media samples.

## Before Opening An Issue

1. Check the latest README download section and [GitHub Releases](https://github.com/ilm-seeker/duroos-watcher/releases).
2. For unsigned alpha packages, read [unsigned alpha install notes](./docs/release/unsigned-alpha-install-notes.md).
3. For security concerns, read [SECURITY.md](./SECURITY.md) and avoid posting exploit details publicly.
4. Confirm whether the problem is in Duroos Watcher software or in a third-party source, platform, channel, feed, or media item.

## Pull Request Checklist

Run the narrowest checks that cover your change. Use existing scripts where possible.

```bash
npm run test
npm run build
npm run release:check
git diff --check
```

For Rust/Tauri changes, also run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

For release packaging changes, explain whether you tested alpha packaging, full packaging, artifact audits, media-tool fetching, or only static release checks.

## Source Adapter Rules

Source adapters must preserve review-first downloading and provenance. A new adapter should make clear:

- what source format it accepts
- what data is stored locally
- whether it requires credentials or cookies
- whether media downloads use direct URLs, feed enclosures, or local tools such as `yt-dlp`
- how permission notes, hashes, signatures, or original source URLs are preserved
- what tests or fixtures prove the parser behavior

Do not include private cookies, tokens, session files, or copyrighted sample media in fixtures.

## Fork And Customization Rules

Forks may redesign the UI, change color schemes, add source adapters, rebrand the app, or package their own builds under the AGPL license. Forks should not present themselves as official Duroos Watcher builds unless released by upstream maintainers.

See [Harness Customization](./docs/harness-customization.md) for the current source-level customization surfaces.
