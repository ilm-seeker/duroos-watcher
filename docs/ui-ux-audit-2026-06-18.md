# Duroos Watcher UI/UX Audit - 2026-06-18

## Scope

This audit covers the Duroos Watcher Tauri/React desktop app in `/Users/traveler/Documents/Duroos Watcher`. It focuses on product clarity, visual hierarchy, interaction polish, accessibility, first-run comprehension, and future onboarding tutorial requirements.

No UI code changes were made in this pass. This file is a planning and implementation note for future polish work.

## Evidence Used

- Repository baseline: `git status --short` was clean before the audit.
- Code reviewed: `src/App.tsx`, `src/styles.css`, `src/lib/tauri.ts`, `src/data/seed.ts`, `README.md`, and `src-tauri/tauri.conf.json`.
- Browser preview checked at `http://127.0.0.1:1420/` with empty seed data.
- Native Tauri app launched with `npm run tauri dev` and inspected with Computer Use.
- Desktop native state observed: desktop runtime available, `yt-dlp` ready, 4 media items, 4 local files, 0 missing files, 10 historical jobs, and one active YouTube-backed source.
- Responsive browser states checked at desktop width and 390px mobile width. The Tauri config sets `minWidth` to 960, so mobile findings are future web-preview/published-preview concerns, not a current desktop blocker.

One process caveat: launching `npm run tauri dev` while a separate Vite server already used port 1420 caused the extra Vite process to move to port 1421, while Tauri still expects `devUrl` 1420. For future desktop QA, stop the standalone preview before launching Tauri, or use one controlled launch command.

## Product Direction

Duroos Watcher should feel like a quiet local-first study cockpit:

- The learner's primary job is to save, verify, organize, and watch lessons without social-feed distraction.
- The app's trust model should stay visible but compact: provenance, signed manifests, local storage, no accounts, no telemetry.
- Advanced publishing and curator mechanics should exist, but they should not dominate the learner's first experience.
- The UI should use disclosure and task-specific panels instead of broad warnings or large explanatory blocks.

## Scorecard

Scores are based on the observed native desktop app plus browser first-run preview.

| Area | Score | Notes |
| --- | ---: | --- |
| Visual hierarchy | 7/10 | The main player and right-side operational panels establish a useful cockpit. Empty and publishing states flatten into many equal-weight boxes. |
| Originality / non-generic feel | 7/10 | Local-first trust cues, source readiness, and media provenance feel product-specific. Some empty cards and import option tiles still feel template-like. |
| Typography | 7/10 | Strong headings and readable labels. Dense operational copy sometimes drops below a comfortable scanning size. |
| Spacing and rhythm | 6/10 | Good macro grid. Repeated cards and dashed empty states create visual sameness on first run. |
| Color/material coherence | 8/10 | Sober palette works well for a trust-first tool. Status colors are restrained and meaningful. |
| Interaction polish | 5/10 | Several controls have unclear scope or disabled states without enough local explanation. Import and publishing are too all-at-once. |
| Accessibility | 6/10 | Good landmark labels and visible focus styling exist. Drawer semantics, focus management, small chips, and inert button roles need work. |
| Product clarity | 6/10 | The real media library is understandable. First-run, import, queue, and publishing flows need clearer task sequencing. |

Overall: solid architecture-aware product UI, but not yet polished enough for nontechnical first-time users.

## What Works

### Native Library State

The real desktop app state is much stronger than the empty browser seed. With four imported/downloaded lessons, the library immediately communicates:

- What is saved locally.
- Whether downloads are playable.
- Which source provided the lesson.
- That native playback is available through VLC.
- That phone sharing is local Wi-Fi only and user-started.

This is the product's strongest current screen. The player area, lesson cards, source chips, watch progress, provenance panel, notes, and local playback affordances all reinforce the local-first study workflow.

### Source Truth

The Source Control page is unusually honest for this kind of app. It clearly separates:

