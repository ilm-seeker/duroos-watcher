import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { seedSnapshot } from "../data/seed";
import type {
  AppSnapshot,
  ClearSourceSummary,
  DownloadSourceSummary,
  ChannelPublishResult,
  CreatePublisherProfileRequest,
  ImportSummary,
  IngestSummary,
  Lesson,
  LessonNote,
  MediaStorageAudit,
  MediaStorageCleanup,
  NativePlaybackResult,
  NostrChannelPreview,
  PhoneMediaScope,
  PhoneMediaSession,
  PublishTeacherChannelRequest,
  PublisherEndpointTestReport,
  PublisherEndpointTestRequest,
  PublisherProfile,
  TrustedCurator,
  TrustCuratorSummary,
  RuntimeDiagnostics,
  WatchState,
} from "../domain/types";
import {
  parseCollectionManifest,
  type ManifestValidationReport,
} from "../domain/collectionManifest";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export const isTauriRuntime = (): boolean =>
  typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);

export const getAppSnapshot = async (): Promise<AppSnapshot> => {
  if (!isTauriRuntime()) {
    return seedSnapshot;
  }

  return invoke<AppSnapshot>("get_app_snapshot");
};

export const getRuntimeDiagnostics = async (): Promise<RuntimeDiagnostics> => {
  if (!isTauriRuntime()) {
    return {
      desktopRuntimeAvailable: false,
      ytDlpAvailable: false,
      requiredMediaToolsAvailable: false,
      mediaToolSource: "missing",
      missingMediaTools: ["yt-dlp", "ffmpeg", "ffprobe"],
      nativePlaybackAvailable: false,
      ytDlpCookiesConfigured: false,
      messages: ["Open the desktop app to check local downloader tools."],
    };
  }

  return invoke<RuntimeDiagnostics>("get_runtime_diagnostics");
};

export const searchLessons = async (query: string): Promise<AppSnapshot["lessons"]> => {
  if (!isTauriRuntime()) {
    const needle = query.trim().toLowerCase();
    if (!needle) {
      return seedSnapshot.lessons;
    }

    return seedSnapshot.lessons.filter((lesson) =>
      [lesson.title, lesson.description, lesson.sourceUrl].some((value) =>
        value?.toLowerCase().includes(needle),
      ),
    );
  }

  return invoke<AppSnapshot["lessons"]>("search_lessons", { query });
};

export const saveWatchState = async (
  lessonId: string,
  progressSeconds: number,
  durationSeconds: number | undefined,
  completed: boolean,
): Promise<WatchState> => {
  if (!isTauriRuntime()) {
    return {
      lessonId,
      progressSeconds,
      completed,
      lastWatchedAt: new Date().toISOString(),
    };
  }

  return invoke<WatchState>("save_watch_state", {
    lessonId,
    progressSeconds,
    durationSeconds: durationSeconds ?? null,
    completed,
  });
};

export const saveLessonNote = async (
  lessonId: string,
  body: string,
): Promise<LessonNote> => {
  if (!isTauriRuntime()) {
    return {
      lessonId,
      body: body.trim(),
      updatedAt: new Date().toISOString(),
    };
  }

  return invoke<LessonNote>("save_lesson_note", { lessonId, body });
};

export const updateLessonOrganization = async (
  lessonId: string,
  teacherDisplayName: string,
  collectionTitle: string,
): Promise<Lesson> => {
  if (!isTauriRuntime()) {
    const lesson = seedSnapshot.lessons.find((item) => item.id === lessonId);
    if (!lesson) {
      throw new Error("Lesson was not found.");
    }

    return lesson;
  }

  return invoke<Lesson>("update_lesson_organization", {
    lessonId,
    teacherDisplayName,
    collectionTitle,
  });
};

export const resolveMediaFileUrl = async (mediaFileId: string): Promise<string> => {
  if (!isTauriRuntime()) {
    throw new Error("Media playback requires the Tauri desktop runtime.");
  }

  const filePath = await invoke<string>("resolve_media_file_path", { mediaFileId });
  return convertFileSrc(filePath);
};

export const resolveMediaThumbnailUrl = async (mediaFileId: string): Promise<string> => {
  if (!isTauriRuntime()) {
    throw new Error("Media cover previews require the Tauri desktop runtime.");
  }

  const filePath = await invoke<string>("resolve_media_thumbnail_path", { mediaFileId });
  return convertFileSrc(filePath);
};

