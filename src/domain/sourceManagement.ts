import type { ContentType, Job, Lesson, Source } from "./types";

export const DEFAULT_SOURCE_IDS: ReadonlySet<string> = new Set([
  "source-local-files",
  "source-telegram",
  "source-rss-feed",
  "source-archive-org",
  "source-youtube",
  "source-x",
  "source-rumble",
  "source-odysee",
  "source-teacher-relay",
]);

export interface SourceStats {
  itemCount: number;
  localFileCount: number;
  missingFileCount: number;
  postCount: number;
  jobCount: number;
}

export const splitSourceRows = (
  sources: Source[],
): { capabilitySources: Source[]; addedSources: Source[] } => ({
  capabilitySources: sources.filter((source) => DEFAULT_SOURCE_IDS.has(source.id)),
  addedSources: sources.filter((source) => !DEFAULT_SOURCE_IDS.has(source.id)),
});

export const getSourceStats = (
  sourceId: string,
  lessons: Lesson[],
  jobs: Job[],
): SourceStats => {
  const sourceLessons = lessons.filter((lesson) => lesson.sourceId === sourceId);
  const fileBackedLessons = sourceLessons.filter((lesson) =>
    isFileBackedContentType(lesson.contentType),
  );

  return {
    itemCount: sourceLessons.length,
    localFileCount: fileBackedLessons.filter((lesson) => lesson.mediaFileId).length,
    missingFileCount: fileBackedLessons.filter((lesson) => !lesson.mediaFileId).length,
    postCount: sourceLessons.filter((lesson) => lesson.contentType === "post").length,
    jobCount: jobs.filter((job) => job.sourceId === sourceId).length,
  };
};

export const isFileBackedContentType = (contentType: ContentType): boolean =>
  contentType === "video" || contentType === "audio" || contentType === "pdf";
