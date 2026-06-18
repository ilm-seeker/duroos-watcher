import type {
  Collection,
  ContentType,
  Lesson,
  MediaFile,
  Source,
  Teacher,
  TeacherRelay,
  TrustedCurator,
  WatchState,
} from "./types";

export type SmartScopeId =
  | "all"
  | "continue"
  | "unwatched"
  | "completed"
  | "needs-files"
  | "recent";

export type LibraryContentTypeFilter = "all" | ContentType;

export type LibraryAvailabilityFilter = "all" | "local" | "needs-files" | "saved-text";

export interface SmartScopeOption {
  id: SmartScopeId;
  label: string;
  count: number;
}

export interface LibraryGroup {
  id: string;
  label: string;
  lessonCount: number;
  activeCount: number;
  completedCount: number;
}

export interface ChannelSubscriptionView {
  id: string;
  relayId: string;
  teacherId: string;
  sourceId?: string;
  title: string;
  curatorLabel: string;
  description?: string;
  feedUrl: string;
  feedFormat: string;
  trustState: TeacherRelay["trustState"];
  trustPolicy: TeacherRelay["trustPolicy"];
  visibility: TeacherRelay["visibility"];
  autoDownload: boolean;
  itemCount: number;
  localFileCount: number;
  missingFileCount: number;
  postCount: number;
  latestUpdateAt?: string;
  latestPublishedAt?: string;
  latestCheckedAt?: string;
  trusted: boolean;
}

export interface LibraryLessonViewInput {
  query: string;
  selectedLessonId: string;
  activeScopeId?: SmartScopeId;
  selectedTeacherId?: string;
  selectedCollectionId?: string;
  selectedSourceId?: string;
  selectedChannelId?: string;
  selectedContentType?: LibraryContentTypeFilter;
  selectedAvailability?: LibraryAvailabilityFilter;
  lessons: Lesson[];
  teachers: Teacher[];
  collections: Collection[];
  sources: Source[];
  mediaFiles: MediaFile[];
  teacherRelays?: TeacherRelay[];
  trustedCurators?: TrustedCurator[];
  watchState: WatchState[];
}

export interface LibraryLessonView {
  isSearchActive: boolean;
  filteredLessons: Lesson[];
  selectedLesson?: Lesson;
  continueLessons: Lesson[];
  newLessons: Lesson[];
  smartScopes: SmartScopeOption[];
  teacherGroups: LibraryGroup[];
  collectionGroups: LibraryGroup[];
  sourceGroups: LibraryGroup[];
  contentTypeGroups: LibraryGroup[];
  availabilityGroups: LibraryGroup[];
  channelSubscriptions: ChannelSubscriptionView[];
}

