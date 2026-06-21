# Duroos Watcher Deep QA And Production Handoff - 2026-06-20

## Scope

This pass tested the current Duroos Watcher repo at `/Users/traveler/Documents/Duroos Watcher` and the freshly built macOS app bundle from this checkout.

Build under test:

- Repo commit reported by release preflight: `954f0560830d3cc393a48c7f81c7e2e80af1682d`.
- Fresh app bundle: `src-tauri/target/release/bundle/macos/Duroos Watcher.app`.
- Native runtime was launched directly from the fresh build output, not the older installed `/Applications/Duroos Watcher.app`.
- Local app database: `/Users/traveler/Library/Application Support/io.duroos.watcher/duroos.sqlite3`.

No source-code fixes were made in this pass. One real missing media item was intentionally downloaded through the app as part of download QA.

## Automated Checks

| Check | Result | Time |
| --- | --- | ---: |
| `npm test` | 10 files, 53 tests passed | 1.77s |
| `npm run build` | TypeScript and Vite build passed | 3.54s |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | Passed | 0.49s |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` | Passed | 5.48s |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 96 tests passed | 3.04s |
| `npm audit --package-lock-only` | 0 vulnerabilities | 1.13s |
| `npm audit --omit=dev --package-lock-only` | 0 vulnerabilities | 1.11s |
| `npm run release:check` | Passed | 0.40s |
| `npm run release:audit-artifacts` | Audited 3 artifacts | 1.50s |
| `npm run tauri:build:app` | App-only macOS bundle built | 34.26s |

Release gates:

- `npm run release:preflight` failed only on production/external blockers:
  - Missing GitHub Actions signing secrets for Tauri, Apple, and Windows.
  - Missing Windows signing variables.
  - Open production-blocking Dependabot alert: `glib` `GHSA-wrw7-89jp-8q8g`.
  - No `v0.1.0*` GitHub release.
  - Missing `docs/production-release-evidence.json`.
- `npm run release:production-gate` failed because `docs/production-release-evidence.json` is missing.
- `cargo tree --manifest-path src-tauri/Cargo.toml --target all -i glib` confirmed the advisory path is still through the Linux GTK/Tauri dependency chain, not a direct app dependency.

## Local Runtime Baseline

Before the live download, the native app showed:

- 6 lessons.
- 5 local files.
- 1 item needing a file.
- 0 active jobs.
- Desktop runtime available.
- System media tools ready.

After the live download test, the native app showed:

- 6 lessons.
- 6 local files.
- 0 items needing files.
- 17 queue jobs.
- 5 phone-shareable media items.

Current database counts after cleanup of the QA-created watch-state row:

| Metric | Count |
| --- | ---: |
| Lessons | 6 |
| Media files | 6 |
| Missing-media lessons | 0 |
| Jobs | 17 |
| Publisher profiles | 2 |
| Publisher channels | 1 |
| Published channel items | 0 |
| Trusted curators | 1 |
| Watch state rows | 1 |
| Lesson notes | 0 |

Local storage:

- Managed library size: 530 MB.
- SQLite database size: 688 KB.

## Runtime Workflow Results

### Dashboard And Library

Passed:

- Fresh build opened successfully.
- Header accurately reflected runtime and downloader readiness.
- Library counters updated after download: `6 local files`, `0 need files`.
- Smart Library filters showed expected counts.
- Downloaded YouTube lesson selection resolved in under a second.
- In-app video playback loaded through `asset://localhost/...mp4`.
- Playback controls changed to Pause and watch progress advanced.
- Resume state appeared in Continue after playback.
- QA-created progress was reset and the resulting zero-progress row was removed from `watch_state`.

Observation:

- The selected video preview was visually black for the tested lesson. This may be valid source content, because the asset loaded and progress advanced. Future visual QA should test a brighter video fixture to avoid false concern.

### Downloading

Live app download test:

- Source: `Channel: Codex Signed Channel Test 2026-06-18`.
- Item: `duroos watcher publish test 20260618`.
- URL host: `blossom.primal.net`.
- Size: 3,666 bytes.
- UI moved to `Downloading` immediately.
- Completion was visible by the next UI poll, under roughly 1 second from click.
- Database recorded a ready media file.
- Hash verification state: `matched`.
- SHA-256 matched the expected source hash.

Because the live file was tiny, the speed number is not a meaningful throughput measure. As a larger network reference, the already-downloaded 15,454,629-byte Blossom PDF fetched with `curl` in 2.30s at about 6.72 MB/s. That measures raw network/server speed, not the app's full DB/hash/write path.

Historical job evidence:

- The older YouTube playlist job downloaded 4 files and ended with `4 downloaded, 0 failed, 0 skipped`.
- The queue has item-level completion timestamps, but not start/end timing per download, so it cannot produce precise throughput.

### Phone Access

Passed:

- Start Phone Access completed in about 0.6s.
- The app selected Wi-Fi/LAN `en0` instead of a VPN/tunnel address.
- Generated QR code and VLC playlist URL.
- Playlist endpoint returned HTTP 200 in 0.0027s with 4 items before the final download.
- Byte-range media request returned HTTP 206 in 0.0034s for a 1 KB range with `video/mp4`.
- Stop Sharing worked and the server was stopped.
- After the live download, the phone panel correctly updated to 5 media.

### Source Control And Storage

Passed:

- Source capability matrix is clear and conservative.
- Added sources show per-source local/missing counts.
- Download buttons disable after all file-backed media is local.
- Trusted curator key is summarized by default with full key behind disclosure.
- Queue display hides full saved paths by default and puts old absolute paths behind `Show saved path`.

Storage scan:

- Scan completed in under 1s.
- Current scan: 16 scanned, 10 referenced, 6 stale, 468.7 MB stale, 3 fragments.
- Stale examples are old YouTube partial/download paths.
- Cleanup was not run because it deletes local files and should be a deliberate user action.

### Channels

Passed:

- Followed channels display trust state, local availability, notification state, manifest URL disclosure, refresh/download/unfollow actions.
- Signed trusted channel: `SPace`, 1/1 local.
- Signed untrusted channel: `Codex Signed Channel Test 2026-06-18`, now 1/1 local after QA download.
- Live provider claims remain conservative and do not overpromise automation.

### Import

Passed:

- Import drawer is now task-mode based: local files, source URL, teacher feed, manifest, trusted keys.
- Drawer uses dialog semantics and modal presentation.
- Source URL mode shows examples and a clear offline-state blocker.
- Enable Fetching immediately enables Subscribe/Ingest.
- Remote fetching was returned to offline mode after testing.

Polish issue:

- The top toggle label `Offline mode` is a current-state label. It is technically correct, but can be read as an action label, and accessibility reports `Value: off`. For nontechnical users, `Remote fetching off` / `Remote fetching on` would be clearer.

### Publishing And Uploading

Verified locally:

- Publish screen loads.
- Existing owned channel `SPace` is visible.
- Existing channel is not published yet: 0 signed items, 0 media, 0 posts, no subscriber link.
- Publisher readiness blocks endpoint testing and publishing until a vault passphrase is entered.
- Unit tests cover publisher vault rejection, endpoint message behavior, Blossom auth, Nostr relay publish/fetch, signed manifests, text posts, hash-safe exports, archive mirror verification, and invite encoding.

Not measured live:

- Actual upload speed.
- Endpoint probe speed.
- Signed publish speed.

Reason:

- The live app requires the local publisher vault passphrase to test endpoints or publish.
- A temporary Rust probe attempt was added and removed, but did not execute network actions: Tauri's default macOS runtime test harness failed before execution because `EventLoop` must be created on the main thread.

Unblock action:

- Either provide the test publisher vault passphrase for a live UI endpoint/publish test, or add a first-class main-thread QA command/script that creates an isolated synthetic publisher profile and runs a public test-only endpoint/publish probe.

## Production Polish Backlog

### P1 - Add Download Throughput And Progress

The queue records completion state but not enough timing/progress metadata to answer "how fast did this download" precisely.

