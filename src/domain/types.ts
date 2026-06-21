export type SourcePlatform =
  | "local-files"
  | "telegram"
  | "rss-feed"
  | "archive-org"
  | "youtube"
  | "x"
  | "rumble"
  | "odysee"
  | "teacher-relay";

export type CapabilityLevel = "native" | "supported" | "limited" | "blocked";

export type Reliability = "stable" | "good" | "best-effort" | "credential-bound";

export type AuthMode = "none" | "local-keychain" | "api-key" | "user-session";

export type FeedFormat = "rss" | "atom" | "json-feed" | "duroos-manifest";

export type FeedTransport = "https" | "nostr";

export type TrustState =
  | "unsigned"
  | "signed-untrusted"
  | "signed-trusted"
  | "tampered";

export type HashVerificationState =
  | "unverified"
  | "matched"
  | "mismatched"
  | "not-provided";

export type JobKind =
  | "import"
  | "refresh"
  | "download"
  | "metadata"
  | "publish"
  | "recording";

export type JobState =
  | "queued"
  | "running"
  | "found"
  | "downloaded"
  | "skipped"
  | "needs-permission"
  | "failed-auth"
  | "unsupported"
  | "failed"
  | "live"
  | "archived";

export type ContentType = "video" | "audio" | "pdf" | "post";

export interface SourceCapability {
  metadata: CapabilityLevel;
  download: CapabilityLevel;
  autoUpdate: CapabilityLevel;
  authRequired: boolean;
  authMode: AuthMode;
  reliability: Reliability;
  note: string;
}

export interface Source {
  id: string;
  platform: SourcePlatform;
  label: string;
  identifier: string;
  feedFormat: FeedFormat;
  feedTransport: FeedTransport;
  trustState: TrustState;
  authMode: AuthMode;
  updateSchedule: string;
  capability: SourceCapability;
  enabled: boolean;
  lastCheckedAt?: string;
  trustedCuratorId?: string;
  lastVerifiedAt?: string;
}

export interface Teacher {
  id: string;
  displayName: string;
  description?: string;
  sourceLinks: string[];
}

export type RelayVisibility = "private" | "unlisted" | "public";
export type RelayTrustPolicy = "signed-feed" | "manual-review" | "teacher-managed";

export interface TeacherRelay {
  id: string;
  teacherId: string;
  title: string;
  feedUrl: string;
  feedFormat: FeedFormat;
  feedTransport: FeedTransport;
  trustState: TrustState;
  subscriberCount: number;
  visibility: RelayVisibility;
  trustPolicy: RelayTrustPolicy;
  autoDownload: boolean;
  lastPublishedAt?: string;
  lastVerifiedAt?: string;
  description?: string;
}

export type LiveProvider = "youtube-live" | "mixlr" | "custom-rtmp";
export type LiveSessionStatus =
  | "scheduled"
  | "live"
  | "recording"
  | "processing"
  | "archived"
  | "manual-import";

export interface LiveSession {
  id: string;
  teacherId: string;
  relayId: string;
  title: string;
  provider: LiveProvider;
  providerUrl: string;
  status: LiveSessionStatus;
  startsAt: string;
  archiveLessonId?: string;
  autoPublishArchive: boolean;
  recordingPolicy: string;
}

export interface Collection {
  id: string;
  title: string;
  ownerLabel: string;
  sortOrder: number;
  lessonCount: number;
  sourceIds: string[];
}

export interface Lesson {
  id: string;
  title: string;
  contentType: ContentType;
  teacherId: string;
  collectionId: string;
  sourceId: string;
  sourceUrl: string;
  publishedAt?: string;
  description?: string;
  thumbnailTone: "slate" | "emerald" | "amber" | "blue" | "rose";
  durationSeconds?: number;
  mediaFileId?: string;
  provenanceId: string;
}

export interface MediaFile {
  id: string;
  lessonId: string;
  relativePath: string;
  thumbnailRelativePath?: string;
  contentHash: string;
  sizeBytes: number;
  codec?: string;
  importStatus: "ready" | "copying" | "missing" | "failed";
  hashVerificationState: HashVerificationState;
}

export interface ProvenanceRecord {
  id: string;
  lessonId: string;
  originUrl: string;
  permissionNote: string;
  importedAt: string;
  adapterName: string;
  contentHash?: string;
}

export interface WatchState {
  lessonId: string;
  progressSeconds: number;
  completed: boolean;
  lastWatchedAt?: string;
}

export interface LessonNote {
  lessonId: string;
  body: string;
  updatedAt: string;
}