export const buildLibraryLessonView = ({
  query,
  selectedLessonId,
  activeScopeId = "all",
  selectedTeacherId = "all",
  selectedCollectionId = "all",
  selectedSourceId = "all",
  selectedChannelId = "all",
  selectedContentType = "all",
  selectedAvailability = "all",
  lessons,
  teachers,
  collections,
  sources,
  mediaFiles,
  teacherRelays = [],
  trustedCurators = [],
  watchState,
}: LibraryLessonViewInput): LibraryLessonView => {
  const normalizedQuery = query.trim().toLowerCase();
  const isSearchActive = normalizedQuery.length > 0;
  const teacherById = new Map(teachers.map((teacher) => [teacher.id, teacher]));
  const collectionById = new Map(
    collections.map((collection) => [collection.id, collection]),
  );
  const sourceById = new Map(sources.map((source) => [source.id, source]));
  const mediaByLessonId = new Map(mediaFiles.map((file) => [file.lessonId, file]));
  const watchByLessonId = new Map(watchState.map((state) => [state.lessonId, state]));
  const channelSubscriptions = buildChannelSubscriptions({
    teacherRelays,
    teachers,
    sources,
    lessons,
    mediaFiles,
    trustedCurators,
  });
  const channelByFilterId = buildChannelFilterIndex(channelSubscriptions);
  const selectedChannel = channelByFilterId.get(selectedChannelId);
  const baseLessons = lessons.filter(
    (lesson) =>
      (selectedTeacherId === "all" || lesson.teacherId === selectedTeacherId) &&
      (selectedCollectionId === "all" || lesson.collectionId === selectedCollectionId) &&
      (selectedSourceId === "all" || lesson.sourceId === selectedSourceId) &&
      lessonMatchesChannel(lesson, selectedChannelId, selectedChannel) &&
      (selectedContentType === "all" || lesson.contentType === selectedContentType) &&
      lessonMatchesAvailability(lesson, selectedAvailability, mediaByLessonId),
  );

  const searchedLessons = isSearchActive
    ? baseLessons.filter((lesson) =>
        [
          lesson.title,
          lesson.description,
          lesson.sourceUrl,
          contentTypeLabel(lesson.contentType),
          availabilityLabel(availabilityForLesson(lesson, mediaByLessonId)),
          teacherById.get(lesson.teacherId)?.displayName,
          collectionById.get(lesson.collectionId)?.title,
          sourceById.get(lesson.sourceId)?.label,
        ].some((value) => value?.toLowerCase().includes(normalizedQuery)),
      )
    : baseLessons;

  const smartScopes: SmartScopeOption[] = [
    { id: "all", label: "All", count: searchedLessons.length },
    {
      id: "continue",
      label: "Continue",
      count: searchedLessons.filter((lesson) => isInProgress(lesson, watchByLessonId)).length,
    },
    {
      id: "unwatched",
      label: "Unwatched",
      count: searchedLessons.filter((lesson) => isUnwatched(lesson, watchByLessonId)).length,
    },
    {
      id: "completed",
      label: "Completed",
      count: searchedLessons.filter((lesson) => watchByLessonId.get(lesson.id)?.completed).length,
    },
    {
      id: "needs-files",
      label: "Needs Files",
      count: searchedLessons.filter((lesson) => needsLocalFile(lesson, mediaByLessonId)).length,
    },
    { id: "recent", label: "Recent", count: searchedLessons.length },
  ];

  const scopedLessons = orderScopedLessons(
    searchedLessons.filter((lesson) =>
      lessonMatchesScope(lesson, activeScopeId, mediaByLessonId, watchByLessonId),
    ),
    activeScopeId,
    watchByLessonId,
  );

  const selectedLesson =
    scopedLessons.find((lesson) => lesson.id === selectedLessonId) ?? scopedLessons[0];

  return {
    isSearchActive,
    filteredLessons: scopedLessons,
    selectedLesson,
    continueLessons: scopedLessons
      .filter((lesson) => isInProgress(lesson, watchByLessonId))
      .slice(0, 4),
    newLessons: scopedLessons
      .filter((lesson) => isUnwatched(lesson, watchByLessonId))
      .concat(scopedLessons.filter((lesson) => !isUnwatched(lesson, watchByLessonId)).slice(0, 2))
      .slice(0, 4),
    smartScopes,
    teacherGroups: buildGroups(lessons, teachers, watchByLessonId, "teacher"),
    collectionGroups: buildGroups(lessons, collections, watchByLessonId, "collection"),
    sourceGroups: buildGroups(lessons, sources, watchByLessonId, "source"),
    contentTypeGroups: buildContentTypeGroups(lessons, watchByLessonId),
    availabilityGroups: buildAvailabilityGroups(lessons, mediaByLessonId, watchByLessonId),
    channelSubscriptions,
  };
};

const isInProgress = (
  lesson: Lesson,
  watchByLessonId: Map<string, WatchState>,
): boolean => {
  const progress = watchByLessonId.get(lesson.id);
  return Boolean(progress && progress.progressSeconds > 0 && !progress.completed);
};

const isUnwatched = (
  lesson: Lesson,
  watchByLessonId: Map<string, WatchState>,
): boolean => {
  const progress = watchByLessonId.get(lesson.id);
  return !progress || progress.progressSeconds <= 0;
};

const needsLocalFile = (
  lesson: Lesson,
  mediaByLessonId: Map<string, MediaFile>,
): boolean =>
  lesson.contentType !== "post" && mediaByLessonId.get(lesson.id)?.importStatus !== "ready";

const availabilityForLesson = (
  lesson: Lesson,
  mediaByLessonId: Map<string, MediaFile>,
): Exclude<LibraryAvailabilityFilter, "all"> => {
  if (lesson.contentType === "post") {
    return "saved-text";
  }

  return mediaByLessonId.get(lesson.id)?.importStatus === "ready" ? "local" : "needs-files";
};

const contentTypeLabel = (contentType: ContentType): string => {
  switch (contentType) {
    case "video":
      return "Video";
    case "audio":
      return "Audio";
    case "pdf":
      return "PDF";
    case "post":
      return "Post";
  }
};