- Platform capability from user-added sources.
- Stable sources from best-effort or credential-bound sources.
- Metadata support from download support.
- Auth requirements and reliability.
- The "no credentials in exports" promise.

This page should be preserved. It is a trust asset.

### Privacy Framing

The persistent sidebar privacy panel is useful and appropriately compact:

- No accounts.
- No telemetry.
- Local credentials only.

This works better than long disclaimers. Keep this pattern and avoid turning every screen into a policy explanation.

### Local-First Phone Sharing

The Watch on Phone panel explains the model in concrete steps:

1. Start phone access.
2. Scan the code.
3. Open in VLC.

The copy correctly avoids implying cloud sync or remote access.

## Highest Priority Findings

### 1. Queue Exposes Full Local Filesystem Paths

Status: verified in native Tauri app.

The Update Queue shows full saved paths such as `/Users/traveler/Library/Application Support/...` directly in job rows. This is accurate, but it is too much for the main UI and creates privacy risk in screenshots, demos, and support conversations.

Why it matters:

- The app positions itself as privacy-first.
- Full local paths may reveal the macOS username and app storage layout.
- Nontechnical users do not benefit from seeing the full path by default.
- Long paths make job rows visually noisy.

Recommended fix:

- Replace full path text with "Saved in app library" in the primary row.
- Add a secondary "Show path" or "Copy path" affordance for advanced diagnostics.
- Consider a privacy/screenshot mode that always hides local paths, source tokens, and exact filesystem details.

Implementation area:

- `QueueView` and job detail rendering in `src/App.tsx`.

### 2. Global Search Is Visible Outside Its Actual Scope

Status: verified in native Tauri app.

The top search field remains visible on Source Control, Curator Relays, and Update Queue. Setting a query in Update Queue did not filter the visible job list. This creates a scope mismatch.

Why it matters:

- Users expect a visible search box to affect the current screen.
- It is especially confusing on Update Queue, where search would naturally mean searching job history.
- It can make users think results are stale or search is broken.

Recommended fix:

- Option A: make the search field truly view-scoped.
  - Library: search lessons, teachers, collections, sources, URLs.
  - Queue: search job label/detail/source/state.
  - Sources: search platforms, source labels, capability notes, trusted curators.
  - Relays: search publisher fields, feeds, live sessions.
- Option B: show the search field only on Library until the other views support it.
- Rename the accessible label and placeholder by view, for example "Search library" or "Search update history".

Implementation area:

- `TopBar` in `src/App.tsx`.
- View-specific filtering logic near `Dashboard`, `SourcesView`, `QueueView`, and `RelaysView`.

### 3. Source Readiness Rows Are Buttons Even When They Do Nothing

Status: verified by code and native accessibility tree.

`SourceReadinessPanel` renders rows as buttons. In the Library sidebar, this can be useful because rows can filter the library by source. In Source Control, the panel is rendered without `onSelectSource`, so rows can appear as buttons without meaningful action.

Why it matters:

- This is an accessibility and interaction contract issue.
- Keyboard and assistive technology users hear "button" and expect an action.
- Mouse users may click and see no change.

Recommended fix:

- Render a non-interactive row when no `onSelectSource` is supplied.
- Or provide real behavior in Source Control, such as filtering/highlighting the matching matrix row.
- Keep the Library version interactive if source filtering remains useful there.

Implementation area:

- `SourceReadinessPanel` in `src/App.tsx`.

### 4. Import Drawer Is Too Dense For First Use

Status: verified in browser and native Tauri app.

The import drawer currently combines:

- Local file import.
- Source URL ingestion.
- Private source limitations.
- Curator feeds.
- Collection manifest validation.
- Nostr channel preview.
- Manual curator key trust.
- Offline mode explanation.
- Transport reference notes.

This is accurate but cognitively heavy. It reads like a full protocol map rather than a task flow.

Why it matters:

- Import is the main first-run action.
- A learner who just wants to add one local file or a feed has to parse unrelated advanced trust and publishing concepts.
- The source caveat block is long and visually dominant.
- Disabled remote ingest requires the user to understand the global Offline mode toggle.

