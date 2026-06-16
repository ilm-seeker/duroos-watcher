import { sourceAdapters } from "../domain/sourceAdapters";
import type {
  AppSnapshot,
  Collection,
  Job,
  Lesson,
  LiveSession,
  MediaFile,
  ProvenanceRecord,
  Source,
  Teacher,
  TeacherRelay,
  TrustedCurator,
  WatchState,
} from "../domain/types";

const now = "2026-06-16T05:40:00.000Z";
const enabledSeedPlatforms = new Set(["local-files", "telegram", "rss-feed", "archive-org", "teacher-relay"]);

export const seedSources: Source[] = sourceAdapters.map((adapter) => ({
  id: `source-${adapter.platform}`,
  platform: adapter.platform,
  label: adapter.platform === "telegram" ? "Telegram Public Preview" : adapter.label,
  identifier:
    adapter.platform === "local-files"
      ? "App Library"
      : adapter.platform === "telegram"
        ? "https://t.me/s/<channel>"
        : adapter.platform === "rss-feed"
          ? "https://example.com/feed.xml"
        : adapter.platform === "archive-org"
          ? "archive.org/details/<identifier>"
          : adapter.platform === "teacher-relay"
            ? "https://teacher.example/feed.xml"
            : `${adapter.platform}:not-configured`,
  feedFormat:
    adapter.platform === "teacher-relay" ? "duroos-manifest" : "rss",
  feedTransport: "https",
  trustState: "unsigned",
  authMode: adapter.capability.authMode,
  updateSchedule:
    adapter.platform === "local-files"
      ? "Manual"
      : enabledSeedPlatforms.has(adapter.platform)
        ? "Manual + daily check"
        : "Manual until configured",
  capability: adapter.capability,
  enabled: enabledSeedPlatforms.has(adapter.platform),
  lastCheckedAt: enabledSeedPlatforms.has(adapter.platform) ? now : undefined,
}));

export const seedTeachers: Teacher[] = [
  {
    id: "teacher-3",
    displayName: "Personal Library",
    description: "Imported local files on this machine.",
    sourceLinks: [],
  },
];

export const seedCollections: Collection[] = [
  {
    id: "collection-2",
    title: "Local Imports",
    ownerLabel: "Local archive",
    sortOrder: 10,
    lessonCount: 0,
    sourceIds: ["source-local-files"],
  },
];

export const seedTeacherRelays: TeacherRelay[] = [];
export const seedLiveSessions: LiveSession[] = [];
export const seedLessons: Lesson[] = [];
export const seedMediaFiles: MediaFile[] = [];
export const seedProvenance: ProvenanceRecord[] = [];
export const seedWatchState: WatchState[] = [];
export const seedJobs: Job[] = [];
export const seedTrustedCurators: TrustedCurator[] = [];

export const seedSnapshot: AppSnapshot = {
  sources: seedSources,
  teachers: seedTeachers,
  teacherRelays: seedTeacherRelays,
  liveSessions: seedLiveSessions,
  collections: seedCollections,
  lessons: seedLessons,
  mediaFiles: seedMediaFiles,
  provenanceRecords: seedProvenance,
  watchState: seedWatchState,
  jobs: seedJobs,
  trustedCurators: seedTrustedCurators,
};