export const importLocalFiles = async (paths: string[]): Promise<ImportSummary> => {
  if (!isTauriRuntime()) {
    return {
      imported: 0,
      skipped: paths.length,
      failed: 0,
      messages: ["Local file import requires the Tauri desktop runtime."],
    };
  }

  return invoke<ImportSummary>("import_local_files", { paths });
};

export const ingestSourceUrl = async (sourceUrl: string): Promise<IngestSummary> => {
  if (!isTauriRuntime()) {
    return {
      sourceUrl,
      discovered: 0,
      imported: 0,
      skipped: 1,
      failed: 0,
      messages: ["Source ingestion requires the Tauri desktop runtime."],
    };
  }

  return invoke<IngestSummary>("ingest_source_url", { sourceUrl });
};

export const refreshSource = async (sourceId: string): Promise<IngestSummary> => {
  if (!isTauriRuntime()) {
    return {
      sourceUrl: sourceId,
      discovered: 0,
      imported: 0,
      skipped: 1,
      failed: 0,
      messages: ["Source refresh requires the Tauri desktop runtime."],
    };
  }

  return invoke<IngestSummary>("refresh_source", { sourceId });
};

export const clearSourceContent = async (
  sourceId: string,
  removeSource: boolean,
): Promise<ClearSourceSummary> => {
  if (!isTauriRuntime()) {
    return {
      sourceId,
      sourceLabel: sourceId,
      removedSource: false,
      lessonsRemoved: 0,
      mediaFilesRemoved: 0,
      jobsRemoved: 0,
      messages: ["Source cleanup requires the Tauri desktop runtime."],
    };
  }

  return invoke<ClearSourceSummary>("clear_source_content", { sourceId, removeSource });
};

export const downloadSourceMedia = async (sourceId: string): Promise<DownloadSourceSummary> => {
  if (!isTauriRuntime()) {
    return {
      sourceId,
      sourceLabel: sourceId,
      attempted: 0,
      downloaded: 0,
      skipped: 0,
      failed: 0,
      messages: ["Source download requires the Tauri desktop runtime."],
    };
  }

  return invoke<DownloadSourceSummary>("download_source_media", { sourceId });
};

export const auditMediaStorage = async (): Promise<MediaStorageAudit> => {
  if (!isTauriRuntime()) {
    return {
      scannedFiles: 0,
      referencedFiles: 0,
      staleFiles: 0,
      staleBytes: 0,
      partialFiles: 0,
      staleSamples: [],
      messages: ["Storage audit requires the Tauri desktop runtime."],
    };
  }

  return invoke<MediaStorageAudit>("audit_media_storage");
};

export const cleanupMediaStorage = async (): Promise<MediaStorageCleanup> => {
  if (!isTauriRuntime()) {
    const audit = await auditMediaStorage();
    return {
      audit,
      removedFiles: 0,
      failedRemovals: 0,
      reclaimedBytes: 0,
      messages: ["Storage cleanup requires the Tauri desktop runtime."],
    };
  }

  return invoke<MediaStorageCleanup>("cleanup_media_storage");
};

export const playMediaFileNative = async (
  mediaFileId: string,
): Promise<NativePlaybackResult> => {
  if (!isTauriRuntime()) {
    throw new Error("Native playback requires the Tauri desktop runtime.");
  }

  return invoke<NativePlaybackResult>("play_media_file_native", { mediaFileId });
};

export const startPhoneMediaSession = async (
  scope?: PhoneMediaScope,
): Promise<PhoneMediaSession> => {
  if (!isTauriRuntime()) {
    return {
      id: "",
      active: false,
      itemCount: 0,
      items: [],
      endpoints: [],
      messages: ["Phone access requires the Tauri desktop runtime."],
    };
  }

  return invoke<PhoneMediaSession>("start_phone_media_session", {
    scope: scope ?? null,
  });
};

export const getPhoneMediaSession = async (): Promise<PhoneMediaSession | null> => {
  if (!isTauriRuntime()) {
    return null;
  }

  return invoke<PhoneMediaSession | null>("get_phone_media_session");
};