export interface Job {
  id: string;
  kind: JobKind;
  state: JobState;
  sourceId?: string;
  lessonId?: string;
  label: string;
  detail: string;
  retryCount: number;
  updatedAt: string;
  startedAt?: string;
  completedAt?: string;
  bytesExpected?: number;
  bytesDownloaded?: number;
  bytesPerSecond?: number;
  elapsedMs?: number;
}

export interface TrustedCurator {
  id: string;
  displayName: string;
  publicKey: string;
  trustNote?: string;
  addedAt: string;
}

export interface TrustCuratorSummary {
  curatorId: string;
  displayName: string;
  publicKey: string;
  sourcesUpdated: number;
  messages: string[];
}

export interface AppSnapshot {
  sources: Source[];
  teachers: Teacher[];
  teacherRelays: TeacherRelay[];
  liveSessions: LiveSession[];
  collections: Collection[];
  lessons: Lesson[];
  mediaFiles: MediaFile[];
  provenanceRecords: ProvenanceRecord[];
  watchState: WatchState[];
  lessonNotes: LessonNote[];
  jobs: Job[];
  trustedCurators: TrustedCurator[];
}

export interface ImportSummary {
  imported: number;
  skipped: number;
  failed: number;
  messages: string[];
}

export interface IngestSummary extends ImportSummary {
  sourceUrl: string;
  discovered: number;
}

export interface ClearSourceSummary {
  sourceId: string;
  sourceLabel: string;
  removedSource: boolean;
  lessonsRemoved: number;
  mediaFilesRemoved: number;
  jobsRemoved: number;
  messages: string[];
}

export interface DownloadSourceSummary {
  sourceId: string;
  sourceLabel: string;
  attempted: number;
  downloaded: number;
  skipped: number;
  failed: number;
  messages: string[];
}

export interface MediaStorageAudit {
  scannedFiles: number;
  referencedFiles: number;
  staleFiles: number;
  staleBytes: number;
  partialFiles: number;
  staleSamples: string[];
  staleItems: MediaStorageStaleItem[];
  messages: string[];
}

export type MediaStorageCleanupMode =
  | "partial-fragments"
  | "old-source-downloads"
  | "all-stale";

export interface MediaStorageStaleItem {
  relativePath: string;
  sizeBytes: number;
  category: "partial-fragment" | "old-source-download" | "unreferenced";
}

export interface MediaStorageCleanupRequest {
  mode: MediaStorageCleanupMode;
}

export interface MediaStorageCleanup {
  audit: MediaStorageAudit;
  mode: MediaStorageCleanupMode;
  removedFiles: number;
  failedRemovals: number;
  reclaimedBytes: number;
  messages: string[];
}

export interface NativePlaybackResult {
  mediaFileId: string;
  lessonId: string;
  title: string;
  playerName: string;
  commandLabel: string;
  launched: boolean;
  messages: string[];
}

export interface OpenMediaResult {
  mediaFileId: string;
  lessonId: string;
  title: string;
  opened: boolean;
  messages: string[];
}

export interface RuntimeDiagnostics {
  desktopRuntimeAvailable: boolean;
  ytDlpAvailable: boolean;
  ytDlpVersion?: string;
  ytDlpCommand?: string;
  requiredMediaToolsAvailable: boolean;
  mediaToolSource: "bundled" | "system" | "mixed" | "missing";
  missingMediaTools: string[];
  nativePlaybackAvailable: boolean;
  nativePlaybackPlayer?: string;
  nativePlaybackCommand?: string;
  ytDlpCookiesConfigured: boolean;
  ytDlpCookiesFile?: string;
  messages: string[];
}

export interface PhoneMediaScope {
  sourceId?: string;
  collectionId?: string;
}

export interface PhoneMediaShareItem {
  mediaFileId: string;
  lessonId: string;
  title: string;
  contentType: "video" | "audio";
  sizeBytes: number;
  durationSeconds?: number;
  teacherLabel?: string;
  collectionTitle?: string;
}

export interface PhoneMediaEndpoint {
  label: string;
  host: string;
  kind: "lan" | "vpn" | "tor" | "loopback" | "other";
  baseUrl: string;
  playlistUrl: string;
  preferred: boolean;
  warning?: string;
}

export interface PhoneMediaSession {
  id: string;
  active: boolean;
  baseUrl?: string;
  playlistUrl?: string;
  endpoints?: PhoneMediaEndpoint[];
  startedAt?: string;
  itemCount: number;
  items: PhoneMediaShareItem[];
  messages: string[];
}