Recommended fix:

- Split the drawer into task modes with tabs or a segmented control:
  - Local Files.
  - Source URL.
  - Teacher/Curator Feed.
  - Manifest Validation.
  - Trusted Keys.
- Show caveats only inside the relevant mode.
- Move "manual trusted key" behind an advanced disclosure.
- When Offline mode is on, show an inline callout next to the disabled remote action with a direct "Enable fetching" button.
- Add examples directly inside the Source URL field area:
  - `https://archive.org/details/...`
  - `https://example.com/feed.xml`
  - `https://t.me/channel`
  - `naddr1...`
- Keep local file import available even when offline.

Implementation area:

- `ImportDrawer`, `ImportOption`, `ChannelPreviewPanel`, and manual trust panel in `src/App.tsx`.
- Drawer layout classes in `src/styles.css`.

### 5. Import Drawer Needs Dialog Semantics And Focus Management

Status: inferred from code and accessibility inspection.

The drawer uses an `aside` with `aria-label="Import content"` and a fixed backdrop. It does not appear to use `role="dialog"`, `aria-modal="true"`, initial focus placement, Escape-to-close handling, or a focus trap.

Why it matters:

- Keyboard users can tab into background UI while the drawer is visually modal.
- Screen reader users may not get a strong modal context.
- The close button exists, but modal behavior should be explicit.

Recommended fix:

- Use `role="dialog"` and `aria-modal="true"`.
- Move focus to the drawer heading or first useful field on open.
- Return focus to the invoking Import button on close.
- Trap focus inside the drawer while open.
- Support Escape to close.
- Consider using `inert` on the app shell while the drawer is open.

Implementation area:

- `ImportDrawer` in `src/App.tsx`.

### 6. First-Run Empty State Shows Too Many Empty Sections

Status: verified in browser preview with empty seed data.

The first-run Library view includes the empty player panel, empty Continue, empty Feed Inbox, empty Curator Relay Feeds, empty Library Search, empty Watch on Phone, empty Sources, empty Teachers, empty Live Lessons, empty Courses, and empty Updates.

Why it matters:

- The user sees the full app skeleton before they understand the primary action.
- Repeated empty dashed cards make the app feel more unfinished than it is.
- Watch on Phone is not actionable until media exists, so it should not compete with import during first run.

Recommended fix:

- Add a dedicated first-run state when `lessons.length === 0`.
- Show one strong primary path:
  - "Add your first lesson"
  - secondary choices: "Import local files", "Add public feed", "Follow teacher feed"
- Hide or collapse phone access, teacher, course, and update panels until there is relevant data.
- Keep privacy defaults visible but compact.
- After first import, transition to the full dashboard.

Implementation area:

- `Dashboard`, `PlayerEmptyPanel`, right-side panels, and empty section rendering in `src/App.tsx`.

### 7. Teacher Publisher Is A Wall Of Advanced Setup

Status: verified in native Tauri app.

The Teacher Publisher section exposes profile creation, passphrase, Nostr relays, Blossom servers, archive mirrors, IPFS API, IPFS gateway, channel title, notes, media selection, endpoint testing, publish readiness, and publish queue in one large form.

Why it matters:

- This is an advanced workflow with real security and publishing implications.
- It is not a single-step form; it is a setup sequence.
- The current layout makes "what do I need next?" harder than it should be.

Recommended fix:

- Convert Teacher Publisher into a guided setup:
  - Step 1: Create or unlock local publisher profile.
  - Step 2: Add relay and storage endpoints.
  - Step 3: Test endpoints.
  - Step 4: Select media.
  - Step 5: Review signed manifest and mirror choices.
  - Step 6: Publish and copy/share channel link.
- Keep "Archive mirrors are public and may be hard to remove" near the archive mirror step only.
- Show publish readiness as a checklist with exact next actions.
- Separate "learner subscribe" from "teacher publish" in navigation or tabs.