Recommended work:

- Record `started_at`, `completed_at`, `bytes_expected`, `bytes_downloaded`, and final `bytes_per_second` for each download job.
- Show MB/s and elapsed time in Queue rows.
- For direct HTTP downloads, stream progress into the job row instead of showing only `Downloading`.
- Keep source-level summary, but preserve item-level timing for diagnostics.

### P1 - Make Storage Cleanup A Guided Flow

The app found 468.7 MB stale managed-library data. The cleanup capability exists, but the next agent should make this easier and safer.

Recommended work:

- Add a Review Stale Files panel with grouped sources, sizes, and fragment markers.
- Offer separate actions: clean partial fragments, clean duplicate old-source copies, clean all stale.
- Add a pre-clean confirmation with total reclaimable size.
- After cleanup, automatically rerun the audit and show reclaimed bytes.

### P1 - Add A Safe Publishing Speed Probe

Publishing is a core platform promise, but live speed cannot be measured without unlocking a vault.

Recommended work:

- Add a user-approved `Run synthetic publisher probe` flow that creates a temporary key/profile, uploads a tiny text probe to selected Blossom servers, publishes a test-only Nostr event, reports per-endpoint latency, and then deletes local temp state.
- Clearly label that accepted probes are public on third-party endpoints.
- Store last endpoint-test timing on the profile and surface it in Publish readiness.

### P1 - Improve First Publish Path

The Publish page has an owned-channel panel and a composer, but the composer defaults to a new channel. For users trying to update an existing channel, the primary path depends on noticing the `Update` button.

Recommended work:

- If there is exactly one saved unpublished channel, preselect it in the composer or show a stronger `Continue publishing SPace` call to action.
- Show the next missing step in plain language: unlock profile, test endpoints, add post/media, publish.
- After successful publish, immediately show subscriber link, copy invite, and feed-follow test actions.

### P2 - Clarify Offline/Fetching State

`Offline mode` works, and the Import drawer has `Enable Fetching`, but the top toggle label can be misread.

Recommended work:

- Rename the state labels to `Remote fetching off` and `Remote fetching on`.
- Add a tooltip: "Remote source refreshes and subscriptions are blocked while off. Local imports still work."
- Keep offline as default.

### P2 - Merge Or Disambiguate Duplicate Teacher Names

The Teachers panel shows `Codex Test Publisher` twice because two channels share the display name.

Recommended work:

- If curator key matches, merge under one teacher identity.
- If keys differ, show channel or key suffix so duplicate display names do not look accidental.

### P2 - Make Trust Promotion Available From Channel Cards

The signed-untrusted channel is understandable, but the trust action is not visible from the channel card.

Recommended work:

- For signed-untrusted channels with a valid manifest, add `Review Trust` or `Trust Curator` from the channel card.
- Keep the explicit outside-verification language.
- Show why the channel remains untrusted when no key can be promoted.

### P2 - Reduce Persistent Global Notices

The stale-storage notice follows users across Library, Channels, Sources, and Publish. It is useful, but noisy after the user understands it.

Recommended work:

- Convert storage notices into a Source Control badge plus a dismissible global notification.
- Keep high-risk runtime errors global; make hygiene findings view-scoped after dismissal.

### P3 - Add Bright Media Fixtures For Visual QA

The tested video asset plays but appears mostly black, making visual QA less conclusive.

Recommended work:

- Add a tiny, redistributable bright/color-bar MP4 fixture for test imports and playback smoke checks.
- Use it in automated or manual QA so screenshots can prove rendered pixels, not just playback state.

## Handoff Summary

The app is materially healthier than the previous UI audit: search is scoped, import is segmented, queue paths are hidden, first-run work appears implemented, phone access works, and the real missing media item now downloads and verifies.

The strongest production gaps are not basic functionality. They are measurable-operability gaps:

- no precise per-download speed/progress history,
- no safe synthetic publish/upload speed probe,
- stale storage needs a guided cleanup flow,
- release still blocked by external signing/evidence and the known `glib` advisory path.