export const stopPhoneMediaSession = async (
  sessionId: string,
): Promise<PhoneMediaSession> => {
  if (!isTauriRuntime()) {
    return {
      id: sessionId,
      active: false,
      itemCount: 0,
      items: [],
      messages: ["Phone access requires the Tauri desktop runtime."],
    };
  }

  return invoke<PhoneMediaSession>("stop_phone_media_session", { sessionId });
};

export const chooseLocalMediaPaths = async (): Promise<string[]> => {
  if (!isTauriRuntime()) {
    return [];
  }

  const selected = await open({
    multiple: true,
    directory: false,
    filters: [
      {
        name: "Study files",
        extensions: [
          "mp4",
          "m4v",
          "mov",
          "mkv",
          "webm",
          "avi",
          "wmv",
          "flv",
          "mpg",
          "mpeg",
          "ts",
          "m2ts",
          "mts",
          "vob",
          "3gp",
          "3g2",
          "mp3",
          "m4a",
          "aac",
          "wav",
          "flac",
          "ogg",
          "opus",
          "wma",
          "aiff",
          "aif",
          "amr",
          "pdf",
        ],
      },
    ],
  });

  if (!selected) {
    return [];
  }

  return Array.isArray(selected) ? selected : [selected];
};

export const validateCollectionManifest = async (
  manifestJson: string,
): Promise<ManifestValidationReport> => {
  if (!isTauriRuntime()) {
    return parseCollectionManifest(manifestJson);
  }

  return invoke<ManifestValidationReport>("validate_collection_manifest", {
    manifestJson,
  });
};

export const addTrustedCurator = async (
  displayName: string,
  publicKey: string,
  trustNote?: string,
): Promise<TrustedCurator> => {
  if (!isTauriRuntime()) {
    throw new Error("Trusted curator storage requires the Tauri desktop runtime.");
  }

  return invoke<TrustedCurator>("add_trusted_curator", {
    displayName,
    publicKey,
    trustNote: trustNote?.trim() ? trustNote : null,
  });
};

export const removeTrustedCurator = async (
  curatorId: string,
): Promise<TrustCuratorSummary> => {
  if (!isTauriRuntime()) {
    return {
      curatorId,
      displayName: curatorId,
      publicKey: "",
      sourcesUpdated: 0,
      messages: ["Trusted curator storage requires the Tauri desktop runtime."],
    };
  }

  return invoke<TrustCuratorSummary>("remove_trusted_curator", { curatorId });
};

export const listPublisherProfiles = async (): Promise<PublisherProfile[]> => {
  if (!isTauriRuntime()) {
    return [];
  }

  return invoke<PublisherProfile[]>("list_publisher_profiles");
};

export const createPublisherProfile = async (
  request: CreatePublisherProfileRequest,
): Promise<PublisherProfile> => {
  if (!isTauriRuntime()) {
    throw new Error("Teacher publisher profiles require the Tauri desktop runtime.");
  }

  return invoke<PublisherProfile>("create_publisher_profile", { request });
};

export const unlockPublisherProfile = async (
  profileId: string,
  passphrase: string,
): Promise<PublisherProfile> => {
  if (!isTauriRuntime()) {
    throw new Error("Teacher publisher vaults require the Tauri desktop runtime.");
  }

  return invoke<PublisherProfile>("unlock_publisher_profile", { profileId, passphrase });
};

export const publishTeacherChannel = async (
  request: PublishTeacherChannelRequest,
): Promise<ChannelPublishResult> => {
  if (!isTauriRuntime()) {
    throw new Error("Federated teacher publishing requires the Tauri desktop runtime.");
  }

  return invoke<ChannelPublishResult>("publish_teacher_channel", { request });
};

export const testPublisherEndpoints = async (
  request: PublisherEndpointTestRequest,
): Promise<PublisherEndpointTestReport> => {
  if (!isTauriRuntime()) {
    return {
      passed: false,
      blossomResults: [],
      relayResults: [],
      messages: ["Publisher endpoint testing requires the Tauri desktop runtime."],
    };
  }

  return invoke<PublisherEndpointTestReport>("test_publisher_endpoints", { request });
};

export const previewNostrChannel = async (
  channelRef: string,
): Promise<NostrChannelPreview> => {
  if (!isTauriRuntime()) {
    throw new Error("Nostr channel previews require the Tauri desktop runtime.");
  }

  return invoke<NostrChannelPreview>("preview_nostr_channel", { channelRef });
};