Implementation area:

- `RelaysView` and `TeacherPublisherPanel` in `src/App.tsx`.

### 8. Disabled Actions Need Local Reasons

Status: verified in browser and native states.

Examples:

- Browser first-run: Start Phone Access is disabled with `0 media`, but the disabled button still takes visual space in a prominent panel.
- Offline import: Subscribe/Ingest is disabled, with explanation below, but the relationship is not immediate enough.
- Source Control: Download Media can be disabled when no missing files exist.

Recommended fix:

- Put the reason directly adjacent to disabled actions.
- Use "why disabled" text or a small status row under the button.
- For unavailable-but-important actions, offer the enabling action when possible:
  - "Enable fetching to subscribe."
  - "Import a video or audio file to use phone sharing."
  - "All media is already downloaded."

Implementation area:

- `PhoneAccessPanel`, `ImportDrawer`, `SourcesView`, and `PlayerPanel`.

### 9. Navigation Naming Is Inconsistent

Status: verified in code and UI.

The sidebar says "Curator Relays", while the page title says "Teacher Relays" and the page heading says "Curator Relays".

Why it matters:

- The app has both learner-facing curator subscriptions and teacher publishing.
- Mixed naming makes the mental model less stable.

Recommended fix:

- Pick one primary nav label.
- Suggested nav: "Feeds & Publishing" or "Teacher Feeds".
- Suggested page structure:
  - Learner tab: Follow Teacher Feed.
  - Teacher tab: Publish Channel.
  - Reference tab: Live Providers.

Implementation area:

- `Sidebar`, `viewTitle`, and `RelaysView` in `src/App.tsx`.

## Onboarding Tutorial Plan

The onboarding should not be a generic tour of buttons. It should teach the product's actual model: local library, source truth, review-first downloads, and optional teacher publishing.

### Onboarding Principles

- Accountless and local-first by default.
- No telemetry requirement.
- Skippable and resumable.
- Persist completion locally.
- Avoid full-screen marketing language.
- Teach only the next useful action.
- Use trust cues as small confirmations, not warnings.
- Separate learner onboarding from teacher publisher onboarding.

### Recommended Tutorial Tracks

#### Track A: Learner First Run

Goal: get one playable lesson into the library.

Steps:

1. Choose first import path:
   - Local file.
   - Public feed/source URL.
   - Teacher/curator feed link.
2. Confirm privacy defaults:
   - Files stay in the app library.
   - No account is created.
   - Remote fetching stays off unless enabled.
3. Import or subscribe.
4. Review source result.
5. Download missing media if needed.
6. Play in app or native player.
7. Add a study note or mark complete.

Do not introduce publisher profiles, Nostr relays, Blossom servers, IPFS, or manual trusted keys in this track.

#### Track B: Follow A Teacher Feed

Goal: follow a signed channel or manifest safely.

Steps:

1. Paste feed URL, manifest URL, or `naddr`.
2. Preview channel.
3. Show teacher/curator identity and trust state.
4. Explain signed but untrusted versus trusted.
5. Let the user trust a curator key only after external confirmation.
6. Import lessons as review-first items.

This track should reuse the app's existing manifest-first trust architecture. It should not imply that archive mirrors or IPFS are the source of truth.

#### Track C: Teacher Publisher

Goal: publish a signed channel without a central catalog.

Steps:

1. Create local publisher profile.
2. Explain passphrase and local signing key.
3. Configure Nostr relay and Blossom storage.
4. Test endpoints.
5. Select media.
6. Review archive mirror/public permanence warning.
7. Publish signed manifest.
8. Copy/share `naddr` channel link.

This should be hidden from normal learner onboarding unless the user chooses "I want to publish lessons."

### Suggested First-Run Screen

Use a focused first-run panel in the Library view:

Title: "Build your local study library"

Supporting copy: "Import lessons you are allowed to save, or follow a teacher feed. Duroos stores your library locally and keeps remote fetching under your control."

