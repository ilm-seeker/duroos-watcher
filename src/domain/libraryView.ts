import type { Collection, Lesson, MediaFile, Source, Teacher, WatchState } from "./types";

export interface LibraryLessonViewInput {
  query: string;
  selectedLessonId: string;
  lessons: Lesson[];
  teachers: Teacher[];
  collections: Collection[];
  sources: Source[];
  mediaFiles: MediaFile[];
  watchState: WatchState[];
}

export interface LibraryLessonView {
  isSearchActive: boolean;
  filteredLessons: Lesson[];
  selectedLesson?: Lesson;
  continueLessons: Lesson[];
  newLessons: Lesson[];
}

export const buildLibraryLessonView = ({
  query,
  selectedLessonId,
  lessons,
  teachers,
  collections,
  sources,
  mediaFiles,
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

  const filteredLessons = isSearchActive
    ? lessons.filter((lesson) =>
        [
          lesson.title,
          lesson.description,
          lesson.sourceUrl,
          teacherById.get(lesson.teacherId)?.displayName,
          collectionById.get(lesson.collectionId)?.title,
          sourceById.get(lesson.sourceId)?.label,
        ].some((value) => value?.toLowerCase().includes(normalizedQuery)),
      )
    : lessons;

  const selectedLesson = isSearchActive
    ? filteredLessons.find((lesson) => lesson.id === selectedLessonId) ?? filteredLessons[0]
    : lessons.find((lesson) => lesson.id === selectedLessonId) ?? lessons[0];
  const scopedLessons = isSearchActive ? filteredLessons : lessons;

  return {
    isSearchActive,
    filteredLessons,
    selectedLesson,
    continueLessons: scopedLessons
      .filter((lesson) => {
        const progress = watchByLessonId.get(lesson.id);
        return progress && !progress.completed;
      })
      .slice(0, 4),
    newLessons: scopedLessons
      .filter((lesson) => !mediaByLessonId.has(lesson.id))
      .concat(scopedLessons.filter((lesson) => mediaByLessonId.has(lesson.id)).slice(0, 2))
      .slice(0, 4),
  };
};
