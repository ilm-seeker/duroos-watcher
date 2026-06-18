import { describe, expect, it } from "vitest";
import { buildLibraryLessonView } from "./libraryView";
import type {
  Collection,
  Lesson,
  MediaFile,
  Source,
  Teacher,
  TeacherRelay,
  TrustedCurator,
  WatchState,
} from "./types";

const teacher: Teacher = {
  id: "teacher-test",
  displayName: "Test Teacher",
  sourceLinks: [],
};

const collection: Collection = {
  id: "collection-test",
  title: "Foundations",
  ownerLabel: "Local",
  sortOrder: 1,
  lessonCount: 2,
  sourceIds: ["source-test"],
};

const source: Source = {
  id: "source-test",
  platform: "archive-org",
  label: "Archive Source",
  identifier: "https://archive.org/details/test",
  feedFormat: "json-feed",
  feedTransport: "https",
  trustState: "unsigned",
  authMode: "none",
  updateSchedule: "Manual",
  capability: {
    metadata: "supported",
    download: "supported",
    autoUpdate: "limited",
    authRequired: false,
    authMode: "none",
    reliability: "stable",
    note: "Test source",
  },
  enabled: true,
};

const lesson = (id: string, title: string): Lesson => ({
  id,
  title,
  contentType: "audio",
  teacherId: teacher.id,
  collectionId: collection.id,
  sourceId: source.id,
  sourceUrl: `https://example.test/${id}`,
  thumbnailTone: "emerald",
  provenanceId: `prov-${id}`,
});

const mediaFile = (lessonId: string): MediaFile => ({
  id: `media-${lessonId}`,
  lessonId,
  relativePath: `${lessonId}.mp3`,
  contentHash: "sha256:test",
  sizeBytes: 42,
  importStatus: "ready",
  hashVerificationState: "matched",
});

const watch = (lessonId: string, completed = false): WatchState => ({
  lessonId,
  progressSeconds: 120,
  completed,
});

const view = (overrides: Partial<Parameters<typeof buildLibraryLessonView>[0]> = {}) =>
  buildLibraryLessonView({
    query: "",
    selectedLessonId: "lesson-one",
    lessons: [lesson("lesson-one", "Opening Class"), lesson("lesson-two", "Closing Class")],
    teachers: [teacher],
    collections: [collection],
    sources: [source],
    mediaFiles: [mediaFile("lesson-two")],
    watchState: [watch("lesson-one"), watch("lesson-two")],
    ...overrides,
  });

