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

export type LiveProvider = "youtube-live" | "mixlr" | "paltalk" | "custom-rtmp";
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

export interface RuntimeDiagnostics {
  desktopRuntimeAvailable: boolean;
  ytDlpAvailable: boolean;
  ytDlpVersion?: string;
  ytDlpCommand?: string;
  ytDlpCookiesConfigured: boolean;
  ytDlpCookiesFile?: string;
  messages: string[];
}