const availabilityLabel = (
  availability: Exclude<LibraryAvailabilityFilter, "all">,
): string => {
  switch (availability) {
    case "local":
      return "Local file";
    case "needs-files":
      return "Needs file";
    case "saved-text":
      return "Saved text";
  }
};

const latestDate = (values: Array<string | undefined>): string | undefined => {
  const dates = values.filter((value): value is string => Boolean(value));
  return dates.sort((left, right) => right.localeCompare(left))[0];
};

const sourceMatchesRelay = (source: Source, relay: TeacherRelay): boolean =>
  source.platform === "teacher-relay" &&
  (source.identifier === relay.feedUrl ||
    source.id === relay.id ||
    source.id === `source-${relay.id}` ||
    source.label === relay.title ||
    source.label === `Curator: ${relay.title}` ||
    source.label === `Channel: ${relay.title}`);

const buildChannelSubscriptions = ({
  teacherRelays,
  teachers,
  sources,
  lessons,
  mediaFiles,
  trustedCurators,
}: {
  teacherRelays: TeacherRelay[];
  teachers: Teacher[];
  sources: Source[];
  lessons: Lesson[];
  mediaFiles: MediaFile[];
  trustedCurators: TrustedCurator[];
}): ChannelSubscriptionView[] => {
  const teacherById = new Map(teachers.map((teacher) => [teacher.id, teacher]));
  const mediaByLessonId = new Map(mediaFiles.map((file) => [file.lessonId, file]));
  const trustedCuratorIds = new Set(trustedCurators.map((curator) => curator.id));

  return teacherRelays
    .map((relay) => {
      const source = sources.find((row) => sourceMatchesRelay(row, relay));
      const channelLessons = source
        ? lessons.filter((lesson) => lesson.sourceId === source.id)
        : lessons.filter((lesson) => lesson.teacherId === relay.teacherId);
      const missingFileCount = channelLessons.filter((lesson) =>
        needsLocalFile(lesson, mediaByLessonId),
      ).length;
      const localFileCount = channelLessons.filter((lesson) =>
        mediaByLessonId.has(lesson.id),
      ).length;
      const postCount = channelLessons.filter((lesson) => lesson.contentType === "post").length;
      const latestLessonAt = latestDate(channelLessons.map((lesson) => lesson.publishedAt));
      const latestUpdateAt = latestDate([
        relay.lastPublishedAt,
        source?.lastCheckedAt,
        latestLessonAt,
      ]);
      const trusted =
        relay.trustState === "signed-trusted" ||
        Boolean(source?.trustedCuratorId && trustedCuratorIds.has(source.trustedCuratorId));

      return {
        id: source?.id ?? relay.id,
        relayId: relay.id,
        teacherId: relay.teacherId,
        sourceId: source?.id,
        title: relay.title,
        curatorLabel: teacherById.get(relay.teacherId)?.displayName ?? relay.teacherId,
        description: relay.description,
        feedUrl: relay.feedUrl,
        feedFormat: relay.feedFormat,
        trustState: relay.trustState,
        trustPolicy: relay.trustPolicy,
        visibility: relay.visibility,
        autoDownload: relay.autoDownload,
        itemCount: channelLessons.length,
        localFileCount,
        missingFileCount,
        postCount,
        latestUpdateAt,
        latestPublishedAt: relay.lastPublishedAt,
        latestCheckedAt: source?.lastCheckedAt,
        trusted,
      };
    })
    .sort(
      (left, right) =>
        (right.latestUpdateAt ?? "").localeCompare(left.latestUpdateAt ?? "") ||
        left.title.localeCompare(right.title),
    );
};

const buildChannelFilterIndex = (
  channels: ChannelSubscriptionView[],
): Map<string, ChannelSubscriptionView> => {
  const index = new Map<string, ChannelSubscriptionView>();
  for (const channel of channels) {
    index.set(channel.id, channel);
    index.set(channel.relayId, channel);
    if (channel.sourceId) {
      index.set(channel.sourceId, channel);
    }
  }
  return index;
};

const lessonMatchesChannel = (
  lesson: Lesson,
  selectedChannelId: string,
  selectedChannel: ChannelSubscriptionView | undefined,
): boolean => {
  if (selectedChannelId === "all") {
    return true;
  }

  if (!selectedChannel) {
    return false;
  }

  return selectedChannel.sourceId
    ? lesson.sourceId === selectedChannel.sourceId
    : lesson.teacherId === selectedChannel.teacherId;
};