describe("buildLibraryLessonView", () => {
  it("returns search-specific empty state data when no lessons match", () => {
    const result = view({ query: "no matching lesson" });

    expect(result.isSearchActive).toBe(true);
    expect(result.filteredLessons).toEqual([]);
    expect(result.selectedLesson).toBeUndefined();
    expect(result.continueLessons).toEqual([]);
    expect(result.newLessons).toEqual([]);
  });

  it("scopes selected, continue, and new lesson rows to active search results", () => {
    const result = view({
      query: "closing",
      selectedLessonId: "lesson-one",
      watchState: [watch("lesson-one"), watch("lesson-two")],
    });

    expect(result.filteredLessons.map((item) => item.id)).toEqual(["lesson-two"]);
    expect(result.selectedLesson?.id).toBe("lesson-two");
    expect(result.continueLessons.map((item) => item.id)).toEqual(["lesson-two"]);
    expect(result.newLessons.map((item) => item.id)).toEqual(["lesson-two"]);
  });

  it("keeps the full library scope when search is blank", () => {
    const result = view({ query: "   " });

    expect(result.isSearchActive).toBe(false);
    expect(result.filteredLessons.map((item) => item.id)).toEqual([
      "lesson-one",
      "lesson-two",
    ]);
    expect(result.selectedLesson?.id).toBe("lesson-one");
    expect(result.continueLessons.map((item) => item.id)).toEqual([
      "lesson-one",
      "lesson-two",
    ]);
  });

  it("filters by smart scope before selecting a lesson", () => {
    const result = view({
      activeScopeId: "completed",
      selectedLessonId: "lesson-one",
      watchState: [watch("lesson-one"), watch("lesson-two", true)],
    });

    expect(result.filteredLessons.map((item) => item.id)).toEqual(["lesson-two"]);
    expect(result.selectedLesson?.id).toBe("lesson-two");
    expect(result.smartScopes.find((scope) => scope.id === "completed")?.count).toBe(1);
  });

  it("finds file-backed lessons that still need local media", () => {
    const result = view({ activeScopeId: "needs-files" });

    expect(result.filteredLessons.map((item) => item.id)).toEqual(["lesson-one"]);
    expect(result.smartScopes.find((scope) => scope.id === "needs-files")?.count).toBe(1);
  });

  it("builds teacher and collection groups with active and completed counts", () => {
    const result = view({
      watchState: [watch("lesson-one"), watch("lesson-two", true)],
    });

    expect(result.teacherGroups).toEqual([
      {
        id: teacher.id,
        label: teacher.displayName,
        lessonCount: 2,
        activeCount: 1,
        completedCount: 1,
      },
    ]);
    expect(result.collectionGroups[0]).toMatchObject({
      id: collection.id,
      label: collection.title,
      lessonCount: 2,
      activeCount: 1,
      completedCount: 1,
    });
  });

  it("combines teacher and collection filters with search", () => {
    const otherTeacher: Teacher = {
      id: "teacher-other",
      displayName: "Other Teacher",
      sourceLinks: [],
    };
    const otherCollection: Collection = {
      id: "collection-other",
      title: "Other Course",
      ownerLabel: "Local",
      sortOrder: 2,
      lessonCount: 1,
      sourceIds: ["source-test"],
    };
    const otherLesson: Lesson = {
      ...lesson("lesson-three", "Opening Class"),
      teacherId: otherTeacher.id,
      collectionId: otherCollection.id,
    };

    const result = view({
      query: "opening",
      selectedTeacherId: otherTeacher.id,
      selectedCollectionId: otherCollection.id,
      lessons: [lesson("lesson-one", "Opening Class"), otherLesson],
      teachers: [teacher, otherTeacher],
      collections: [collection, otherCollection],
    });

    expect(result.filteredLessons.map((item) => item.id)).toEqual(["lesson-three"]);
  });

  it("filters by source and returns source groups", () => {
    const otherSource: Source = {
      ...source,
      id: "source-other",
      label: "Other Source",
    };
    const otherLesson: Lesson = {
      ...lesson("lesson-three", "Outside Class"),
      sourceId: otherSource.id,
    };

    const result = view({
      selectedSourceId: otherSource.id,
      lessons: [lesson("lesson-one", "Opening Class"), otherLesson],
      sources: [source, otherSource],
    });

    expect(result.filteredLessons.map((item) => item.id)).toEqual(["lesson-three"]);
    expect(result.sourceGroups.map((group) => group.id)).toEqual([
      source.id,
      otherSource.id,
    ]);
  });

  it("derives subscribed channel status from relays, source rows, lessons, media, and curator trust", () => {
    const channelSource: Source = {
      ...source,
      id: "source-channel",
      platform: "teacher-relay",
      label: "Channel: Foundations",
      identifier: "https://teacher.example/duroos.json",
      trustedCuratorId: "curator-foundations",
      lastCheckedAt: "2026-06-18T10:00:00.000Z",
    };
    const relay: TeacherRelay = {
      id: "relay-foundations",
      teacherId: teacher.id,
      title: "Foundations",
      description: "Signed classes and notes.",
      feedUrl: channelSource.identifier,
      feedFormat: "duroos-manifest",
      feedTransport: "nostr",
      trustState: "signed-untrusted",
      trustPolicy: "manual-review",
      visibility: "public",
      autoDownload: true,
      lastPublishedAt: "2026-06-17T10:00:00.000Z",
      subscriberCount: 12,
    };
    const curator: TrustedCurator = {
      id: "curator-foundations",
      displayName: "Foundations Curator",
      publicKey: "npub1test",
      trustNote: "Known local source.",
      addedAt: "2026-06-18T08:00:00.000Z",
    };
    const postLesson: Lesson = {
      ...lesson("lesson-post", "Source Note"),
      contentType: "post",
      sourceId: channelSource.id,
      publishedAt: "2026-06-18T11:00:00.000Z",
    };
    const missingLesson: Lesson = {
      ...lesson("lesson-missing", "Needed Audio"),
      sourceId: channelSource.id,
      publishedAt: "2026-06-18T09:00:00.000Z",
    };
    const readyLesson: Lesson = {
      ...lesson("lesson-ready", "Ready Audio"),
      sourceId: channelSource.id,
      publishedAt: "2026-06-17T09:00:00.000Z",
    };

    const result = view({
      sources: [source, channelSource],
      teacherRelays: [relay],
      trustedCurators: [curator],
      lessons: [postLesson, missingLesson, readyLesson],
      mediaFiles: [mediaFile(readyLesson.id)],
    });

    expect(result.channelSubscriptions).toEqual([
      expect.objectContaining({
        id: channelSource.id,
        sourceId: channelSource.id,
        relayId: relay.id,
        title: "Foundations",
        curatorLabel: teacher.displayName,
        itemCount: 3,
        localFileCount: 1,
        missingFileCount: 1,
        postCount: 1,
        latestUpdateAt: postLesson.publishedAt,
        trusted: true,
      }),
    ]);
  });
});
