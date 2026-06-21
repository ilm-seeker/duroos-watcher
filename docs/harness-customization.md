# Harness Customization

Duroos Watcher is a working desktop app and a forkable local media harness. The supported customization model today is source-level customization: clone the repo, change the app, run the checks, and package your own build. There is no stable plugin SDK or external theme API yet.

## What You Can Customize

- **Visual design:** edit `src/styles.css` for color tokens, spacing, layout density, surfaces, and responsive behavior.
- **App shell and flows:** edit React components in `src/App.tsx` and pure domain helpers under `src/domain/`.
- **Source adapters:** extend source identification and display behavior in `src/domain/sourceAdapters.ts`, then connect backend ingest changes in `src-tauri/src/db.rs` when a new source needs fetch, parse, download, or persistence behavior.
- **Signed manifests:** evolve manifest parsing and validation in `src-tauri/src/manifest.rs` and matching TypeScript types when the protocol changes.
- **Teacher publishing:** adjust local publishing, Nostr announcement, Blossom storage, and archive mirror behavior in `src-tauri/src/publisher.rs`.
- **Media tools:** update pinned release tool inputs in `src-tauri/binaries/media-tools.manifest.json`, then run `npm run media-tools:fetch -- --target=<target-triple>` before packaging.
- **App identity:** update `src-tauri/tauri.conf.json`, icons under `src-tauri/icons/`, and package metadata when a fork changes the public name, bundle identifier, publisher, or category.
- **Packaging:** use the existing Tauri scripts and GitHub workflows as starting points for unsigned alpha packages or signed production releases.

## Local Build Path

```bash
npm install
npm run test
npm run build
npm run tauri:build:app
```

For active desktop development:

```bash
npm run tauri dev
```

After changing source on macOS, `npm run tauri:install:local` rebuilds the app bundle and replaces `/Applications/Duroos Watcher.app`.

## Distribution Model

The GitHub Releases page is the download surface for packaged builds. Alpha packages can be unsigned, but they must be labeled as alpha/testing artifacts and include checksums. Production packages require signing, notarization where applicable, artifact checksums, media-tool evidence, and platform smoke-test proof.

Forks should publish their own packages from their own repository or release channel. Do not present a fork as an official Duroos Watcher build unless it is released by the upstream maintainers.

## Scholarly Media Guardrails

Customization must preserve the project boundary unless a fork intentionally takes responsibility for changing it:

- Keep local-first defaults: no accounts, no telemetry, no central Duroos server, and no automatic sharing.
- Do not add credential export, local absolute paths, command hooks, or private tokens to shared manifests.
- Do not bypass platform permissions, paid access, copyright controls, or source terms.
- Treat remote feeds, manifests, thumbnails, source pages, and media files as untrusted input.
- Keep signed manifests honest: a valid signature proves integrity for a public key, not truth, legality, endorsement, safety, or religious review.
- Keep IPFS CIDs and BitTorrent magnets modeled as references unless a fork adds a complete threat model, user consent flow, and redistribution policy.

## Contribution Expectations

Small, focused pull requests are more likely to be reviewed. Include the checks you ran, explain any new source adapter or protocol behavior, and avoid mixing UI redesign, release packaging, and backend ingest changes in one PR unless the feature requires all of them.

See [CONTRIBUTING.md](../CONTRIBUTING.md) for issue and pull request rules.