Primary actions:

- Add Local Files.
- Add Source URL.
- Follow Teacher Feed.

Secondary:

- Learn how Duroos stores media.
- Open sample checklist.

Avoid:

- A full marketing hero.
- Protocol-heavy explanations.
- Fake metrics.
- A generic app tour before the first import.

## Visual Polish Backlog

### P0 - Correctness And Trust Polish

- Hide full local filesystem paths in job rows by default.
- Fix global search scope or hide it outside Library.
- Remove inert button behavior from Source Control readiness rows.
- Add modal dialog semantics and focus handling to Import.
- Add local reasons for disabled actions.

### P1 - First-Run And Import Flow

- Replace repeated empty dashboard sections with a focused first-run state.
- Split Import into task modes.
- Move manual trusted-key entry behind advanced disclosure.
- Add concrete source examples near the URL input.
- Add a direct "Enable fetching" affordance when remote ingest is disabled by Offline mode.

### P1 - Publishing Flow

- Turn Teacher Publisher into a stepper or checklist.
- Separate learner subscription from teacher publishing.
- Make publish readiness actionable, not just diagnostic.
- Keep archive permanence warnings close to archive mirror configuration.

### P2 - Interaction And Accessibility

- Add visible placeholder text to the search field.
- Increase scope chips from 34px to at least 40px height or verify keyboard/touch target requirements for desktop.
- Ensure all icon-only mobile/sidebar buttons have stable accessible names and tooltips.
- Add Escape-to-close and focus return for the import drawer.
- Add `aria-live` only where status changes need announcement; avoid noisy updates.
- Verify keyboard order through Library, Import, Sources, Relays, and Queue.

### P2 - Visual Hierarchy

- Differentiate empty states by purpose instead of repeating dashed boxes.
- Reduce same-weight card repetition in the right column.
- Keep the player as the primary focal point when a lesson exists.
- In first-run state, let the import decision be the primary focal point.
- Make side panels reorder or collapse based on relevance:
  - With no media: Sources and Import next steps first.
  - With media: Watch on Phone and Continue become useful.
  - With active jobs: Queue summary rises.

### P3 - Advanced Polish

- Add "privacy screenshot mode" to hide local paths and long source URLs.
- Add optional sample-library reset for demos and QA.
- Add local-only onboarding progress state.
- Add a command palette later if the app grows more operational screens.

## Specific Screen Notes

### Library

Strengths:

- Real populated state is clear and useful.
- The player cover image creates a strong focal point.
- Source, media state, and provenance are visible.
- Notes and organization editing are colocated with the selected lesson.

Issues:

- Empty state is much weaker than populated state.
- The player empty panel says "Ready for video, audio, and PDFs" but does not make the three import choices clear enough.
- Continue, Feed Inbox, Curator Relay Feeds, and Library Search all render empty at first run, creating a long scroll of non-events.
- The Watch on Phone panel is prominent even when no media is eligible.

Recommended changes:

- Add first-run branching before showing the full dashboard.
- Collapse irrelevant sections when counts are zero.
- Add "Recently imported" or "Needs attention" as the second focal area after the player when content exists.

### Import Drawer

Strengths:

- It supports the product's real source model.
- It truthfully communicates platform limits.
- It keeps local file import available while offline.

Issues:

- Too many import modes appear simultaneously.
- The caveat block is visually heavier than the input and actions.
- Source URL field has no visible example placeholder.
- Manual trust is advanced but always visible.
- Dialog semantics/focus handling need hardening.

Recommended changes:

- Mode split: Local, Source URL, Teacher Feed, Manifest, Trusted Keys.
- Use concise, mode-specific helper text.
- Put remote/offline state directly beside remote actions.
- Treat manual trusted-key entry as advanced.

### Source Control

Strengths:

- The capability matrix is one of the clearest trust surfaces in the app.
- Reliability and auth labels are useful.
- Storage audit belongs here.

Issues:

