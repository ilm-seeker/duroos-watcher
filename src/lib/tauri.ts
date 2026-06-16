import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { seedSnapshot } from "../data/seed";
import type {
  AppSnapshot,
  ClearSourceSummary,
  DownloadSourceSummary,
  ImportSummary,
  IngestSummary,
  TrustedCurator,
  TrustCuratorSummary,
  RuntimeDiagnostics,
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

export const resolveMediaFileUrl = async (mediaFileId: string): Promise<string> => {
  if (!isTauriRuntime()) {
    throw new Error("Media playback requires the Tauri desktop runtime.");
  }

  const filePath = await invoke<string>("resolve_media_file_path", { mediaFileId });
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
          "mp3",
          "m4a",
          "aac",
          "wav",
          "flac",
          "ogg",
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