const lessonMatchesAvailability = (
  lesson: Lesson,
  selectedAvailability: LibraryAvailabilityFilter,
  mediaByLessonId: Map<string, MediaFile>,
): boolean =>
  selectedAvailability === "all" ||
  availabilityForLesson(lesson, mediaByLessonId) === selectedAvailability;

const lessonMatchesScope = (
  lesson: Lesson,
  activeScopeId: SmartScopeId,
  mediaByLessonId: Map<string, MediaFile>,
  watchByLessonId: Map<string, WatchState>,
): boolean => {
  switch (activeScopeId) {
    case "continue":
      return isInProgress(lesson, watchByLessonId);
    case "unwatched":
      return isUnwatched(lesson, watchByLessonId);
    case "completed":
      return Boolean(watchByLessonId.get(lesson.id)?.completed);
    case "needs-files":
      return needsLocalFile(lesson, mediaByLessonId);
    case "all":
    case "recent":
      return true;
  }
};

const orderScopedLessons = (
  lessons: Lesson[],
  activeScopeId: SmartScopeId,
  watchByLessonId: Map<string, WatchState>,
): Lesson[] => {
  if (activeScopeId !== "recent") {
    return lessons;
  }

  return [...lessons].sort((left, right) => {
    const leftDate = watchByLessonId.get(left.id)?.lastWatchedAt ?? left.publishedAt ?? "";
    const rightDate = watchByLessonId.get(right.id)?.lastWatchedAt ?? right.publishedAt ?? "";
    return rightDate.localeCompare(leftDate);
  });
};

const buildGroups = <T extends Teacher | Collection | Source>(
  lessons: Lesson[],
  rows: T[],
  watchByLessonId: Map<string, WatchState>,
  kind: "teacher" | "collection" | "source",
): LibraryGroup[] =>
  rows
    .map((row) => {
      const groupedLessons = lessons.filter((lesson) =>
        kind === "teacher"
          ? lesson.teacherId === row.id
          : kind === "collection"
            ? lesson.collectionId === row.id
            : lesson.sourceId === row.id,
      );
      return {
        id: row.id,
        label: "displayName" in row ? row.displayName : "title" in row ? row.title : row.label,
        lessonCount: groupedLessons.length,
        activeCount: groupedLessons.filter((lesson) => isInProgress(lesson, watchByLessonId))
          .length,
        completedCount: groupedLessons.filter((lesson) => watchByLessonId.get(lesson.id)?.completed)
          .length,
      };
    })
    .filter((group) => group.lessonCount > 0)
    .sort((left, right) => right.lessonCount - left.lessonCount || left.label.localeCompare(right.label));

const buildContentTypeGroups = (
  lessons: Lesson[],
  watchByLessonId: Map<string, WatchState>,
): LibraryGroup[] => {
  const contentTypes: ContentType[] = ["video", "audio", "pdf", "post"];
  return contentTypes
    .map((contentType) =>
      groupFromLessons(
        contentType,
        contentTypeLabel(contentType),
        lessons.filter((lesson) => lesson.contentType === contentType),
        watchByLessonId,
      ),
    )
    .filter((group) => group.lessonCount > 0);
};

const buildAvailabilityGroups = (
  lessons: Lesson[],
  mediaByLessonId: Map<string, MediaFile>,
  watchByLessonId: Map<string, WatchState>,
): LibraryGroup[] => {
  const availabilityIds: Array<Exclude<LibraryAvailabilityFilter, "all">> = [
    "local",
    "needs-files",
    "saved-text",
  ];
  return availabilityIds
    .map((availability) =>
      groupFromLessons(
        availability,
        availabilityLabel(availability),
        lessons.filter((lesson) => availabilityForLesson(lesson, mediaByLessonId) === availability),
        watchByLessonId,
      ),
    )
    .filter((group) => group.lessonCount > 0);
};

const groupFromLessons = (
  id: string,
  label: string,
  groupedLessons: Lesson[],
  watchByLessonId: Map<string, WatchState>,
): LibraryGroup => ({
  id,
  label,
  lessonCount: groupedLessons.length,
  activeCount: groupedLessons.filter((lesson) => isInProgress(lesson, watchByLessonId)).length,
  completedCount: groupedLessons.filter((lesson) => watchByLessonId.get(lesson.id)?.completed)
    .length,
});