- Source readiness rows can be interactive without action.
- The matrix is dense and may require horizontal scrolling in narrow views.
- Added source actions need stronger disabled-state explanations.

Recommended changes:

- Fix button semantics.
- Add row detail expansion for platform notes instead of relying only on dense text.
- Add "what this means" help for `Limited`, `Best effort`, and `Needs cookies`.

### Update Queue

Strengths:

- Job history is source-aware and useful.
- Success and unsupported states are visible.
- The copy is more truthful than most media apps.

Issues:

- Full local paths are too exposed.
- Long unsupported errors read like developer logs.
- Global search does not filter jobs.

Recommended changes:

- Collapse paths and developer-level errors behind reveal buttons.
- Add filters: All, Running, Failed, Unsupported, Downloaded.
- Add job search if the top search remains visible.
- Add "Retry", "Open source", or "Repair" actions where appropriate.

### Feeds And Publishing

Strengths:

- The page preserves the no-central-catalog model.
- Live-provider support is truthful.
- Archive mirror warning is appropriately cautious.

Issues:

- Naming alternates between Teacher Relays and Curator Relays.
- Teacher publisher setup is too much for one screen.
- Learner subscription and teacher publishing are competing jobs.

Recommended changes:

- Rename and split the page into learner and teacher modes.
- Use a guided checklist for publishing.
- Keep advanced transport details available, but not foregrounded before profile setup.

### Responsive / Mobile Preview

Status: not a current desktop blocker because Tauri `minWidth` is 960.

Findings:

- At 390px, navigation becomes compact and mechanically usable.
- The dashboard becomes very long because every empty section stacks.
- The import drawer becomes full width and scrollable, but the first viewport still contains too much explanatory content before the user reaches all actions.
- Scope chips wrap acceptably, but at 34px height they are tight.

Recommended changes before any public web/mobile preview:

- Use the first-run state to reduce vertical empty content.
- Keep import actions visible above long helper text.
- Consider a bottom action bar inside the drawer on mobile.
- Keep icon-only navigation labels accessible and provide tooltips on desktop narrow widths.

## Implementation Notes By File

`src/App.tsx`

- `TopBar`: make search view-scoped or Library-only.
- `Dashboard`: add first-run state and relevance-based side panel ordering.
- `ImportDrawer`: split modes, add dialog semantics, focus trap, Escape handling, examples, advanced disclosure.
- `SourceReadinessPanel`: render static rows when no action is supplied.
- `QueueView`: hide full local paths by default; add filters/search; collapse raw errors.
- `RelaysView` and `TeacherPublisherPanel`: split learner subscription from teacher publishing and use a guided setup sequence.

`src/styles.css`

- Tune scope-chip/button heights.
- Add drawer mobile action positioning if needed.
- Reduce repeated same-weight empty-card treatment.
- Preserve the restrained palette; avoid decorative blobs, heavy gradients, or generic SaaS hero styling.

`src/data/seed.ts`

- Consider a first-run demo fixture or a controlled empty fixture for visual QA. The browser preview currently shows empty state while the native app may load real local data.

`src-tauri/tauri.conf.json`

- Current desktop minimum width is 960. Treat sub-960 responsive behavior as future-proofing unless the app is later published as a web/mobile surface.

## Suggested Validation For Future UI Changes

For the next polish implementation pass, validate:

- `npm run build`
- `npm test`
- `git diff --check`
- Browser preview at desktop width for first-run empty state.
- Native Tauri app at 1280x840 with existing data.
- Keyboard tab order through Library and Import.
- Import drawer open/close/focus return.
- Search behavior per view.
- Source Control button semantics with screen reader/accessibility tree.
- Queue rows with local paths hidden by default.

## Bottom Line

The app already has a credible local-first product shape. The biggest UX risk is not lack of features; it is exposing too much system detail and too many advanced concepts before the user has completed a first useful action. Prioritize scoped search, hidden local paths, true button semantics, import progressive disclosure, and first-run onboarding before broader visual restyling.