export interface NostrRelayConfig {
  url: string;
}

export interface BlossomServerConfig {
  url: string;
}

export interface ArchiveMirrorConfig {
  service: string;
  url: string;
  gatewayUrl?: string;
  label?: string;
}

export interface PublisherProfile {
  id: string;
  displayName: string;
  curatorPublicKey: string;
  nostrPubkey: string;
  relays: NostrRelayConfig[];
  blossomServers: BlossomServerConfig[];
  createdAt: string;
  updatedAt: string;
  vaultConfigured: boolean;
  lastEndpointTestedAt?: string;
  lastEndpointTestPassed?: boolean;
  lastEndpointTestSummary?: string;
}

export interface PublisherChannel {
  id: string;
  profileId: string;
  title: string;
  description?: string;
  channelIdentifier: string;
  naddr?: string;
  canonicalChannelLink?: string;
  lastManifestSha256?: string;
  lastManifestUrl?: string;
  lastPublishedAt?: string;
  mediaCount: number;
  postCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface PublishedChannelItem {
  id: string;
  channelId: string;
  itemType: string;
  title: string;
  contentType: string;
  description?: string;
  originUrl: string;
  retrievalUrl?: string;
  sha256: string;
  sizeBytes?: number;
  mimeType?: string;
  publishedAt: string;
}

export interface CreatePublisherProfileRequest {
  displayName: string;
  passphrase: string;
  relays: NostrRelayConfig[];
  blossomServers: BlossomServerConfig[];
}

export interface SavePublisherChannelRequest {
  profileId: string;
  channelId?: string;
  channelTitle: string;
  channelDescription?: string;
}

export interface PublishedLessonDraft {
  title: string;
  contentType: "video" | "audio" | "pdf";
  path: string;
  description?: string;
}

export interface PublishedPostDraft {
  title: string;
  body: string;
}

export interface PublishTeacherChannelRequest {
  profileId: string;
  channelId?: string;
  passphrase: string;
  channelTitle: string;
  channelDescription?: string;
  relays: NostrRelayConfig[];
  blossomServers: BlossomServerConfig[];
  archiveMirrors?: ArchiveMirrorConfig[];
  lessons: PublishedLessonDraft[];
  posts?: PublishedPostDraft[];
}

export interface PublisherEndpointTestRequest {
  profileId: string;
  passphrase: string;
  relays: NostrRelayConfig[];
  blossomServers: BlossomServerConfig[];
}

export interface SyntheticPublisherProbeRequest {
  relays: NostrRelayConfig[];
  blossomServers: BlossomServerConfig[];
  confirmPublicProbe: boolean;
}

export interface BlossomUploadResult {
  serverUrl: string;
  hash: string;
  url?: string;
  uploaded: boolean;
  elapsedMs?: number;
  bytesPerSecond?: number;
  message: string;
}

export interface NostrRelayPublishResult {
  relayUrl: string;
  accepted: boolean;
  elapsedMs?: number;
  message: string;
}

export interface ArchiveMirrorResult {
  service: string;
  endpointUrl: string;
  url?: string;
  cid?: string;
  archived: boolean;
  verified: boolean;
  message: string;
}

export interface ChannelPublishResult {
  channelId: string;
  channelTitle: string;
  naddr: string;
  canonicalChannelLink: string;
  inviteText: string;
  verificationCode: string;
  manifestJson: string;
  manifestSha256: string;
  manifestUrl: string;
  nostrEventId: string;
  blossomResults: BlossomUploadResult[];
  archiveResults: ArchiveMirrorResult[];
  relayResults: NostrRelayPublishResult[];
  mediaCount: number;
  postCount: number;
  totalItemCount: number;
  messages: string[];
}

export interface PublisherEndpointTestReport {
  passed: boolean;
  synthetic: boolean;
  testedAt: string;
  blossomResults: BlossomUploadResult[];
  relayResults: NostrRelayPublishResult[];
  messages: string[];
}

export interface NostrChannelPreview {
  naddr: string;
  manifestUrl: string;
  manifestSha256: string;
  title: string;
  curatorDisplayName: string;
  curatorPublicKey?: string;
  trustState: TrustState;
  publishedAt?: string;
  lessonCount: number;
  mediaCount: number;
  relayCount: number;
  blossomServerCount: number;
  archiveMirrorCount: number;
  relays: string[];
  blossomServers: string[];
  archiveMirrors: string[];
  warnings: string[];
  messages: string[];
}
