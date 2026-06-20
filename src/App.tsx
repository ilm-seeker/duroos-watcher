import {
  AlertTriangle,
  Archive,
  BookOpen,
  CheckCircle2,
  ChevronRight,
  Clock3,
  Copy,
  Database,
  Download,
  FileArchive,
  FileText,
  FolderOpen,
  Globe2,
  HardDrive,
  History,
  Import,
  KeyRound,
  Library,
  ListVideo,
  Lock,
  MessageSquare,
  PanelLeftClose,
  PanelLeftOpen,
  Play,
  QrCode,
  RefreshCcw,
  RadioTower,
  Rss,
  Search,
  ShieldCheck,
  Smartphone,
  Square,
  Trash2,
  UploadCloud,
  UserRound,
  Video,
  Volume2,
  Wifi,
  WifiOff,
  X,
} from "lucide-react";
import QRCode from "qrcode";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  addTrustedCurator,
  auditMediaStorage,
  chooseLocalMediaPaths,
  clearSourceContent,
  cleanupMediaStorage,
  createPublisherProfile,
  downloadSourceMedia,
  getPhoneMediaSession,
  getAppSnapshot,
  getRuntimeDiagnostics,
  ingestSourceUrl,
  importLocalFiles,
  isTauriRuntime,
  listPublisherProfiles,
  playMediaFileNative,
  previewNostrChannel,
  publishTeacherChannel,
  refreshSource,
  removeTrustedCurator,
  resolveMediaFileUrl,
  resolveMediaThumbnailUrl,
  saveLessonNote,
  saveWatchState,
  startPhoneMediaSession,
  stopPhoneMediaSession,
  testPublisherEndpoints,
  unlockPublisherProfile,
  updateLessonOrganization,
  validateCollectionManifest,
} from "./lib/tauri";
import type { ManifestValidationReport } from "./domain/collectionManifest";
import {
  buildChannelInvite,
  canonicalizeChannelRef,
} from "./domain/channelInvite";
import { displayJobDetail } from "./domain/jobDisplay";
import {
  filterQueueJobs,
  queueFilterLabel,
  queueFilterMatches,
  queueFilters,
  type QueueFilter,
} from "./domain/jobQueue";
import { recordOnboardingLane } from "./domain/onboardingState";
import {
  endpointTestHasFailures,
  endpointTestStatus,
} from "./domain/publisherEndpointReport";
import type {
  AppSnapshot,
  ArchiveMirrorConfig,
  CapabilityLevel,
  ChannelPublishResult,
  Collection,
  ContentType,
  Job,
  Lesson,
  LessonNote,
  LiveSession,
  MediaStorageAudit,
  MediaFile,
  NostrChannelPreview,
  PhoneMediaSession,
  PublishedLessonDraft,
  PublisherEndpointTestReport,
  PublisherProfile,
  ProvenanceRecord,
  RuntimeDiagnostics,
  Source,
  SourcePlatform,
  Teacher,
  TrustState,
  TrustedCurator,
  WatchState,
} from "./domain/types";
import {
  DEFAULT_SOURCE_IDS,
  getSourceStats,
  isFileBackedContentType,
  splitSourceRows,
} from "./domain/sourceManagement";
import {
  buildLibraryLessonView,
  type ChannelSubscriptionView,
  type LibraryAvailabilityFilter,
  type LibraryContentTypeFilter,
  type LibraryGroup,
  type SmartScopeId,
} from "./domain/libraryView";
import { seedSnapshot } from "./data/seed";

type ViewMode = "library" | "relays" | "publish" | "sources" | "queue";
type ImportMode = "local" | "source" | "feed" | "manifest" | "keys";
type BusySourceAction = { sourceId: string; action: "clear" | "download" } | null;
type PhoneAccessBusyAction = "start" | "stop" | null;
type MediaStorageBusyAction = "audit" | "cleanup" | null;

const sidebarCollapsedStorageKey = "duroos.sidebarCollapsed";

const defaultRuntimeDiagnostics: RuntimeDiagnostics = {
  desktopRuntimeAvailable: isTauriRuntime(),
  ytDlpAvailable: false,
  requiredMediaToolsAvailable: false,
  mediaToolSource: "missing",
  missingMediaTools: ["yt-dlp", "ffmpeg", "ffprobe"],
  nativePlaybackAvailable: false,
  ytDlpCookiesConfigured: false,
  messages: ["Runtime diagnostics have not been checked yet."],
};

const starterNostrRelayPresets = [
  { name: "Damus relay", url: "wss://relay.damus.io" },
  { name: "nos.lol", url: "wss://nos.lol" },
  { name: "Primal relay", url: "wss://relay.primal.net" },
];

const starterBlossomServerPresets = [
  { name: "Primal Blossom", url: "https://blossom.primal.net" },
  { name: "hzrd149 CDN", url: "https://cdn.hzrd149.com" },
  { name: "Blossom Band", url: "https://blossom.band" },
];

const starterRelayText = starterNostrRelayPresets.map((preset) => preset.url).join("\n");
const starterBlossomText = starterBlossomServerPresets.map((preset) => preset.url).join("\n");

const downloaderStatus = (
  runtimeDiagnostics: RuntimeDiagnostics,
): { label: string; tone: "neutral" | "positive" | "warning" } => {
  if (!runtimeDiagnostics.desktopRuntimeAvailable) {
    return { label: "Desktop check", tone: "neutral" };
  }

  if (runtimeDiagnostics.requiredMediaToolsAvailable) {
    if (runtimeDiagnostics.mediaToolSource === "bundled") {
      return { label: "Bundled tools ready", tone: "positive" };
    }
    if (runtimeDiagnostics.mediaToolSource === "system") {
      return { label: "System tools ready", tone: "positive" };
    }
    return { label: "Media tools ready", tone: "positive" };
  }

  const missing = runtimeDiagnostics.missingMediaTools.join(", ");
  return {
    label: missing ? `${missing} needed` : "Media tools needed",
    tone: "warning",
  };
};

const formatDuration = (seconds?: number): string => {
  if (!seconds) {
    return "--:--";
  }

  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = seconds % 60;

  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, "0")}:${secs
      .toString()
      .padStart(2, "0")}`;
  }

  return `${minutes}:${secs.toString().padStart(2, "0")}`;
};

const formatDate = (value?: string): string => {
  if (!value) {
    return "Not checked";
  }

  return new Intl.DateTimeFormat("en", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(value));
};

const formatBytes = (bytes: number): string => {
  const safeBytes = Math.max(0, bytes);
  if (safeBytes >= 1024 * 1024 * 1024) {
    return `${(safeBytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  }
  if (safeBytes >= 1024 * 1024) {
    return `${(safeBytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  if (safeBytes >= 1024) {
    return `${(safeBytes / 1024).toFixed(1)} KB`;
  }
  return `${safeBytes} B`;
};

const capabilityLabel = (level: CapabilityLevel): string => {
  switch (level) {
    case "native":
      return "Native";
    case "supported":
      return "Supported";
    case "limited":
      return "Limited";
    case "blocked":
      return "No";
  }
};

const capabilityClass = (level: CapabilityLevel): string => {
  switch (level) {
    case "native":
    case "supported":
      return "status-positive";
    case "limited":
      return "status-warning";
    case "blocked":
      return "status-muted";
  }
};

const trustLabel = (trustState: TrustState): string => {
  switch (trustState) {
    case "unsigned":
      return "Unsigned";
    case "signed-untrusted":
      return "Signed, untrusted";
    case "signed-trusted":
      return "Signed, trusted";
    case "tampered":
      return "Tampered";
  }
};

const trustTone = (trustState: TrustState): "neutral" | "positive" | "warning" | "danger" => {
  switch (trustState) {
    case "signed-trusted":
      return "positive";
    case "signed-untrusted":
    case "unsigned":
      return "warning";
    case "tampered":
      return "danger";
  }
};

const normalizeSearch = (value: string): string => value.trim().toLowerCase();

const includesQuery = (query: string, values: Array<string | undefined>): boolean => {
  if (!query) {
    return true;
  }

  return values.some((value) => value?.toLowerCase().includes(query));
};

const searchConfigForView = (
  viewMode: ViewMode,
): { ariaLabel: string; placeholder: string } | null => {
  switch (viewMode) {
    case "library":
      return {
        ariaLabel: "Search library by title, teacher, source, or URL",
        placeholder: "Search lessons, teachers, sources, or URLs",
      };
    case "relays":
      return {
        ariaLabel: "Search channels and live lessons",
        placeholder: "Search channels, curators, live lessons",
      };
    case "publish":
      return null;
    case "sources":
      return {
        ariaLabel: "Search source types, added sources, or curator keys",
        placeholder: "Search platforms, sources, keys",
      };
    case "queue":
      return {
        ariaLabel: "Search update queue by job, state, source, or detail",
        placeholder: "Search jobs, states, sources, details",
      };
  }
};

const getLessonProgress = (lesson: Lesson, watchState?: WatchState): number => {
  if (!lesson.durationSeconds || !watchState) {
    return 0;
  }

  return Math.min(100, Math.round((watchState.progressSeconds / lesson.durationSeconds) * 100));
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

const contentTypeIcon = (contentType: ContentType) => {
  switch (contentType) {
    case "video":
      return Video;
    case "audio":
      return Volume2;
    case "pdf":
      return FileText;
    case "post":
      return MessageSquare;
  }
};

const availabilityLabel = (lesson: Lesson, mediaFile?: MediaFile): string => {
  if (!isFileBackedContentType(lesson.contentType)) {
    return "Saved post";
  }

  if (!mediaFile) {
    return "Needs file";
  }

  if (mediaFile.importStatus === "copying") {
    return "Copying file";
  }

  if (mediaFile.importStatus === "missing") {
    return "File missing";
  }

  if (mediaFile.importStatus === "failed") {
    return "Import failed";
  }

  if (isPlaybackUnverified(mediaFile)) {
    return "Codec unverified";
  }

  if (isPlaybackCompatible(mediaFile)) {
    return mediaFile.hashVerificationState === "matched"
      ? "Verified playable file"
      : "Playable download";
  }

  return mediaFile.hashVerificationState === "matched" ? "Verified local file" : "Downloaded";
};

const isPlaybackCompatible = (mediaFile?: MediaFile): boolean =>
  mediaFile?.codec?.startsWith("webkit-compatible:") ?? false;

const isPlaybackUnverified = (mediaFile?: MediaFile): boolean =>
  mediaFile?.codec?.startsWith("webkit-unverified:") ?? false;

const availabilityTone = (
  lesson: Lesson,
  mediaFile?: MediaFile,
): "neutral" | "positive" | "warning" => {
  if (!isFileBackedContentType(lesson.contentType)) {
    return "neutral";
  }

  if (!mediaFile || mediaFile.importStatus !== "ready") {
    return "warning";
  }

  if (isPlaybackUnverified(mediaFile)) {
    return "warning";
  }

  return "positive";
};

const App = () => {
  const [snapshot, setSnapshot] = useState<AppSnapshot>(seedSnapshot);
  const [query, setQuery] = useState("");
  const [selectedLessonId, setSelectedLessonId] = useState(seedSnapshot.lessons[0]?.id ?? "");
  const [activeScopeId, setActiveScopeId] = useState<SmartScopeId>("all");
  const [selectedTeacherId, setSelectedTeacherId] = useState("all");
  const [selectedCollectionId, setSelectedCollectionId] = useState("all");
  const [selectedSourceId, setSelectedSourceId] = useState("all");
  const [selectedChannelId, setSelectedChannelId] = useState("all");
  const [selectedContentType, setSelectedContentType] =
    useState<LibraryContentTypeFilter>("all");
  const [selectedAvailability, setSelectedAvailability] =
    useState<LibraryAvailabilityFilter>("all");
  const [viewMode, setViewMode] = useState<ViewMode>("library");
  const [isOnlineMode, setIsOnlineMode] = useState(false);
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState(() => {
    if (typeof window === "undefined") {
      return false;
    }

    return window.localStorage.getItem(sidebarCollapsedStorageKey) === "true";
  });
  const [isImportOpen, setIsImportOpen] = useState(false);
  const [initialImportMode, setInitialImportMode] = useState<ImportMode>("local");
  const [systemNotice, setSystemNotice] = useState("");
  const [busySourceAction, setBusySourceAction] = useState<BusySourceAction>(null);
  const [busyNativeMediaId, setBusyNativeMediaId] = useState<string | null>(null);
  const [selectedMediaUrl, setSelectedMediaUrl] = useState("");
  const [selectedMediaError, setSelectedMediaError] = useState("");
  const [runtimeDiagnostics, setRuntimeDiagnostics] = useState<RuntimeDiagnostics>(
    defaultRuntimeDiagnostics,
  );
  const [isRefreshingSources, setIsRefreshingSources] = useState(false);
  const [phoneSession, setPhoneSession] = useState<PhoneMediaSession | null>(null);
  const [phoneAccessBusyAction, setPhoneAccessBusyAction] =
    useState<PhoneAccessBusyAction>(null);
  const [phoneAccessNotice, setPhoneAccessNotice] = useState("");
  const [publisherProfiles, setPublisherProfiles] = useState<PublisherProfile[]>([]);
  const [mediaThumbnailUrls, setMediaThumbnailUrls] = useState<Record<string, string>>({});
  const [mediaStorageAudit, setMediaStorageAudit] = useState<MediaStorageAudit | null>(null);
  const [mediaStorageBusyAction, setMediaStorageBusyAction] =
    useState<MediaStorageBusyAction>(null);

  useEffect(() => {
    window.localStorage.setItem(sidebarCollapsedStorageKey, String(isSidebarCollapsed));
  }, [isSidebarCollapsed]);

  useEffect(() => {
    let isMounted = true;

    getAppSnapshot()
      .then((nextSnapshot) => {
        if (!isMounted) {
          return;
        }

        setSnapshot(nextSnapshot);
        setSelectedLessonId((current) => current || nextSnapshot.lessons[0]?.id || "");
      })
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unknown load error";
        setSystemNotice(`Using local seed data: ${message}`);
      });

    return () => {
      isMounted = false;
    };
  }, []);

  useEffect(() => {
    let isMounted = true;

    getRuntimeDiagnostics()
      .then((diagnostics) => {
        if (isMounted) {
          setRuntimeDiagnostics(diagnostics);
        }
      })
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : String(error);
        if (isMounted) {
          setRuntimeDiagnostics({
            desktopRuntimeAvailable: isTauriRuntime(),
            ytDlpAvailable: false,
            requiredMediaToolsAvailable: false,
            mediaToolSource: "missing",
            missingMediaTools: ["yt-dlp", "ffmpeg", "ffprobe"],
            nativePlaybackAvailable: false,
            ytDlpCookiesConfigured: false,
            messages: [message],
          });
        }
      });

    return () => {
      isMounted = false;
    };
  }, []);

  useEffect(() => {
    let isMounted = true;

    getPhoneMediaSession()
      .then((session) => {
        if (isMounted && session?.active) {
          setPhoneSession(session);
        }
      })
      .catch(() => {
        if (isMounted) {
          setPhoneSession(null);
        }
      });

    return () => {
      isMounted = false;
    };
  }, []);

  const refreshPublisherProfiles = async () => {
    try {
      setPublisherProfiles(await listPublisherProfiles());
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setSystemNotice(message);
    }
  };

  useEffect(() => {
    void refreshPublisherProfiles();
  }, []);

  const refreshSnapshot = async (fallbackNotice?: string) => {
    try {
      const nextSnapshot = await getAppSnapshot();
      setSnapshot(nextSnapshot);
      setSelectedLessonId((current) => current || nextSnapshot.lessons[0]?.id || "");
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : "Unknown refresh error";
      setSystemNotice(fallbackNotice ? `${fallbackNotice} Snapshot refresh failed: ${message}` : message);
    }
  };

  const teacherById = useMemo(
    () => new Map(snapshot.teachers.map((teacher) => [teacher.id, teacher])),
    [snapshot.teachers],
  );
  const collectionById = useMemo(
    () => new Map(snapshot.collections.map((collection) => [collection.id, collection])),
    [snapshot.collections],
  );
  const sourceById = useMemo(
    () => new Map(snapshot.sources.map((source) => [source.id, source])),
    [snapshot.sources],
  );
  const mediaByLessonId = useMemo(
    () => new Map(snapshot.mediaFiles.map((file) => [file.lessonId, file])),
    [snapshot.mediaFiles],
  );

  useEffect(() => {
    let isMounted = true;
    const mediaFilesWithThumbnails = snapshot.mediaFiles.filter(
      (mediaFile) => mediaFile.thumbnailRelativePath,
    );

    if (!mediaFilesWithThumbnails.length) {
      setMediaThumbnailUrls({});
      return () => {
        isMounted = false;
      };
    }

    Promise.all(
      mediaFilesWithThumbnails.map(async (mediaFile) => {
        try {
          const thumbnailUrl = await resolveMediaThumbnailUrl(mediaFile.id);
          return [mediaFile.id, thumbnailUrl] as const;
        } catch {
          return null;
        }
      }),
    ).then((entries) => {
      if (!isMounted) {
        return;
      }

      setMediaThumbnailUrls(
        Object.fromEntries(entries.filter((entry): entry is readonly [string, string] => Boolean(entry))),
      );
    });

    return () => {
      isMounted = false;
    };
  }, [snapshot.mediaFiles]);

  const provenanceById = useMemo(
    () =>
      new Map(
        snapshot.provenanceRecords.map((record) => [record.id, record] as const),
      ),
    [snapshot.provenanceRecords],
  );
  const watchByLessonId = useMemo(
    () => new Map(snapshot.watchState.map((state) => [state.lessonId, state])),
    [snapshot.watchState],
  );
  const noteByLessonId = useMemo(
    () => new Map(snapshot.lessonNotes.map((note) => [note.lessonId, note])),
    [snapshot.lessonNotes],
  );

  const {
    isSearchActive,
    filteredLessons,
    selectedLesson,
    continueLessons,
    newLessons,
    smartScopes,
    teacherGroups,
    collectionGroups,
    sourceGroups,
    contentTypeGroups,
    availabilityGroups,
    channelSubscriptions,
  } = useMemo(
    () =>
      buildLibraryLessonView({
        query,
        selectedLessonId,
        activeScopeId,
        selectedTeacherId,
        selectedCollectionId,
        selectedSourceId,
        selectedChannelId,
        selectedContentType,
        selectedAvailability,
        lessons: snapshot.lessons,
        teachers: snapshot.teachers,
        collections: snapshot.collections,
        sources: snapshot.sources,
        mediaFiles: snapshot.mediaFiles,
        teacherRelays: snapshot.teacherRelays,
        trustedCurators: snapshot.trustedCurators,
        watchState: snapshot.watchState,
      }),
    [
      activeScopeId,
      query,
      selectedCollectionId,
      selectedAvailability,
      selectedChannelId,
      selectedContentType,
      selectedLessonId,
      selectedSourceId,
      selectedTeacherId,
      snapshot.collections,
      snapshot.lessons,
      snapshot.mediaFiles,
      snapshot.sources,
      snapshot.teachers,
      snapshot.teacherRelays,
      snapshot.trustedCurators,
      snapshot.watchState,
    ],
  );
  const selectedMediaFile = selectedLesson ? mediaByLessonId.get(selectedLesson.id) : undefined;
  const selectedMediaLessonId = selectedLesson?.id ?? "";
  const selectedMediaContentType = selectedLesson?.contentType;
  const selectedMediaFileId = selectedMediaFile?.id ?? "";

  useEffect(() => {
    let isMounted = true;

    setSelectedMediaUrl("");
    setSelectedMediaError("");

    if (!selectedMediaLessonId || !selectedMediaFileId || selectedMediaContentType === "post") {
      return () => {
        isMounted = false;
      };
    }

    resolveMediaFileUrl(selectedMediaFileId)
      .then((url) => {
        if (isMounted) {
          setSelectedMediaUrl(url);
        }
      })
      .catch((error: unknown) => {
        if (!isMounted) {
          return;
        }

        const message = error instanceof Error ? error.message : String(error);
        setSelectedMediaError(message);
      });

    return () => {
      isMounted = false;
    };
  }, [selectedMediaContentType, selectedMediaFileId, selectedMediaLessonId]);

  const phoneEligibleMediaCount = snapshot.lessons.filter((lesson) => {
    const mediaFile = mediaByLessonId.get(lesson.id);
    return (
      mediaFile?.importStatus === "ready" &&
      (lesson.contentType === "video" || lesson.contentType === "audio")
    );
  }).length;

  const handleSaveWatchState = async (
    lessonId: string,
    progressSeconds: number,
    durationSeconds: number | undefined,
    completed: boolean,
  ) => {
    try {
      const savedState = await saveWatchState(
        lessonId,
        Math.max(0, Math.floor(progressSeconds)),
        durationSeconds && Number.isFinite(durationSeconds) ? Math.floor(durationSeconds) : undefined,
        completed,
      );
      setSnapshot((current) => {
        const nextWatchState = current.watchState.some((state) => state.lessonId === lessonId)
          ? current.watchState.map((state) =>
              state.lessonId === lessonId ? savedState : state,
            )
          : current.watchState.concat(savedState);
        const nextLessons =
          durationSeconds && Number.isFinite(durationSeconds)
            ? current.lessons.map((lesson) =>
                lesson.id === lessonId
                  ? { ...lesson, durationSeconds: Math.floor(durationSeconds) }
                  : lesson,
              )
            : current.lessons;

        return {
          ...current,
          lessons: nextLessons,
          watchState: nextWatchState,
        };
      });
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setSystemNotice(message);
    }
  };

  const handleSaveLessonNote = async (lessonId: string, body: string) => {
    try {
      const savedNote = await saveLessonNote(lessonId, body);
      setSnapshot((current) => {
        const withoutNote = current.lessonNotes.filter((note) => note.lessonId !== lessonId);
        return {
          ...current,
          lessonNotes: savedNote.body ? withoutNote.concat(savedNote) : withoutNote,
        };
      });
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setSystemNotice(message);
    }
  };

  const handleUpdateLessonOrganization = async (
    lesson: Lesson,
    teacherDisplayName: string,
    collectionTitle: string,
  ) => {
    try {
      const updatedLesson = await updateLessonOrganization(
        lesson.id,
        teacherDisplayName,
        collectionTitle,
      );
      setSelectedLessonId(updatedLesson.id);
      await refreshSnapshot("Lesson organization updated.");
      setSystemNotice("Lesson organization updated.");
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setSystemNotice(message);
    }
  };

  const handleStartPhoneAccess = async () => {
    try {
      setPhoneAccessBusyAction("start");
      setPhoneAccessNotice("");
      const session = await startPhoneMediaSession();
      setPhoneSession(session.active ? session : null);
      setPhoneAccessNotice(session.messages.join(" "));
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setPhoneAccessNotice(message);
    } finally {
      setPhoneAccessBusyAction(null);
    }
  };

  const handleStopPhoneAccess = async () => {
    if (!phoneSession?.id) {
      return;
    }

    try {
      setPhoneAccessBusyAction("stop");
      const session = await stopPhoneMediaSession(phoneSession.id);
      setPhoneSession(null);
      setPhoneAccessNotice(session.messages.join(" "));
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setPhoneAccessNotice(message);
    } finally {
      setPhoneAccessBusyAction(null);
    }
  };

  const handleCopyPhoneLink = async (playlistUrl?: string) => {
    const link = playlistUrl ?? phoneSession?.playlistUrl;
    if (!link) {
      return;
    }

    try {
      await navigator.clipboard.writeText(link);
      setPhoneAccessNotice("Phone link copied.");
    } catch {
      setPhoneAccessNotice("Copy failed. Select and copy the link manually.");
    }
  };

  const handleImportResult = (notice: string) => {
    setSystemNotice(notice);
    setIsImportOpen(false);
    void refreshSnapshot(notice);
  };

  const openImport = (mode: ImportMode = "local") => {
    recordOnboardingLane("study");
    setInitialImportMode(mode);
    setIsImportOpen(true);
  };

  const openPublishFromFirstRun = () => {
    recordOnboardingLane("publish");
    setViewMode("publish");
  };

  const handleClearSource = async (source: Source) => {
    const lessonCount = snapshot.lessons.filter((lesson) => lesson.sourceId === source.id).length;
    const jobCount = snapshot.jobs.filter((job) => job.sourceId === source.id).length;
    const removeSource = !DEFAULT_SOURCE_IDS.has(source.id);
    const action = removeSource ? "remove this source and clear" : "clear";
    const confirmed = window.confirm(
      `This will ${action} ${lessonCount} item(s) and ${jobCount} job(s) for "${source.label}". This cannot be undone.`,
    );

    if (!confirmed) {
      return;
    }

    try {
      setBusySourceAction({ sourceId: source.id, action: "clear" });
      const result = await clearSourceContent(source.id, removeSource);
      setSystemNotice(result.messages.join(" "));
      await refreshSnapshot(result.messages.join(" "));
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setSystemNotice(message);
    } finally {
      setBusySourceAction(null);
    }
  };

  const handleAuditMediaStorage = async () => {
    try {
      setMediaStorageBusyAction("audit");
      const result = await auditMediaStorage();
      setMediaStorageAudit(result);
      setSystemNotice(result.messages.join(" "));
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setSystemNotice(message);
    } finally {
      setMediaStorageBusyAction(null);
    }
  };

  const handleCleanupMediaStorage = async () => {
    const staleFiles = mediaStorageAudit?.staleFiles ?? 0;
    const staleBytes = mediaStorageAudit?.staleBytes ?? 0;
    if (staleFiles <= 0) {
      setSystemNotice("Run a storage scan first; no stale library files are currently listed.");
      return;
    }
    const confirmed = window.confirm(
      `Remove ${staleFiles} unreferenced file(s) from the app library and reclaim ${formatBytes(staleBytes)}? This cannot be undone.`,
    );
    if (!confirmed) {
      return;
    }

    try {
      setMediaStorageBusyAction("cleanup");
      const result = await cleanupMediaStorage();
      setMediaStorageAudit(result.audit);
      setSystemNotice(result.messages.join(" "));
      await refreshSnapshot(result.messages.join(" "));
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setSystemNotice(message);
    } finally {
      setMediaStorageBusyAction(null);
    }
  };

  const handleDownloadSource = async (source: Source) => {
    const missingCount = getSourceStats(source.id, snapshot.lessons, snapshot.jobs).missingFileCount;

    let refreshTimer: number | undefined;

    try {
      setBusySourceAction({ sourceId: source.id, action: "download" });
      setSystemNotice(
        missingCount > 0
          ? `Downloading ${missingCount} missing file-backed item(s) from ${source.label}...`
          : `Checking existing downloads from ${source.label}...`,
      );
      refreshTimer = window.setInterval(() => {
        void refreshSnapshot();
      }, 4000);
      const result = await downloadSourceMedia(source.id);
      setSystemNotice(result.messages.join(" "));
      await refreshSnapshot(result.messages.join(" "));
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setSystemNotice(message);
    } finally {
      if (refreshTimer !== undefined) {
        window.clearInterval(refreshTimer);
      }
      setBusySourceAction(null);
    }
  };

  const sourceForChannel = (channel: ChannelSubscriptionView): Source | undefined =>
    channel.sourceId ? sourceById.get(channel.sourceId) : undefined;

  const handleSelectSourceFilter = (sourceId: string) => {
    setSelectedSourceId(sourceId);
    if (sourceId !== "all") {
      setSelectedChannelId("all");
    }
  };

  const handleSelectChannelFilter = (channelId: string) => {
    setSelectedChannelId(channelId);
    if (channelId !== "all") {
      setSelectedSourceId("all");
    }
  };

  const handleRefreshChannel = async (channel: ChannelSubscriptionView) => {
    if (!isOnlineMode) {
      setSystemNotice("Switch to online fetch mode before refreshing subscribed channels.");
      return;
    }

    const source = sourceForChannel(channel);
    if (!source) {
      setSystemNotice(`No editable source row is linked to "${channel.title}". Re-follow the channel link to manage it.`);
      return;
    }

    try {
      setSystemNotice(`Refreshing ${channel.title}...`);
      const result = await refreshSource(source.id);
      const counts = `${result.imported} added, ${result.skipped} skipped, ${result.failed} failed.`;
      const message = result.messages.join(" ");
      const notice = `${channel.title}: ${counts}${message ? ` ${message}` : ""}`;
      setSystemNotice(notice);
      await refreshSnapshot(notice);
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setSystemNotice(message);
    }
  };

  const handleDownloadChannel = async (channel: ChannelSubscriptionView) => {
    const source = sourceForChannel(channel);
    if (!source) {
      setSystemNotice(`No editable source row is linked to "${channel.title}". Re-follow the channel link to manage downloads.`);
      return;
    }

    await handleDownloadSource(source);
  };

  const handleUnfollowChannel = async (channel: ChannelSubscriptionView) => {
    const source = sourceForChannel(channel);
    if (!source) {
      setSystemNotice(`No editable source row is linked to "${channel.title}".`);
      return;
    }

    await handleClearSource(source);
  };

  const handleNativePlayback = async (mediaFile: MediaFile) => {
    try {
      setBusyNativeMediaId(mediaFile.id);
      const result = await playMediaFileNative(mediaFile.id);
      setSystemNotice(result.messages.join(" "));
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setSystemNotice(message);
    } finally {
      setBusyNativeMediaId(null);
    }
  };

  const handleAddTrustedCurator = async (
    displayName: string,
    publicKey: string,
    trustNote?: string,
  ): Promise<TrustedCurator> => {
    const curator = await addTrustedCurator(displayName, publicKey, trustNote);
    const notice = `Trusted curator key saved for ${curator.displayName}.`;
    setSystemNotice(notice);
    await refreshSnapshot(notice);
    return curator;
  };

  const handleRemoveTrustedCurator = async (curator: TrustedCurator) => {
    const confirmed = window.confirm(
      `Remove "${curator.displayName}" from trusted curators? Matching signed sources will become signed but untrusted.`,
    );

    if (!confirmed) {
      return;
    }

    try {
      const result = await removeTrustedCurator(curator.id);
      setSystemNotice(result.messages.join(" "));
      await refreshSnapshot(result.messages.join(" "));
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setSystemNotice(message);
    }
  };

  const handleRefreshEnabledSources = async () => {
    if (!isOnlineMode) {
      setSystemNotice("Switch to online fetch mode before refreshing remote sources.");
      return;
    }

    const refreshableSources = snapshot.sources.filter(
      (source) =>
        source.enabled &&
        !DEFAULT_SOURCE_IDS.has(source.id) &&
        (source.identifier.startsWith("http://") || source.identifier.startsWith("https://")),
    );

    if (refreshableSources.length === 0) {
      setSystemNotice("No enabled added sources with refreshable URLs were found.");
      return;
    }

    setIsRefreshingSources(true);
    setSystemNotice(`Refreshing ${refreshableSources.length} enabled source(s)...`);

    try {
      const summaries = [];

      for (const source of refreshableSources) {
        const result = await refreshSource(source.id);
        const message = result.messages.join(" ");
        const counts = `${result.imported} added, ${result.skipped} skipped, ${result.failed} failed.`;
        summaries.push(`${source.label}: ${counts}${message ? ` ${message}` : ""}`);
      }

      const notice = summaries.join(" ");
      setSystemNotice(notice);
      await refreshSnapshot(notice);
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setSystemNotice(message);
    } finally {
      setIsRefreshingSources(false);
    }
  };

  return (
    <div
      className={isSidebarCollapsed ? "app-shell app-shell-sidebar-collapsed" : "app-shell"}
    >
      <Sidebar
        viewMode={viewMode}
        setViewMode={setViewMode}
        isCollapsed={isSidebarCollapsed}
        onToggleCollapsed={() => setIsSidebarCollapsed((current) => !current)}
      />
      <main className="workspace">
        <TopBar
          viewMode={viewMode}
          snapshot={snapshot}
          query={query}
          setQuery={setQuery}
          isOnlineMode={isOnlineMode}
          setIsOnlineMode={setIsOnlineMode}
          runtimeDiagnostics={runtimeDiagnostics}
          openImport={() => openImport("local")}
        />
        {systemNotice ? (
          <div className="notice-bar" role="status">
            <AlertTriangle size={16} />
            <span>{systemNotice}</span>
            <button type="button" onClick={() => setSystemNotice("")}>
              Dismiss
            </button>
          </div>
        ) : null}

        {viewMode === "library" ? (
          <Dashboard
            snapshot={snapshot}
            filteredLessons={filteredLessons}
            continueLessons={continueLessons}
            newLessons={newLessons}
            smartScopes={smartScopes}
            activeScopeId={activeScopeId}
            setActiveScopeId={setActiveScopeId}
            teacherGroups={teacherGroups}
            collectionGroups={collectionGroups}
            sourceGroups={sourceGroups}
            contentTypeGroups={contentTypeGroups}
            availabilityGroups={availabilityGroups}
            channelSubscriptions={channelSubscriptions}
            selectedTeacherId={selectedTeacherId}
            selectedCollectionId={selectedCollectionId}
            selectedSourceId={selectedSourceId}
            selectedChannelId={selectedChannelId}
            selectedContentType={selectedContentType}
            selectedAvailability={selectedAvailability}
            isSearchActive={isSearchActive}
            query={query}
            selectedLesson={selectedLesson}
            selectedMediaFile={selectedMediaFile}
            selectedMediaUrl={selectedMediaUrl}
            selectedMediaError={selectedMediaError}
            selectedLessonNote={selectedLesson ? noteByLessonId.get(selectedLesson.id) : undefined}
            teacherById={teacherById}
            collectionById={collectionById}
            sourceById={sourceById}
            mediaByLessonId={mediaByLessonId}
            mediaThumbnailUrls={mediaThumbnailUrls}
            provenanceById={provenanceById}
            watchByLessonId={watchByLessonId}
            setSelectedLessonId={setSelectedLessonId}
            onOpenImport={openImport}
            onOpenPublish={openPublishFromFirstRun}
            onClearSearch={() => setQuery("")}
            runtimeDiagnostics={runtimeDiagnostics}
            phoneSession={phoneSession}
            phoneEligibleMediaCount={phoneEligibleMediaCount}
            phoneAccessBusyAction={phoneAccessBusyAction}
            phoneAccessNotice={phoneAccessNotice}
            busySourceAction={busySourceAction}
            busyNativeMediaId={busyNativeMediaId}
            onDownloadSource={handleDownloadSource}
            onRefreshChannel={handleRefreshChannel}
            onDownloadChannel={handleDownloadChannel}
            onUnfollowChannel={handleUnfollowChannel}
            onNativePlayback={handleNativePlayback}
            onMediaPlaybackError={setSelectedMediaError}
            onSaveWatchState={handleSaveWatchState}
            onSaveLessonNote={handleSaveLessonNote}
            onUpdateLessonOrganization={handleUpdateLessonOrganization}
            onSelectTeacher={setSelectedTeacherId}
            onSelectCollection={setSelectedCollectionId}
            onSelectSource={handleSelectSourceFilter}
            onSelectChannel={handleSelectChannelFilter}
            onSelectContentType={setSelectedContentType}
            onSelectAvailability={setSelectedAvailability}
            onStartPhoneAccess={handleStartPhoneAccess}
            onStopPhoneAccess={handleStopPhoneAccess}
            onCopyPhoneLink={handleCopyPhoneLink}
          />
        ) : null}

        {viewMode === "relays" ? (
          <RelaysView
            channelSubscriptions={channelSubscriptions}
            liveSessions={snapshot.liveSessions}
            teachers={teacherById}
            query={query}
            busySourceAction={busySourceAction}
            onOpenImport={openImport}
            onRefreshChannel={handleRefreshChannel}
            onDownloadChannel={handleDownloadChannel}
            onUnfollowChannel={handleUnfollowChannel}
          />
        ) : null}

        {viewMode === "publish" ? (
          <PublishView
            publisherProfiles={publisherProfiles}
            onPublisherProfilesChanged={refreshPublisherProfiles}
            onPublisherResult={setSystemNotice}
          />
        ) : null}

        {viewMode === "sources" ? (
          <SourcesView
            sources={snapshot.sources}
            lessons={snapshot.lessons}
            jobs={snapshot.jobs}
            runtimeDiagnostics={runtimeDiagnostics}
            trustedCurators={snapshot.trustedCurators}
            query={query}
            mediaStorageAudit={mediaStorageAudit}
            mediaStorageBusyAction={mediaStorageBusyAction}
            busySourceAction={busySourceAction}
            onClearSource={handleClearSource}
            onDownloadSource={handleDownloadSource}
            onRemoveTrustedCurator={handleRemoveTrustedCurator}
            onAuditMediaStorage={handleAuditMediaStorage}
            onCleanupMediaStorage={handleCleanupMediaStorage}
          />
        ) : null}

        {viewMode === "queue" ? (
          <QueueView
            jobs={snapshot.jobs}
            sources={sourceById}
            query={query}
            isRefreshingSources={isRefreshingSources}
            onRefreshEnabledSources={handleRefreshEnabledSources}
          />
        ) : null}
      </main>

      {isImportOpen ? (
        <ImportDrawer
          isOnlineMode={isOnlineMode}
          initialMode={initialImportMode}
          trustedCurators={snapshot.trustedCurators}
          onEnableFetching={() => setIsOnlineMode(true)}
          close={() => setIsImportOpen(false)}
          onResult={handleImportResult}
          onTrustCurator={handleAddTrustedCurator}
        />
      ) : null}
    </div>
  );
};

interface SidebarProps {
  viewMode: ViewMode;
  setViewMode: (mode: ViewMode) => void;
  isCollapsed: boolean;
  onToggleCollapsed: () => void;
}

const Sidebar = ({
  viewMode,
  setViewMode,
  isCollapsed,
  onToggleCollapsed,
}: SidebarProps) => {
  const navItems: { mode: ViewMode; label: string; icon: typeof Library }[] = [
    { mode: "library", label: "Library", icon: Library },
    { mode: "relays", label: "Channels", icon: Rss },
    { mode: "publish", label: "Publish", icon: UploadCloud },
    { mode: "sources", label: "Sources", icon: Database },
    { mode: "queue", label: "Update Queue", icon: History },
  ];

  return (
    <aside
      id="primary-sidebar"
      className={isCollapsed ? "sidebar sidebar-collapsed" : "sidebar"}
      aria-label="Primary"
      data-collapsed={isCollapsed ? "true" : "false"}
    >
      <div className="sidebar-header">
        <div className="brand-lockup">
          <div className="brand-mark">
            <BookOpen size={22} />
          </div>
          <div className="brand-copy">
            <p className="brand-name">Duroos Watcher</p>
            <p className="brand-subtitle">Local study library</p>
          </div>
        </div>
        <button
          type="button"
          className="sidebar-toggle"
          onClick={onToggleCollapsed}
          aria-controls="primary-sidebar"
          aria-expanded={!isCollapsed}
          aria-label={isCollapsed ? "Expand sidebar" : "Collapse sidebar"}
          title={isCollapsed ? "Expand sidebar" : "Collapse sidebar"}
        >
          {isCollapsed ? <PanelLeftOpen size={18} /> : <PanelLeftClose size={18} />}
        </button>
      </div>

      <nav className="nav-stack">
        {navItems.map(({ mode, label, icon: Icon }) => (
          <button
            key={mode}
            type="button"
            className={viewMode === mode ? "nav-item nav-item-active" : "nav-item"}
            onClick={() => setViewMode(mode)}
            aria-label={label}
            aria-current={viewMode === mode ? "page" : undefined}
            title={label}
          >
            <Icon size={18} aria-hidden="true" />
            <span>{label}</span>
          </button>
        ))}
      </nav>

      <div className="privacy-panel">
        <div className="privacy-heading">
          <ShieldCheck size={18} />
          <span>Privacy Defaults</span>
        </div>
        <ul>
          <li>No accounts</li>
          <li>No telemetry</li>
          <li>Local credentials only</li>
        </ul>
      </div>

      <div
        className="privacy-rail"
        aria-label="Privacy defaults: no accounts, no telemetry, local credentials only"
        title="Privacy defaults: no accounts, no telemetry, local credentials only"
      >
        <ShieldCheck size={18} />
      </div>

    </aside>
  );
};

interface TopBarProps {
  viewMode: ViewMode;
  snapshot: AppSnapshot;
  query: string;
  setQuery: (query: string) => void;
  isOnlineMode: boolean;
  setIsOnlineMode: (online: boolean) => void;
  runtimeDiagnostics: RuntimeDiagnostics;
  openImport: () => void;
}

const TopBar = ({
  viewMode,
  snapshot,
  query,
  setQuery,
  isOnlineMode,
  setIsOnlineMode,
  runtimeDiagnostics,
  openImport,
}: TopBarProps) => {
  const fileBackedLessons = snapshot.lessons.filter((lesson) =>
    isFileBackedContentType(lesson.contentType),
  );
  const missingFileCount = fileBackedLessons.filter((lesson) => !lesson.mediaFileId).length;
  const runningJobs = snapshot.jobs.filter((job) => job.state === "queued" || job.state === "running");
  const title = viewTitle(viewMode);
  const searchConfig = searchConfigForView(viewMode);

  const downloader = downloaderStatus(runtimeDiagnostics);
  const runtimeLabel = isTauriRuntime() ? "Desktop runtime" : "Browser preview";
  const runtimeStatusLabel = `Runtime status: ${runtimeLabel}. Downloader status: ${downloader.label}.`;

  return (
    <header className="top-bar">
      <div className="top-bar-main">
        <div className="top-bar-title">
          <p>Duroos Watcher</p>
          <h1>{title}</h1>
          <div className="top-bar-meta">
            <StatusChip label={`${snapshot.lessons.length} items`} tone="neutral" />
            <StatusChip label={`${snapshot.mediaFiles.length} local files`} tone="positive" />
            <StatusChip
              label={`${missingFileCount} need files`}
              tone={missingFileCount > 0 ? "warning" : "positive"}
            />
            <StatusChip
              label={`${runningJobs.length} active jobs`}
              tone={runningJobs.length > 0 ? "warning" : "neutral"}
            />
          </div>
        </div>
        <div className="top-actions">
          <div
            className="runtime-status-group"
            role="status"
            aria-live="polite"
            aria-label={runtimeStatusLabel}
          >
            <span className="runtime-status-item">
              <HardDrive size={15} aria-hidden="true" />
              <span>{runtimeLabel}</span>
            </span>
            <span
              className={
                downloader.tone === "positive"
                  ? "runtime-status-item runtime-status-positive"
                  : downloader.tone === "warning"
                    ? "runtime-status-item runtime-status-warning"
                    : "runtime-status-item"
              }
              title={runtimeDiagnostics.messages.join(" ")}
            >
              <Download size={15} aria-hidden="true" />
              <span>{downloader.label}</span>
            </span>
          </div>
          <button
            type="button"
            className={isOnlineMode ? "mode-toggle mode-toggle-online" : "mode-toggle"}
            onClick={() => setIsOnlineMode(!isOnlineMode)}
            aria-pressed={isOnlineMode}
          >
            {isOnlineMode ? <Wifi size={18} /> : <WifiOff size={18} />}
            <span>{isOnlineMode ? "Fetching enabled" : "Offline mode"}</span>
          </button>
          <button type="button" className="primary-action" onClick={openImport}>
            <Import size={17} />
            <span>Import</span>
          </button>
        </div>
      </div>
      {searchConfig ? (
        <div className="top-bar-search">
          <div className="search-wrap">
            <Search size={18} />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              aria-label={searchConfig.ariaLabel}
              placeholder={searchConfig.placeholder}
            />
          </div>
        </div>
      ) : null}
    </header>
  );
};

const viewTitle = (viewMode: ViewMode): string => {
  switch (viewMode) {
    case "library":
      return "Media Library";
    case "relays":
      return "Channels";
    case "publish":
      return "Publish";
    case "sources":
      return "Source Control";
    case "queue":
      return "Update Queue";
  }
};

interface DashboardProps {
  snapshot: AppSnapshot;
  filteredLessons: Lesson[];
  continueLessons: Lesson[];
  newLessons: Lesson[];
  smartScopes: { id: SmartScopeId; label: string; count: number }[];
  activeScopeId: SmartScopeId;
  setActiveScopeId: (scopeId: SmartScopeId) => void;
  teacherGroups: LibraryGroup[];
  collectionGroups: LibraryGroup[];
  sourceGroups: LibraryGroup[];
  contentTypeGroups: LibraryGroup[];
  availabilityGroups: LibraryGroup[];
  channelSubscriptions: ChannelSubscriptionView[];
  selectedTeacherId: string;
  selectedCollectionId: string;
  selectedSourceId: string;
  selectedChannelId: string;
  selectedContentType: LibraryContentTypeFilter;
  selectedAvailability: LibraryAvailabilityFilter;
  isSearchActive: boolean;
  query: string;
  selectedLesson?: Lesson;
  selectedMediaFile?: MediaFile;
  selectedMediaUrl: string;
  selectedMediaError: string;
  selectedLessonNote?: LessonNote;
  teacherById: Map<string, Teacher>;
  collectionById: Map<string, Collection>;
  sourceById: Map<string, Source>;
  mediaByLessonId: Map<string, MediaFile>;
  mediaThumbnailUrls: Record<string, string>;
  provenanceById: Map<string, ProvenanceRecord>;
  watchByLessonId: Map<string, WatchState>;
  setSelectedLessonId: (lessonId: string) => void;
  onOpenImport: (mode?: ImportMode) => void;
  onOpenPublish: () => void;
  onClearSearch: () => void;
  runtimeDiagnostics: RuntimeDiagnostics;
  phoneSession: PhoneMediaSession | null;
  phoneEligibleMediaCount: number;
  phoneAccessBusyAction: PhoneAccessBusyAction;
  phoneAccessNotice: string;
  busySourceAction: BusySourceAction;
  busyNativeMediaId: string | null;
  onDownloadSource: (source: Source) => void;
  onRefreshChannel: (channel: ChannelSubscriptionView) => void;
  onDownloadChannel: (channel: ChannelSubscriptionView) => void;
  onUnfollowChannel: (channel: ChannelSubscriptionView) => void;
  onNativePlayback: (mediaFile: MediaFile) => void;
  onMediaPlaybackError: (message: string) => void;
  onSaveWatchState: (
    lessonId: string,
    progressSeconds: number,
    durationSeconds: number | undefined,
    completed: boolean,
  ) => void;
  onSaveLessonNote: (lessonId: string, body: string) => void;
  onUpdateLessonOrganization: (
    lesson: Lesson,
    teacherDisplayName: string,
    collectionTitle: string,
  ) => void;
  onSelectTeacher: (teacherId: string) => void;
  onSelectCollection: (collectionId: string) => void;
  onSelectSource: (sourceId: string) => void;
  onSelectChannel: (channelId: string) => void;
  onSelectContentType: (contentType: LibraryContentTypeFilter) => void;
  onSelectAvailability: (availability: LibraryAvailabilityFilter) => void;
  onStartPhoneAccess: () => void;
  onStopPhoneAccess: () => void;
  onCopyPhoneLink: (playlistUrl?: string) => void;
}

const Dashboard = ({
  snapshot,
  filteredLessons,
  continueLessons,
  newLessons,
  smartScopes,
  activeScopeId,
  setActiveScopeId,
  teacherGroups,
  collectionGroups,
  sourceGroups,
  contentTypeGroups,
  availabilityGroups,
  channelSubscriptions,
  selectedTeacherId,
  selectedCollectionId,
  selectedSourceId,
  selectedChannelId,
  selectedContentType,
  selectedAvailability,
  isSearchActive,
  query,
  selectedLesson,
  selectedMediaFile,
  selectedMediaUrl,
  selectedMediaError,
  selectedLessonNote,
  teacherById,
  collectionById,
  sourceById,
  mediaByLessonId,
  mediaThumbnailUrls,
  provenanceById,
  watchByLessonId,
  setSelectedLessonId,
  onOpenImport,
  onOpenPublish,
  onClearSearch,
  runtimeDiagnostics,
  phoneSession,
  phoneEligibleMediaCount,
  phoneAccessBusyAction,
  phoneAccessNotice,
  busySourceAction,
  busyNativeMediaId,
  onDownloadSource,
  onRefreshChannel,
  onDownloadChannel,
  onUnfollowChannel,
  onNativePlayback,
  onMediaPlaybackError,
  onSaveWatchState,
  onSaveLessonNote,
  onUpdateLessonOrganization,
  onSelectTeacher,
  onSelectCollection,
  onSelectSource,
  onSelectChannel,
  onSelectContentType,
  onSelectAvailability,
  onStartPhoneAccess,
  onStopPhoneAccess,
  onCopyPhoneLink,
}: DashboardProps) => {
  if (snapshot.lessons.length === 0 && !isSearchActive) {
    return (
      <FirstRunDashboard
        sources={snapshot.sources}
        runtimeDiagnostics={runtimeDiagnostics}
        onOpenImport={onOpenImport}
        onOpenPublish={onOpenPublish}
      />
    );
  }

  const selectedChannel =
    selectedChannelId === "all"
      ? undefined
      : channelSubscriptions.find(
          (channel) =>
            channel.id === selectedChannelId ||
            channel.relayId === selectedChannelId ||
            channel.sourceId === selectedChannelId,
        );
  const selectedContentTypeGroup =
    selectedContentType === "all"
      ? undefined
      : contentTypeGroups.find((group) => group.id === selectedContentType);
  const selectedAvailabilityGroup =
    selectedAvailability === "all"
      ? undefined
      : availabilityGroups.find((group) => group.id === selectedAvailability);

  return (
  <div className="dashboard-grid">
    <section className="content-column">
      <SmartLibraryControls
        scopes={smartScopes}
        activeScopeId={activeScopeId}
        selectedTeacher={selectedTeacherId === "all" ? undefined : teacherById.get(selectedTeacherId)}
        selectedCollection={
          selectedCollectionId === "all" ? undefined : collectionById.get(selectedCollectionId)
        }
        selectedSource={selectedSourceId === "all" ? undefined : sourceById.get(selectedSourceId)}
        selectedChannel={selectedChannel}
        selectedContentType={selectedContentTypeGroup}
        selectedAvailability={selectedAvailabilityGroup}
        onSelectScope={setActiveScopeId}
        onClearTeacher={() => onSelectTeacher("all")}
        onClearCollection={() => onSelectCollection("all")}
        onClearSource={() => onSelectSource("all")}
        onClearChannel={() => onSelectChannel("all")}
        onClearContentType={() => onSelectContentType("all")}
        onClearAvailability={() => onSelectAvailability("all")}
      />
      {selectedLesson ? (
        <PlayerPanel
          lesson={selectedLesson}
          teacher={teacherById.get(selectedLesson.teacherId)}
          collection={collectionById.get(selectedLesson.collectionId)}
          source={sourceById.get(selectedLesson.sourceId)}
          mediaFile={selectedMediaFile}
          thumbnailUrl={selectedMediaFile ? mediaThumbnailUrls[selectedMediaFile.id] : undefined}
          mediaUrl={selectedMediaUrl}
          mediaError={selectedMediaError}
          note={selectedLessonNote}
          provenance={provenanceById.get(selectedLesson.provenanceId)}
          watchState={watchByLessonId.get(selectedLesson.id)}
          progress={getLessonProgress(
            selectedLesson,
            watchByLessonId.get(selectedLesson.id),
          )}
          busySourceAction={busySourceAction}
          runtimeDiagnostics={runtimeDiagnostics}
          busyNativeMediaId={busyNativeMediaId}
          onDownloadSource={onDownloadSource}
          onNativePlayback={onNativePlayback}
          onMediaPlaybackError={onMediaPlaybackError}
          onSaveWatchState={onSaveWatchState}
          onSaveLessonNote={onSaveLessonNote}
          onUpdateLessonOrganization={onUpdateLessonOrganization}
        />
      ) : (
        isSearchActive ? (
          <SearchEmptyPanel query={query} onClearSearch={onClearSearch} />
        ) : (
          <PlayerEmptyPanel onOpenImport={onOpenImport} runtimeDiagnostics={runtimeDiagnostics} />
        )
      )}

      <SectionHeader title="Continue" meta={`${continueLessons.length} active items`} />
      <div className="lesson-row">
        {continueLessons.length ? (
          continueLessons.map((lesson) => {
            const mediaFile = mediaByLessonId.get(lesson.id);
            return (
              <LessonCard
                key={lesson.id}
                lesson={lesson}
                teacher={teacherById.get(lesson.teacherId)}
                collection={collectionById.get(lesson.collectionId)}
                source={sourceById.get(lesson.sourceId)}
                mediaFile={mediaFile}
                thumbnailUrl={mediaFile ? mediaThumbnailUrls[mediaFile.id] : undefined}
                progress={getLessonProgress(lesson, watchByLessonId.get(lesson.id))}
                onSelect={() => setSelectedLessonId(lesson.id)}
              />
            );
          })
        ) : (
          <EmptyState
            icon={Play}
            title={isSearchActive ? "No matching active items" : "No active study items"}
            detail={
              isSearchActive
                ? "No in-progress lessons match this search."
                : "Start any imported video, audio, PDF, or post and it will appear here."
            }
          />
        )}
      </div>

      <SectionHeader
        title="New Items"
        meta={isSearchActive ? "Matching updates" : "Subscribed updates"}
      />
      <div className="lesson-grid">
        {newLessons.length ? (
          newLessons.map((lesson) => {
            const mediaFile = mediaByLessonId.get(lesson.id);
            return (
              <LessonCard
                key={lesson.id}
                lesson={lesson}
                teacher={teacherById.get(lesson.teacherId)}
                collection={collectionById.get(lesson.collectionId)}
                source={sourceById.get(lesson.sourceId)}
                mediaFile={mediaFile}
                thumbnailUrl={mediaFile ? mediaThumbnailUrls[mediaFile.id] : undefined}
                progress={getLessonProgress(lesson, watchByLessonId.get(lesson.id))}
                onSelect={() => setSelectedLessonId(lesson.id)}
              />
            );
          })
        ) : (
          <EmptyState
            icon={Rss}
            title={isSearchActive ? "No matching new items" : "No new items"}
            detail={
              isSearchActive
                ? "No source updates match this search."
                : "Use Import to add local media, a public feed, Archive item, or direct video URL."
            }
          />
        )}
      </div>

      <SectionHeader
        title="Subscribed Channels"
        meta={`${channelSubscriptions.length} followed`}
      />
      <div className="channel-card-list">
        {channelSubscriptions.length ? (
          channelSubscriptions.map((channel) => (
            <ChannelSubscriptionCard
              key={channel.id}
              channel={channel}
              selected={selectedChannel?.id === channel.id}
              busySourceAction={busySourceAction}
              onSelect={onSelectChannel}
              onRefresh={onRefreshChannel}
              onDownload={onDownloadChannel}
              onUnfollow={onUnfollowChannel}
            />
          ))
        ) : (
          <EmptyState
            icon={Rss}
            title="No followed channels"
            detail="Follow a signed teacher channel or curator manifest when one is available."
          />
        )}
      </div>

      <SectionHeader title="Library Search" meta={`${filteredLessons.length} items`} />
      <div className="library-list">
        {filteredLessons.length ? (
          filteredLessons.map((lesson) => {
            const mediaFile = mediaByLessonId.get(lesson.id);
            return (
              <LessonRow
                key={lesson.id}
                lesson={lesson}
                teacher={teacherById.get(lesson.teacherId)}
                collection={collectionById.get(lesson.collectionId)}
                source={sourceById.get(lesson.sourceId)}
                mediaFile={mediaFile}
                thumbnailUrl={mediaFile ? mediaThumbnailUrls[mediaFile.id] : undefined}
                onSelect={() => setSelectedLessonId(lesson.id)}
              />
            );
          })
        ) : (
          <EmptyState
            icon={Search}
            title={isSearchActive ? "No matching study items" : "No study items imported"}
            detail={
              isSearchActive
                ? "Clear the search or try a teacher, title, source, or URL."
                : "Import video, audio, PDF files, teacher posts, source feeds, or direct video URLs."
            }
          />
        )}
      </div>
    </section>

    <aside className="detail-column">
      <PhoneAccessPanel
        session={phoneSession}
        eligibleMediaCount={phoneEligibleMediaCount}
        busyAction={phoneAccessBusyAction}
        notice={phoneAccessNotice}
        desktopRuntimeAvailable={runtimeDiagnostics.desktopRuntimeAvailable}
        onStart={onStartPhoneAccess}
        onStop={onStopPhoneAccess}
        onCopyLink={onCopyPhoneLink}
      />
      <LibraryOrganizationPanel
        channels={channelSubscriptions}
        lessons={snapshot.lessons}
        mediaByLessonId={mediaByLessonId}
        contentTypeGroups={contentTypeGroups}
        availabilityGroups={availabilityGroups}
        selectedChannelId={selectedChannelId}
        selectedContentType={selectedContentType}
        selectedAvailability={selectedAvailability}
        onSelectChannel={onSelectChannel}
        onSelectContentType={onSelectContentType}
        onSelectAvailability={onSelectAvailability}
      />
      <SourceReadinessPanel
        sources={snapshot.sources}
        groups={sourceGroups}
        selectedSourceId={selectedSourceId}
        runtimeDiagnostics={runtimeDiagnostics}
        onSelectSource={onSelectSource}
      />
      <TeacherPanel
        groups={teacherGroups}
        selectedTeacherId={selectedTeacherId}
        onSelectTeacher={onSelectTeacher}
      />
      <LiveSessionPanel
        liveSessions={snapshot.liveSessions}
        teachers={teacherById}
      />
      <CoursesPanel
        groups={collectionGroups}
        selectedCollectionId={selectedCollectionId}
        onSelectCollection={onSelectCollection}
      />
      <QueueCompact jobs={snapshot.jobs} />
    </aside>
  </div>
  );
};

const FirstRunDashboard = ({
  sources,
  runtimeDiagnostics,
  onOpenImport,
  onOpenPublish,
}: {
  sources: Source[];
  runtimeDiagnostics: RuntimeDiagnostics;
  onOpenImport: (mode?: ImportMode) => void;
  onOpenPublish: () => void;
}) => {
  const downloader = downloaderStatus(runtimeDiagnostics);

  return (
    <div className="first-run-layout">
      <section className="first-run-panel" aria-label="Choose Duroos Watcher setup path">
        <div className="first-run-copy">
          <StatusChip label="Local-first" tone="positive" />
          <h2>Choose how you want to start</h2>
          <p>
            Start as a learner building a private study library, or as a teacher publishing a
            signed channel for learners to follow. Both paths keep accounts and telemetry out of
            the app.
          </p>
        </div>

        <div className="first-run-path-grid">
          <article className="first-run-path">
            <div className="round-icon">
              <Library size={17} />
            </div>
            <div>
              <strong>Study Library</strong>
              <span>Import lessons, follow sources, resume playback, keep notes, and use phone access.</span>
            </div>
            <div className="first-run-actions" aria-label="Start learner path">
              <button type="button" className="primary-action" onClick={() => onOpenImport("local")}>
                <FolderOpen size={17} />
                <span>Add Local Files</span>
              </button>
              <button type="button" className="secondary-action" onClick={() => onOpenImport("source")}>
                <Globe2 size={17} />
                <span>Add Source URL</span>
              </button>
              <button type="button" className="secondary-action" onClick={() => onOpenImport("feed")}>
                <Rss size={17} />
                <span>Follow Channel</span>
              </button>
            </div>
          </article>

          <article className="first-run-path">
            <div className="round-icon">
              <UploadCloud size={17} />
            </div>
            <div>
              <strong>Teacher Channel</strong>
              <span>Create a local signing profile, test relays and storage, then share one signed invite.</span>
            </div>
            <div className="first-run-actions" aria-label="Start publisher path">
              <button type="button" className="secondary-action" onClick={onOpenPublish}>
                <UploadCloud size={17} />
                <span>Set Up Publishing</span>
              </button>
            </div>
          </article>
        </div>

        <div className="first-run-trust-row">
          <StatusChip label="No account" tone="positive" />
          <StatusChip label="No telemetry" tone="positive" />
          <StatusChip label="Review-first downloads" tone="positive" />
          <StatusChip label={downloader.label} tone={downloader.tone} />
        </div>
      </section>

      <aside className="first-run-side">
        <section className="side-panel first-run-privacy">
          <SectionHeader title="Privacy Defaults" meta="always on" />
          <div className="publish-steps">
            <div>
              <ShieldCheck size={17} />
              <span>No Duroos account is created.</span>
            </div>
            <div>
              <HardDrive size={17} />
              <span>Media and watch state stay in the local app library.</span>
            </div>
            <div>
              <WifiOff size={17} />
              <span>Remote fetching stays off until you enable it.</span>
            </div>
          </div>
        </section>
        <SourceReadinessPanel sources={sources} runtimeDiagnostics={runtimeDiagnostics} />
      </aside>
    </div>
  );
};

const SmartLibraryControls = ({
  scopes,
  activeScopeId,
  selectedTeacher,
  selectedCollection,
  selectedSource,
  selectedChannel,
  selectedContentType,
  selectedAvailability,
  onSelectScope,
  onClearTeacher,
  onClearCollection,
  onClearSource,
  onClearChannel,
  onClearContentType,
  onClearAvailability,
}: {
  scopes: { id: SmartScopeId; label: string; count: number }[];
  activeScopeId: SmartScopeId;
  selectedTeacher?: Teacher;
  selectedCollection?: Collection;
  selectedSource?: Source;
  selectedChannel?: ChannelSubscriptionView;
  selectedContentType?: LibraryGroup;
  selectedAvailability?: LibraryGroup;
  onSelectScope: (scopeId: SmartScopeId) => void;
  onClearTeacher: () => void;
  onClearCollection: () => void;
  onClearSource: () => void;
  onClearChannel: () => void;
  onClearContentType: () => void;
  onClearAvailability: () => void;
}) => (
  <section className="smart-library-bar" aria-label="Smart library organization">
    <div className="smart-scope-row" role="tablist" aria-label="Library scope">
      {scopes.map((scope) => (
        <button
          key={scope.id}
          type="button"
          className={scope.id === activeScopeId ? "scope-chip scope-chip-active" : "scope-chip"}
          onClick={() => onSelectScope(scope.id)}
          aria-pressed={scope.id === activeScopeId}
        >
          <span>{scope.label}</span>
          <strong>{scope.count}</strong>
        </button>
      ))}
    </div>
    {selectedTeacher ||
    selectedCollection ||
    selectedSource ||
    selectedChannel ||
    selectedContentType ||
    selectedAvailability ? (
      <div className="active-filter-row" aria-label="Active library filters">
        {selectedTeacher ? (
          <button type="button" className="filter-chip" onClick={onClearTeacher}>
            <UserRound size={14} />
            <span>{selectedTeacher.displayName}</span>
            <X size={13} />
          </button>
        ) : null}
        {selectedCollection ? (
          <button type="button" className="filter-chip" onClick={onClearCollection}>
            <ListVideo size={14} />
            <span>{selectedCollection.title}</span>
            <X size={13} />
          </button>
        ) : null}
        {selectedSource ? (
          <button type="button" className="filter-chip" onClick={onClearSource}>
            <Database size={14} />
            <span>{selectedSource.label}</span>
            <X size={13} />
          </button>
        ) : null}
        {selectedChannel ? (
          <button type="button" className="filter-chip" onClick={onClearChannel}>
            <Rss size={14} />
            <span>{selectedChannel.title}</span>
            <X size={13} />
          </button>
        ) : null}
        {selectedContentType ? (
          <button type="button" className="filter-chip" onClick={onClearContentType}>
            {(() => {
              const Icon = contentTypeIcon(selectedContentType.id as ContentType);
              return <Icon size={14} />;
            })()}
            <span>{selectedContentType.label}</span>
            <X size={13} />
          </button>
        ) : null}
        {selectedAvailability ? (
          <button type="button" className="filter-chip" onClick={onClearAvailability}>
            <HardDrive size={14} />
            <span>{selectedAvailability.label}</span>
            <X size={13} />
          </button>
        ) : null}
      </div>
    ) : null}
  </section>
);

const SectionHeader = ({ title, meta }: { title: string; meta: string }) => (
  <div className="section-header">
    <h2>{title}</h2>
    <span>{meta}</span>
  </div>
);

interface LessonCardProps {
  lesson: Lesson;
  teacher?: Teacher;
  collection?: Collection;
  source?: Source;
  mediaFile?: MediaFile;
  thumbnailUrl?: string;
  progress: number;
  onSelect: () => void;
}

const LessonCard = ({
  lesson,
  teacher,
  collection,
  source,
  mediaFile,
  thumbnailUrl,
  progress,
  onSelect,
}: LessonCardProps) => (
  <button type="button" className="lesson-card" onClick={onSelect}>
    <LessonThumb
      tone={lesson.thumbnailTone}
      contentType={lesson.contentType}
      badge={lessonBadge(lesson)}
      thumbnailUrl={thumbnailUrl}
    />
    <div className="lesson-card-body">
      <h3>{lesson.title}</h3>
      <p>{teacher?.displayName ?? "Unknown teacher"}</p>
      <div className="lesson-meta-line">
        <span>{collection?.title ?? "Unsorted"}</span>
        <span>{source?.label ?? "Unknown source"}</span>
      </div>
      <div className="progress-track" aria-label={`${progress}% watched`}>
        <span style={{ width: `${progress}%` }} />
      </div>
      <div className="card-footer">
        <StatusChip label={contentTypeLabel(lesson.contentType)} tone="neutral" />
        <StatusChip label={availabilityLabel(lesson, mediaFile)} tone={availabilityTone(lesson, mediaFile)} />
        <ChevronRight size={16} />
      </div>
    </div>
  </button>
);

const LessonThumb = ({
  tone,
  contentType,
  badge,
  thumbnailUrl,
}: {
  tone: Lesson["thumbnailTone"];
  contentType: Lesson["contentType"];
  badge: string;
  thumbnailUrl?: string;
}) => (
  <div
    className={thumbnailUrl ? `lesson-thumb thumb-${tone} lesson-thumb-has-image` : `lesson-thumb thumb-${tone}`}
    aria-hidden="true"
  >
    {thumbnailUrl ? (
      <img src={thumbnailUrl} alt="" loading="lazy" />
    ) : (
      <div className="thumb-book">
        {(() => {
          const Icon = contentTypeIcon(contentType);
          return <Icon size={28} />;
        })()}
      </div>
    )}
    <span>{badge}</span>
  </div>
);

const lessonBadge = (lesson: Lesson): string => {
  if (lesson.contentType === "video" || lesson.contentType === "audio") {
    return formatDuration(lesson.durationSeconds);
  }

  return contentTypeLabel(lesson.contentType);
};

interface LessonRowProps {
  lesson: Lesson;
  teacher?: Teacher;
  collection?: Collection;
  source?: Source;
  mediaFile?: MediaFile;
  thumbnailUrl?: string;
  onSelect: () => void;
}

const LessonRow = ({
  lesson,
  teacher,
  collection,
  source,
  mediaFile,
  thumbnailUrl,
  onSelect,
}: LessonRowProps) => (
  <button type="button" className="lesson-row-item" onClick={onSelect}>
    <LessonThumb
      tone={lesson.thumbnailTone}
      contentType={lesson.contentType}
      badge={lessonBadge(lesson)}
      thumbnailUrl={thumbnailUrl}
    />
    <div className="lesson-row-copy">
      <h3>{lesson.title}</h3>
      <p>{lesson.description}</p>
      <div className="lesson-meta-line">
        <span>{teacher?.displayName ?? "Unknown teacher"}</span>
        <span>{collection?.title ?? "Unsorted"}</span>
        <span>{source?.label ?? "Unknown source"}</span>
      </div>
    </div>
    <div className="lesson-row-status">
      <StatusChip label={contentTypeLabel(lesson.contentType)} tone="neutral" />
      <StatusChip label={availabilityLabel(lesson, mediaFile)} tone={availabilityTone(lesson, mediaFile)} />
    </div>
  </button>
);

interface PlayerPanelProps {
  lesson: Lesson;
  teacher?: Teacher;
  collection?: Collection;
  source?: Source;
  mediaFile?: MediaFile;
  thumbnailUrl?: string;
  mediaUrl: string;
  mediaError: string;
  note?: LessonNote;
  provenance?: ProvenanceRecord;
  watchState?: WatchState;
  progress: number;
  busySourceAction: BusySourceAction;
  runtimeDiagnostics: RuntimeDiagnostics;
  busyNativeMediaId: string | null;
  onDownloadSource: (source: Source) => void;
  onNativePlayback: (mediaFile: MediaFile) => void;
  onMediaPlaybackError: (message: string) => void;
  onSaveWatchState: (
    lessonId: string,
    progressSeconds: number,
    durationSeconds: number | undefined,
    completed: boolean,
  ) => void;
  onSaveLessonNote: (lessonId: string, body: string) => void;
  onUpdateLessonOrganization: (
    lesson: Lesson,
    teacherDisplayName: string,
    collectionTitle: string,
  ) => void;
}

const PlayerPanel = ({
  lesson,
  teacher,
  collection,
  source,
  mediaFile,
  thumbnailUrl,
  mediaUrl,
  mediaError,
  note,
  provenance,
  watchState,
  progress,
  busySourceAction,
  runtimeDiagnostics,
  busyNativeMediaId,
  onDownloadSource,
  onNativePlayback,
  onMediaPlaybackError,
  onSaveWatchState,
  onSaveLessonNote,
  onUpdateLessonOrganization,
}: PlayerPanelProps) => {
  const needsMediaFile =
    isFileBackedContentType(lesson.contentType) && (!mediaFile || Boolean(mediaError));
  const isDownloading =
    Boolean(source) &&
    busySourceAction?.sourceId === source?.id &&
    busySourceAction?.action === "download";
  const downloadBlocked = source?.capability.download === "blocked";
  const canDownloadMedia = Boolean(source) && !downloadBlocked;
  const canUseNativePlayback =
    Boolean(mediaFile) &&
    (lesson.contentType === "video" || lesson.contentType === "audio") &&
    runtimeDiagnostics.nativePlaybackAvailable;
  const isOpeningNativePlayer = mediaFile ? busyNativeMediaId === mediaFile.id : false;
  const activeMediaElementRef = useRef<HTMLMediaElement | null>(null);
  useEffect(() => {
    activeMediaElementRef.current = null;
  }, [lesson.id]);
  const resetActiveMediaElement = () => {
    const element = activeMediaElementRef.current;
    if (element) {
      element.currentTime = 0;
    }
  };

  return (
    <section className="player-panel" aria-label="Selected lesson">
      <PlayerSurface
        lesson={lesson}
        mediaFile={mediaFile}
        thumbnailUrl={thumbnailUrl}
        mediaUrl={mediaUrl}
        mediaError={mediaError}
        watchState={watchState}
        onMediaElementReady={(element) => {
          activeMediaElementRef.current = element;
        }}
        onMediaPlaybackError={onMediaPlaybackError}
        onSaveWatchState={onSaveWatchState}
      />
      <div className="player-copy">
        <h2>{lesson.title}</h2>
        <p>{teacher?.displayName ?? "Unknown teacher"}</p>
        <div className="player-tags">
          <StatusChip label={collection?.title ?? "Unsorted"} tone="neutral" />
          <StatusChip label={source?.label ?? "Source unknown"} tone="neutral" />
          <StatusChip label={contentTypeLabel(lesson.contentType)} tone="neutral" />
          <StatusChip label={availabilityLabel(lesson, mediaFile)} tone={availabilityTone(lesson, mediaFile)} />
        </div>
      </div>
      <LessonOrganizationEditor
        lesson={lesson}
        teacher={teacher}
        collection={collection}
        onUpdateLessonOrganization={onUpdateLessonOrganization}
      />
      {mediaFile && (lesson.contentType === "video" || lesson.contentType === "audio") ? (
        <div className="player-actions" aria-label="Native playback actions">
          <button
            type="button"
            className="secondary-action"
            onClick={() => onNativePlayback(mediaFile)}
            disabled={!canUseNativePlayback || isOpeningNativePlayer}
            title={
              runtimeDiagnostics.nativePlaybackAvailable
                ? `Open in ${runtimeDiagnostics.nativePlaybackPlayer ?? "native player"}.`
                : "Native playback needs VLC, mpv, or ffplay available locally."
            }
          >
            <Play size={15} />
            <span>{isOpeningNativePlayer ? "Opening" : "Open Native Player"}</span>
          </button>
          <span>
            {runtimeDiagnostics.nativePlaybackAvailable
              ? `Uses ${runtimeDiagnostics.nativePlaybackPlayer ?? "a native player"} for broad codec playback.`
              : "Native player unavailable on this runtime."}
          </span>
        </div>
      ) : null}
      {needsMediaFile ? (
        <div className="player-actions" aria-label="Media actions">
          <button
            type="button"
            className="secondary-action"
            onClick={() => {
              if (source) {
                onDownloadSource(source);
              }
            }}
            disabled={!canDownloadMedia || isDownloading}
            title={
              downloadBlocked
                ? "This source type does not currently support downloads."
                : "Download missing or invalid media files into the local library."
            }
          >
            <Download size={15} />
            <span>{isDownloading ? "Downloading" : "Download Media"}</span>
          </button>
          <span>
            Download missing or invalid media from this source into the local library before playback.
          </span>
        </div>
      ) : null}
      <div className="progress-track progress-large" aria-label={`${progress}% watched`}>
        <span style={{ width: `${progress}%` }} />
      </div>
      <StudyProgressActions
        lesson={lesson}
        watchState={watchState}
        onResetPlayback={resetActiveMediaElement}
        onSaveWatchState={onSaveWatchState}
      />
      <div className="provenance-box">
        <div>
          <ShieldCheck size={17} />
          <strong>Source Record</strong>
        </div>
        <p>{provenance?.permissionNote ?? "No source record found."}</p>
        <code>{provenance?.originUrl ?? lesson.sourceUrl}</code>
      </div>
      <LessonNotesPanel
        lesson={lesson}
        note={note}
        onSaveLessonNote={onSaveLessonNote}
      />
    </section>
  );
};

const LessonOrganizationEditor = ({
  lesson,
  teacher,
  collection,
  onUpdateLessonOrganization,
}: {
  lesson: Lesson;
  teacher?: Teacher;
  collection?: Collection;
  onUpdateLessonOrganization: (
    lesson: Lesson,
    teacherDisplayName: string,
    collectionTitle: string,
  ) => void;
}) => {
  const defaultTeacher = teacher?.displayName ?? "Personal Library";
  const defaultCollection = collection?.title ?? "Local Imports";
  const [teacherName, setTeacherName] = useState(defaultTeacher);
  const [collectionTitle, setCollectionTitle] = useState(defaultCollection);

  useEffect(() => {
    setTeacherName(defaultTeacher);
    setCollectionTitle(defaultCollection);
  }, [defaultCollection, defaultTeacher, lesson.id]);

  const hasChanges =
    teacherName.trim() !== defaultTeacher || collectionTitle.trim() !== defaultCollection;

  return (
    <div className="organization-editor">
      <label className="field compact-field">
        <span>Teacher</span>
        <input
          value={teacherName}
          onChange={(event) => setTeacherName(event.target.value)}
        />
      </label>
      <label className="field compact-field">
        <span>Course</span>
        <input
          value={collectionTitle}
          onChange={(event) => setCollectionTitle(event.target.value)}
        />
      </label>
      <button
        type="button"
        className="secondary-action"
        onClick={() => onUpdateLessonOrganization(lesson, teacherName, collectionTitle)}
        disabled={!hasChanges || !teacherName.trim() || !collectionTitle.trim()}
      >
        <FolderOpen size={15} />
        <span>Save</span>
      </button>
    </div>
  );
};

const StudyProgressActions = ({
  lesson,
  watchState,
  onResetPlayback,
  onSaveWatchState,
}: {
  lesson: Lesson;
  watchState?: WatchState;
  onResetPlayback: () => void;
  onSaveWatchState: (
    lessonId: string,
    progressSeconds: number,
    durationSeconds: number | undefined,
    completed: boolean,
  ) => void;
}) => {
  const duration = lesson.durationSeconds;
  const markCompleteProgress = duration ?? watchState?.progressSeconds ?? 0;

  return (
    <div className="study-actions" aria-label="Study progress actions">
      <button
        type="button"
        className="secondary-action"
        onClick={() => onSaveWatchState(lesson.id, markCompleteProgress, duration, true)}
        disabled={watchState?.completed}
      >
        <CheckCircle2 size={15} />
        <span>Mark Complete</span>
      </button>
      <button
        type="button"
        className="secondary-action"
        onClick={() => {
          onResetPlayback();
          onSaveWatchState(lesson.id, 0, duration, false);
        }}
        disabled={!watchState}
      >
        <Square size={15} />
        <span>Reset</span>
      </button>
    </div>
  );
};

const LessonNotesPanel = ({
  lesson,
  note,
  onSaveLessonNote,
}: {
  lesson: Lesson;
  note?: LessonNote;
  onSaveLessonNote: (lessonId: string, body: string) => void;
}) => {
  const savedBodyRef = useRef(note?.body ?? "");
  const [body, setBody] = useState(note?.body ?? "");

  useEffect(() => {
    const nextBody = note?.body ?? "";
    savedBodyRef.current = nextBody;
    setBody(nextBody);
  }, [lesson.id, note?.body]);

  useEffect(() => {
    if (body === savedBodyRef.current) {
      return;
    }

    const timer = window.setTimeout(() => {
      savedBodyRef.current = body;
      onSaveLessonNote(lesson.id, body);
    }, 650);

    return () => window.clearTimeout(timer);
  }, [body, lesson.id, onSaveLessonNote]);

  return (
    <div className="lesson-notes-panel">
      <div className="notes-heading">
        <MessageSquare size={16} />
        <strong>Study Note</strong>
        <span>{note?.updatedAt ? `Saved ${formatDate(note.updatedAt)}` : "No note"}</span>
      </div>
      <textarea
        value={body}
        onChange={(event) => setBody(event.target.value)}
        rows={4}
        aria-label="Study note"
      />
    </div>
  );
};

const PlayerSurface = ({
  lesson,
  mediaFile,
  thumbnailUrl,
  mediaUrl,
  mediaError,
  watchState,
  onMediaElementReady,
  onMediaPlaybackError,
  onSaveWatchState,
}: {
  lesson: Lesson;
  mediaFile?: MediaFile;
  thumbnailUrl?: string;
  mediaUrl: string;
  mediaError: string;
  watchState?: WatchState;
  onMediaElementReady: (element: HTMLMediaElement | null) => void;
  onMediaPlaybackError: (message: string) => void;
  onSaveWatchState: (
    lessonId: string,
    progressSeconds: number,
    durationSeconds: number | undefined,
    completed: boolean,
  ) => void;
}) => {
  const lastSavedSecondRef = useRef(0);
  const handlePlaybackError = () => {
    onMediaPlaybackError(
      "Downloaded file is structurally valid, but the desktop WebView cannot decode this codec. Use a WebKit-compatible MP4 video with H.264/AAC, or a supported audio file.",
    );
  };
  const restoreProgress = (element: HTMLMediaElement) => {
    const duration = Number.isFinite(element.duration) ? Math.floor(element.duration) : undefined;
    const progressSeconds = watchState?.completed ? 0 : (watchState?.progressSeconds ?? 0);
    if (progressSeconds > 5 && duration && progressSeconds < duration - 5) {
      element.currentTime = progressSeconds;
      lastSavedSecondRef.current = progressSeconds;
    }
    if (duration && duration !== lesson.durationSeconds) {
      onSaveWatchState(lesson.id, progressSeconds, duration, false);
    }
  };
  const saveProgress = (element: HTMLMediaElement, completed = false) => {
    const currentSecond = Math.floor(element.currentTime || 0);
    const duration = Number.isFinite(element.duration) ? Math.floor(element.duration) : undefined;
    if (!completed && Math.abs(currentSecond - lastSavedSecondRef.current) < 10) {
      return;
    }

    lastSavedSecondRef.current = currentSecond;
    onSaveWatchState(lesson.id, currentSecond, duration, completed);
  };

  if (mediaUrl && lesson.contentType === "video") {
    return (
      <div className="player-frame player-frame-live">
        <video
          className="media-player"
          controls
          preload="metadata"
          ref={onMediaElementReady}
          src={mediaUrl}
          poster={thumbnailUrl}
          onError={handlePlaybackError}
          onLoadedMetadata={(event) => restoreProgress(event.currentTarget)}
          onTimeUpdate={(event) => saveProgress(event.currentTarget)}
          onPause={(event) => saveProgress(event.currentTarget)}
          onEnded={(event) => saveProgress(event.currentTarget, true)}
        />
      </div>
    );
  }

  if (mediaUrl && lesson.contentType === "audio") {
    return (
      <div className={`player-frame audio-player-frame thumb-${lesson.thumbnailTone}`}>
        <div className="audio-player-icon" aria-hidden="true">
          <Volume2 size={34} />
        </div>
        <audio
          className="audio-player"
          controls
          preload="metadata"
          ref={onMediaElementReady}
          src={mediaUrl}
          onError={handlePlaybackError}
          onLoadedMetadata={(event) => restoreProgress(event.currentTarget)}
          onTimeUpdate={(event) => saveProgress(event.currentTarget)}
          onPause={(event) => saveProgress(event.currentTarget)}
          onEnded={(event) => saveProgress(event.currentTarget, true)}
        />
      </div>
    );
  }

  if (mediaUrl && lesson.contentType === "pdf") {
    return (
      <div className="player-frame pdf-player-frame">
        <iframe className="pdf-player" src={mediaUrl} title={lesson.title} />
      </div>
    );
  }

  return (
    <div
      className={
        thumbnailUrl
          ? `player-frame thumb-${lesson.thumbnailTone} player-frame-has-image`
          : `player-frame thumb-${lesson.thumbnailTone}`
      }
    >
      {thumbnailUrl ? <img className="player-cover-image" src={thumbnailUrl} alt="" /> : null}
      <div className="play-button" aria-hidden="true">
        {(() => {
          const Icon = mediaFile && lesson.contentType === "video" ? Play : contentTypeIcon(lesson.contentType);
          return <Icon size={30} fill={lesson.contentType === "video" && mediaFile ? "currentColor" : "none"} />;
        })()}
      </div>
      <span className="player-duration">{lessonBadge(lesson)}</span>
      {mediaFile ? (
        <span className="player-frame-message">
          {mediaError || "Preparing local media..."}
        </span>
      ) : null}
    </div>
  );
};

const PlayerEmptyPanel = ({
  onOpenImport,
  runtimeDiagnostics,
}: {
  onOpenImport: () => void;
  runtimeDiagnostics: RuntimeDiagnostics;
}) => {
  const downloader = downloaderStatus(runtimeDiagnostics);

  return (
    <section className="player-panel player-empty-panel" aria-label="Media viewer">
      <div className="player-frame player-frame-live player-empty-surface">
        <div className="empty-player-icons" aria-hidden="true">
          <Video size={30} />
          <Volume2 size={30} />
          <FileText size={30} />
        </div>
        <span className="player-frame-message">
          Import local media or add a source to start building the watch queue.
        </span>
      </div>
      <div className="player-copy">
        <h2>Ready for video, audio, and PDFs</h2>
        <p>
          Local files play from the app library. Source downloads stay user-initiated and visible in
          the update queue.
        </p>
        <div className="player-tags">
          <StatusChip label="Local-first" tone="positive" />
          <StatusChip label="Duplicate checks" tone="positive" />
          <StatusChip label={downloader.label} tone={downloader.tone} />
        </div>
      </div>
      <button type="button" className="primary-action player-import-action" onClick={onOpenImport}>
        <Import size={17} />
        <span>Import Content</span>
      </button>
    </section>
  );
};

const SearchEmptyPanel = ({
  query,
  onClearSearch,
}: {
  query: string;
  onClearSearch: () => void;
}) => (
  <section className="player-panel player-empty-panel" aria-label="Search results">
    <div className="player-frame player-frame-live player-empty-surface">
      <div className="empty-player-icons" aria-hidden="true">
        <Search size={30} />
      </div>
      <span className="player-frame-message">
        No library items match "{query.trim()}".
      </span>
    </div>
    <div className="player-copy">
      <h2>No matching study items</h2>
      <p>Search checks lesson titles, descriptions, URLs, teachers, collections, and sources.</p>
      <div className="player-tags">
        <StatusChip label="Search active" tone="warning" />
        <StatusChip label="0 matches" tone="neutral" />
      </div>
    </div>
    <button type="button" className="secondary-action player-import-action" onClick={onClearSearch}>
      <X size={17} />
      <span>Clear Search</span>
    </button>
  </section>
);

interface PhoneAccessPanelProps {
  session: PhoneMediaSession | null;
  eligibleMediaCount: number;
  busyAction: PhoneAccessBusyAction;
  notice: string;
  desktopRuntimeAvailable: boolean;
  onStart: () => void;
  onStop: () => void;
  onCopyLink: (playlistUrl?: string) => void;
}

const PhoneAccessPanel = ({
  session,
  eligibleMediaCount,
  busyAction,
  notice,
  desktopRuntimeAvailable,
  onStart,
  onStop,
  onCopyLink,
}: PhoneAccessPanelProps) => {
  const [qrDataUrl, setQrDataUrl] = useState("");
  const endpoints = session?.endpoints?.length
    ? session.endpoints
    : session?.playlistUrl
      ? [
          {
            label: "Default link",
            host: "",
            kind: "other" as const,
            baseUrl: session.baseUrl ?? "",
            playlistUrl: session.playlistUrl,
            preferred: true,
          },
        ]
      : [];
  const [selectedEndpointUrl, setSelectedEndpointUrl] = useState("");
  const selectedEndpoint =
    endpoints.find((endpoint) => endpoint.playlistUrl === selectedEndpointUrl) ??
    endpoints.find((endpoint) => endpoint.preferred) ??
    endpoints[0];
  const playlistUrl = selectedEndpoint?.playlistUrl ?? "";
  const isActive = Boolean(session?.active && playlistUrl);
  const startDisabled =
    !desktopRuntimeAvailable || busyAction !== null || eligibleMediaCount === 0;
  const startDisabledReason = !desktopRuntimeAvailable
    ? "Open the desktop app to share media on your local network."
    : eligibleMediaCount === 0
      ? "Import or download a ready audio/video file before using phone sharing."
      : busyAction !== null
        ? "Phone sharing is already changing state."
        : "";

  useEffect(() => {
    const preferredEndpoint =
      endpoints.find((endpoint) => endpoint.preferred) ?? endpoints[0];
    setSelectedEndpointUrl(preferredEndpoint?.playlistUrl ?? "");
  }, [session?.id]);

  useEffect(() => {
    let isMounted = true;

    if (!playlistUrl) {
      setQrDataUrl("");
      return () => {
        isMounted = false;
      };
    }

    QRCode.toDataURL(playlistUrl, {
      errorCorrectionLevel: "M",
      margin: 1,
      width: 176,
      color: {
        dark: "#13251f",
        light: "#ffffff",
      },
    })
      .then((url) => {
        if (isMounted) {
          setQrDataUrl(url);
        }
      })
      .catch(() => {
        if (isMounted) {
          setQrDataUrl("");
        }
      });

    return () => {
      isMounted = false;
    };
  }, [playlistUrl]);

  return (
    <section className="phone-access-panel" aria-label="Watch on phone">
      <div className="phone-access-heading">
        <div>
          <div className="panel-title-line">
            <Smartphone size={18} />
            <h2>Watch on Phone</h2>
          </div>
          <p>Scan with your phone, open in VLC, and keep this app open.</p>
        </div>
        <StatusChip
          label={isActive ? "Sharing" : `${eligibleMediaCount} media`}
          tone={isActive ? "positive" : eligibleMediaCount > 0 ? "neutral" : "warning"}
        />
      </div>

      {isActive ? (
        <div className="phone-session">
          <div className="qr-frame">
            {qrDataUrl ? (
              <img src={qrDataUrl} alt="Phone access QR code" />
            ) : (
              <QrCode size={42} aria-hidden="true" />
            )}
          </div>
          <div className="phone-link-box">
            <span>Open in VLC</span>
            <code>{playlistUrl}</code>
          </div>
          {endpoints.length > 1 ? (
            <label className="field phone-endpoint-field">
              <span>Network address</span>
              <select
                value={playlistUrl}
                onChange={(event) => setSelectedEndpointUrl(event.target.value)}
              >
                {endpoints.map((endpoint) => (
                  <option key={endpoint.playlistUrl} value={endpoint.playlistUrl}>
                    {endpoint.label}
                  </option>
                ))}
              </select>
              <span className="field-hint">
                {selectedEndpoint?.warning ??
                  "Use the Wi-Fi/LAN address when VPN, Tor, or a privacy tunnel is active."}
              </span>
            </label>
          ) : null}
          <div className="phone-actions">
            <button
              type="button"
              className="secondary-action"
              onClick={() => onCopyLink(playlistUrl)}
              disabled={busyAction !== null}
            >
              <Copy size={15} />
              <span>Copy Link</span>
            </button>
            <button
              type="button"
              className="danger-action"
              onClick={onStop}
              disabled={busyAction !== null}
            >
              <Square size={15} />
              <span>{busyAction === "stop" ? "Stopping" : "Stop Sharing"}</span>
            </button>
          </div>
        </div>
      ) : (
        <div className="phone-session phone-session-idle">
          <div className="phone-steps">
            <span>1. Start phone access</span>
            <span>2. Scan the code</span>
            <span>3. Open in VLC</span>
          </div>
          <button
            type="button"
            className="primary-action"
            onClick={onStart}
            disabled={startDisabled}
            title={
              eligibleMediaCount === 0
                ? "Download or import audio/video before using phone access."
                : "Share ready audio and video on your Wi-Fi."
            }
          >
            <Smartphone size={17} />
            <span>{busyAction === "start" ? "Starting" : "Start Phone Access"}</span>
          </button>
          {startDisabledReason ? (
            <p className="action-reason">{startDisabledReason}</p>
          ) : null}
        </div>
      )}

      <div className="phone-access-footnote">
        <Wifi size={15} />
        <span>Use a Wi-Fi/LAN address for phone scans; VPN or Tor addresses may not be reachable.</span>
      </div>
      {notice ? <p className="phone-access-notice">{notice}</p> : null}
    </section>
  );
};

const sourcePriority: SourcePlatform[] = [
  "archive-org",
  "youtube",
  "x",
  "rumble",
  "odysee",
  "rss-feed",
  "teacher-relay",
  "telegram",
  "local-files",
];

const SourceReadinessPanel = ({
  sources,
  groups,
  selectedSourceId,
  runtimeDiagnostics,
  onSelectSource,
}: {
  sources: Source[];
  groups?: LibraryGroup[];
  selectedSourceId?: string;
  runtimeDiagnostics: RuntimeDiagnostics;
  onSelectSource?: (sourceId: string) => void;
}) => {
  const sourceByPlatform = new Map(sources.map((source) => [source.platform, source]));
  const sourceById = new Map(sources.map((source) => [source.id, source]));
  const groupBySourceId = new Map((groups ?? []).map((group) => [group.id, group]));
  const orderedSources = groups
    ? groups
        .map((group) => sourceById.get(group.id))
        .filter((source): source is Source => Boolean(source))
    : sourcePriority
        .map((platform) => sourceByPlatform.get(platform))
        .filter((source): source is Source => Boolean(source));

  return (
    <section className="side-panel source-readiness-panel">
      <SectionHeader title="Sources" meta={groups ? `${groups.length} active` : "v1 pulls"} />
      <div className="source-readiness-list">
        {orderedSources.length ? orderedSources.slice(0, 7).map((source) => {
          const readiness = sourceReadiness(source, runtimeDiagnostics);
          const group = groupBySourceId.get(source.id);

          const rowContent = (
            <>
              <div className="round-icon">
                <SourceIcon platform={source.platform} />
              </div>
              <div>
                <strong>{source.label}</strong>
                <span>
                  {group ? `${group.lessonCount} items. ` : ""}
                  {readiness.detail}
                </span>
              </div>
              <StatusChip label={readiness.label} tone={readiness.tone} />
            </>
          );

          return onSelectSource ? (
            <button
              type="button"
              className={
                selectedSourceId === source.id
                  ? "source-readiness-row source-readiness-row-button source-readiness-row-active"
                  : "source-readiness-row source-readiness-row-button"
              }
              key={source.id}
              onClick={() => onSelectSource(selectedSourceId === source.id ? "all" : source.id)}
            >
              {rowContent}
            </button>
          ) : (
            <div className="source-readiness-row source-readiness-row-static" key={source.id}>
              {rowContent}
            </div>
          );
        }) : (
          <p className="panel-empty">No source statuses match the current search.</p>
        )}
      </div>
    </section>
  );
};

const sourceReadiness = (
  source: Source,
  runtimeDiagnostics: RuntimeDiagnostics,
): { label: string; tone: "neutral" | "positive" | "warning" | "danger"; detail: string } => {
  switch (source.platform) {
    case "local-files":
      return {
        label: isTauriRuntime() ? "Ready" : "Desktop only",
        tone: isTauriRuntime() ? "positive" : "warning",
        detail: "Imports selected video, audio, and PDF files into the local library.",
      };
    case "archive-org":
      return {
        label: "Ready",
        tone: "positive",
        detail: "Uses Archive item metadata and direct file listings.",
      };
    case "rss-feed":
      return {
        label: "Ready",
        tone: "positive",
        detail: "Reads RSS, Atom, JSON Feed, media enclosures, and posts.",
      };
    case "teacher-relay":
      return {
        label: "Ready",
        tone: "positive",
        detail: "Validates signed manifests and hash-backed media refs.",
      };
    case "youtube":
      return {
        label: runtimeDiagnostics.desktopRuntimeAvailable
          ? runtimeDiagnostics.requiredMediaToolsAvailable
            ? "User pull"
            : "Needs tool"
          : "Desktop check",
        tone: runtimeDiagnostics.desktopRuntimeAvailable && runtimeDiagnostics.requiredMediaToolsAvailable
          ? "positive"
          : runtimeDiagnostics.desktopRuntimeAvailable
            ? "warning"
            : "neutral",
        detail: runtimeDiagnostics.desktopRuntimeAvailable
          ? "Metadata via feeds/API; permitted downloads use verified local media tools."
          : "Desktop app checks local media tools before downloads.",
      };
    case "rumble":
      return {
        label: runtimeDiagnostics.desktopRuntimeAvailable
          ? runtimeDiagnostics.requiredMediaToolsAvailable
            ? runtimeDiagnostics.ytDlpCookiesConfigured
              ? "Cookie fallback"
              : "Best effort"
            : "Needs tool"
          : "Desktop check",
        tone: runtimeDiagnostics.desktopRuntimeAvailable ? "warning" : "neutral",
        detail: runtimeDiagnostics.desktopRuntimeAvailable
          ? runtimeDiagnostics.ytDlpCookiesConfigured
            ? "Direct URLs can use local yt-dlp plus app-local cookies when required."
            : "No broad API assumed; 403s usually need app-local cookies or manual import."
          : "Desktop app checks local tooling for direct Rumble URLs.",
      };
    case "odysee":
      return {
        label: "Limited",
        tone: "warning",
        detail: "Tracks Odysee/LBRY references; native daemon support is future work.",
      };
    case "x":
      return {
        label: runtimeDiagnostics.ytDlpCookiesConfigured ? "Cookies ready" : "Needs cookies",
        tone: "warning",
        detail: runtimeDiagnostics.ytDlpCookiesConfigured
          ? "Media pulls can try local yt-dlp cookies; API access is still cleaner."
          : "X media normally requires API access, app-local cookies, or manual import.",
      };
    case "telegram":
      return {
        label: "Public only",
        tone: "warning",
        detail: "Public t.me previews work; private channels need a later local session adapter.",
      };
  }
};

const StorageHygienePanel = ({
  audit,
  busyAction,
  onAudit,
  onCleanup,
}: {
  audit: MediaStorageAudit | null;
  busyAction: MediaStorageBusyAction;
  onAudit: () => void;
  onCleanup: () => void;
}) => {
  const staleFiles = audit?.staleFiles ?? 0;
  const canCleanup = staleFiles > 0 && busyAction === null;

  return (
    <section className="storage-hygiene-panel">
      <div className="storage-hygiene-heading">
        <div className="matrix-source">
          <Database size={18} aria-hidden="true" />
          <div>
            <strong>Storage Audit</strong>
            <span>Find app-library files no current DB row references.</span>
          </div>
        </div>
        <div className="managed-source-actions">
          <button
            type="button"
            className="secondary-action"
            onClick={onAudit}
            disabled={busyAction !== null}
          >
            <Search size={15} />
            <span>{busyAction === "audit" ? "Scanning" : "Scan Storage"}</span>
          </button>
          <button
            type="button"
            className="danger-action"
            onClick={onCleanup}
            disabled={!canCleanup}
            title={
              staleFiles > 0
                ? "Remove unreferenced files from the managed app library."
                : "Scan storage before cleanup."
            }
          >
            <Trash2 size={15} />
            <span>{busyAction === "cleanup" ? "Cleaning" : "Clean Stale Files"}</span>
          </button>
        </div>
      </div>
      <div className="storage-hygiene-stats">
        <StatusChip label={`${audit?.scannedFiles ?? 0} scanned`} tone="neutral" />
        <StatusChip label={`${audit?.referencedFiles ?? 0} referenced`} tone="positive" />
        <StatusChip
          label={`${staleFiles} stale`}
          tone={staleFiles > 0 ? "warning" : "positive"}
        />
        <StatusChip label={formatBytes(audit?.staleBytes ?? 0)} tone="neutral" />
        <StatusChip label={`${audit?.partialFiles ?? 0} fragments`} tone="neutral" />
      </div>
      {audit?.staleSamples.length ? (
        <div className="storage-sample-list" aria-label="Stale file examples">
          {audit.staleSamples.map((sample) => (
            <code key={sample}>{sample}</code>
          ))}
        </div>
      ) : (
        <p className="panel-empty">
          {audit ? audit.messages.join(" ") : "Run a scan before cleaning anything."}
        </p>
      )}
    </section>
  );
};

const LibraryOrganizationPanel = ({
  channels,
  lessons,
  contentTypeGroups,
  availabilityGroups,
  selectedChannelId,
  selectedContentType,
  selectedAvailability,
  onSelectChannel,
  onSelectContentType,
  onSelectAvailability,
}: {
  channels: ChannelSubscriptionView[];
  lessons: Lesson[];
  mediaByLessonId: Map<string, MediaFile>;
  contentTypeGroups: LibraryGroup[];
  availabilityGroups: LibraryGroup[];
  selectedChannelId: string;
  selectedContentType: LibraryContentTypeFilter;
  selectedAvailability: LibraryAvailabilityFilter;
  onSelectChannel: (channelId: string) => void;
  onSelectContentType: (contentType: LibraryContentTypeFilter) => void;
  onSelectAvailability: (availability: LibraryAvailabilityFilter) => void;
}) => {
  return (
    <section className="side-panel library-organization-panel">
      <SectionHeader
        title="Library Organization"
        meta={`${channels.length} channels · ${lessons.length} items`}
      />
      <div className="compact-list">
        {channels.map((channel) => (
          <button
            type="button"
            className={
              selectedChannelId === channel.id ||
              selectedChannelId === channel.relayId ||
              selectedChannelId === channel.sourceId
                ? "compact-row compact-row-active"
                : "compact-row"
            }
            key={channel.id}
            onClick={() => onSelectChannel(selectedChannelId === channel.id ? "all" : channel.id)}
            aria-pressed={
              selectedChannelId === channel.id ||
              selectedChannelId === channel.relayId ||
              selectedChannelId === channel.sourceId
            }
          >
            <div className="round-icon">
              <Rss size={16} />
            </div>
            <div>
              <strong>{channel.title}</strong>
              <span>
                {channel.itemCount} items · {channel.localFileCount} local ·{" "}
                {channel.missingFileCount} need files
              </span>
            </div>
          </button>
        ))}
        {channels.length === 0 ? <p className="panel-empty">No followed channels yet.</p> : null}
      </div>
      <div className="organization-pill-grid" aria-label="Library content types">
        {contentTypeGroups.map((group) => {
          const Icon = contentTypeIcon(group.id as ContentType);
          return (
            <button
              type="button"
              key={group.id}
              className={
                selectedContentType === group.id ? "scope-chip scope-chip-active" : "scope-chip"
              }
              onClick={() =>
                onSelectContentType(
                  selectedContentType === group.id
                    ? "all"
                    : (group.id as LibraryContentTypeFilter),
                )
              }
              aria-pressed={selectedContentType === group.id}
            >
              <Icon size={14} />
              <span>{group.label}</span>
              <strong>{group.lessonCount}</strong>
            </button>
          );
        })}
        {contentTypeGroups.length === 0 ? (
          <p className="panel-empty">No content type groups yet.</p>
        ) : null}
      </div>
      <div className="organization-pill-grid" aria-label="Library availability">
        {availabilityGroups.map((group) => (
          <button
            type="button"
            key={group.id}
            className={
              selectedAvailability === group.id ? "scope-chip scope-chip-active" : "scope-chip"
            }
            onClick={() =>
              onSelectAvailability(
                selectedAvailability === group.id
                  ? "all"
                  : (group.id as LibraryAvailabilityFilter),
              )
            }
            aria-pressed={selectedAvailability === group.id}
          >
            <HardDrive size={14} />
            <span>{group.label}</span>
            <strong>{group.lessonCount}</strong>
          </button>
        ))}
        {availabilityGroups.length === 0 ? (
          <p className="panel-empty">No availability groups yet.</p>
        ) : null}
      </div>
    </section>
  );
};

const TeacherPanel = ({
  groups,
  selectedTeacherId,
  onSelectTeacher,
}: {
  groups: LibraryGroup[];
  selectedTeacherId: string;
  onSelectTeacher: (teacherId: string) => void;
}) => (
  <section className="side-panel">
    <SectionHeader title="Teachers" meta={`${groups.length} saved`} />
    <div className="compact-list">
      {groups.length ? (
        groups.map((group) => (
        <button
          type="button"
          className={
            selectedTeacherId === group.id
              ? "compact-row compact-row-active"
              : "compact-row"
          }
          key={group.id}
          onClick={() => onSelectTeacher(selectedTeacherId === group.id ? "all" : group.id)}
        >
          <div className="round-icon">
            <UserRound size={16} />
          </div>
          <div>
            <strong>{group.label}</strong>
            <span>{group.lessonCount} items · {group.activeCount} active</span>
          </div>
        </button>
      ))
      ) : (
        <p className="panel-empty">No teachers yet.</p>
      )}
    </div>
  </section>
);

const ChannelSubscriptionCard = ({
  channel,
  selected = false,
  busySourceAction,
  onSelect,
  onRefresh,
  onDownload,
  onUnfollow,
}: {
  channel: ChannelSubscriptionView;
  selected?: boolean;
  busySourceAction: BusySourceAction;
  onSelect?: (channelId: string) => void;
  onRefresh: (channel: ChannelSubscriptionView) => void;
  onDownload: (channel: ChannelSubscriptionView) => void;
  onUnfollow: (channel: ChannelSubscriptionView) => void;
}) => {
  const isDownloading =
    busySourceAction?.action === "download" && busySourceAction.sourceId === channel.sourceId;
  const isRemoving =
    busySourceAction?.action === "clear" && busySourceAction.sourceId === channel.sourceId;
  const mediaTone =
    channel.missingFileCount > 0 ? "warning" : channel.localFileCount > 0 ? "positive" : "neutral";

  return (
    <article className={selected ? "channel-card channel-card-active" : "channel-card"}>
      <div className="channel-card-main">
        <div className="relay-card-header">
          <div className="round-icon">
            <Rss size={16} />
          </div>
          <div className="channel-title-block">
            <h3>{channel.title}</h3>
            <span>{channel.curatorLabel}</span>
          </div>
        </div>
        <p>{channel.description ?? "Signed channel subscription."}</p>
        <div className="channel-card-meta">
          <StatusChip label={trustLabel(channel.trustState)} tone={trustTone(channel.trustState)} />
          <StatusChip
            label={channel.trusted ? "Trusted curator" : channel.trustPolicy.replace(/-/g, " ")}
            tone={channel.trusted ? "positive" : "neutral"}
          />
          <StatusChip
            label={`${channel.localFileCount}/${channel.itemCount} local`}
            tone={mediaTone}
          />
          <StatusChip
            label={channel.autoDownload ? "Auto-download" : "Review first"}
            tone={channel.autoDownload ? "positive" : "warning"}
          />
        </div>
        <div className="channel-card-stats">
          <span>{channel.itemCount} items</span>
          <span>{channel.missingFileCount} need files</span>
          <span>{channel.postCount} notes</span>
          <span>Updated {formatDate(channel.latestUpdateAt)}</span>
        </div>
        <code>{channel.feedUrl}</code>
      </div>
      <div className="channel-card-actions">
        {onSelect ? (
          <button
            type="button"
            className="secondary-action"
            onClick={() => onSelect(selected ? "all" : channel.id)}
            aria-pressed={selected}
          >
            <Search size={15} />
            <span>{selected ? "Filtering" : "Filter"}</span>
          </button>
        ) : null}
        <button
          type="button"
          className="secondary-action"
          onClick={() => onRefresh(channel)}
          disabled={!channel.sourceId}
          title={channel.sourceId ? "Refresh this channel." : "Re-follow this channel to manage it."}
        >
          <RefreshCcw size={15} />
          <span>Refresh</span>
        </button>
        <button
          type="button"
          className="secondary-action"
          onClick={() => onDownload(channel)}
          disabled={!channel.sourceId || isDownloading}
          title={
            channel.sourceId
              ? "Download missing file-backed lessons for this channel."
              : "Re-follow this channel to manage downloads."
          }
        >
          <Download size={15} />
          <span>{isDownloading ? "Downloading" : "Download"}</span>
        </button>
        <button
          type="button"
          className="danger-action"
          onClick={() => onUnfollow(channel)}
          disabled={!channel.sourceId || isRemoving}
          title={channel.sourceId ? "Remove this channel source and its rows." : "No source row is linked."}
        >
          <X size={15} />
          <span>{isRemoving ? "Removing" : "Unfollow"}</span>
        </button>
      </div>
    </article>
  );
};

const LiveSessionPanel = ({
  liveSessions,
  teachers,
}: {
  liveSessions: LiveSession[];
  teachers: Map<string, Teacher>;
}) => (
  <section className="side-panel">
    <SectionHeader title="Live Lessons" meta={`${liveSessions.length} tracked`} />
    <div className="compact-list">
      {liveSessions.length ? (
        liveSessions.slice(0, 3).map((session) => (
          <div className="compact-row" key={session.id}>
            <div className={`round-icon ${liveSessionToneClass(session.status)}`}>
              <RadioTower size={16} />
            </div>
            <div>
              <strong>{session.title}</strong>
              <span>
                {teachers.get(session.teacherId)?.displayName ?? "Unknown teacher"} ·{" "}
                {session.provider.replace("-", " ")} · {session.status.replace("-", " ")}
              </span>
            </div>
          </div>
        ))
      ) : (
        <p className="panel-empty">No live lessons tracked.</p>
      )}
    </div>
  </section>
);

const RelaysView = ({
  channelSubscriptions,
  liveSessions,
  teachers,
  query,
  busySourceAction,
  onOpenImport,
  onRefreshChannel,
  onDownloadChannel,
  onUnfollowChannel,
}: {
  channelSubscriptions: ChannelSubscriptionView[];
  liveSessions: LiveSession[];
  teachers: Map<string, Teacher>;
  query: string;
  busySourceAction: BusySourceAction;
  onOpenImport: (mode?: ImportMode) => void;
  onRefreshChannel: (channel: ChannelSubscriptionView) => void;
  onDownloadChannel: (channel: ChannelSubscriptionView) => void;
  onUnfollowChannel: (channel: ChannelSubscriptionView) => void;
}) => {
  const relayQuery = normalizeSearch(query);
  const visibleChannels = channelSubscriptions.filter((channel) =>
    includesQuery(relayQuery, [
      channel.title,
      channel.feedUrl,
      channel.feedFormat,
      channel.trustState,
      channel.visibility,
      channel.trustPolicy,
      channel.description,
      channel.curatorLabel,
    ]),
  );
  const visibleLiveSessions = liveSessions.filter((session) => {
    const teacher = teachers.get(session.teacherId);
    return includesQuery(relayQuery, [
      session.title,
      session.provider,
      session.providerUrl,
      session.status,
      session.recordingPolicy,
      teacher?.displayName,
    ]);
  });

  return (
    <div className="wide-page relays-page">
      <div className="page-heading relays-heading">
        <div>
          <h2>Channels</h2>
          <p>
            Follow signed teacher and curator channels, review trust, and download approved lessons
            into the local library.
          </p>
        </div>
        <div className="relays-heading-status">
          <StatusChip label="Signed manifests" tone="positive" />
          <StatusChip label="No central catalog" tone="neutral" />
        </div>
      </div>

      <div className="relay-layout">
        <section className="relay-main">
          <section className="feed-follow-panel" aria-label="Followed channels">
            <div className="feed-follow-copy">
              <SectionHeader
                title="Following"
                meta={`${visibleChannels.length} channel${visibleChannels.length === 1 ? "" : "s"}`}
              />
              <p>
                Follow shared channel links here. Review trust and file availability before
                refreshing or downloading lessons.
              </p>
            </div>
            <button type="button" className="secondary-action" onClick={() => onOpenImport("feed")}>
              <Rss size={16} />
              <span>Follow Channel</span>
            </button>
            <div className="channel-card-list">
              {visibleChannels.length ? (
                visibleChannels.map((channel) => (
                  <ChannelSubscriptionCard
                    key={channel.id}
                    channel={channel}
                    busySourceAction={busySourceAction}
                    onRefresh={onRefreshChannel}
                    onDownload={onDownloadChannel}
                    onUnfollow={onUnfollowChannel}
                  />
                ))
              ) : (
                <EmptyState
                  icon={Rss}
                  title={relayQuery ? "No matching channels" : "No followed channels"}
                  detail={
                    relayQuery
                      ? "Clear search or try a curator, channel URL, media state, or trust state."
                      : "Follow a signed teacher channel or curator manifest when one is available."
                  }
                />
              )}
            </div>
          </section>
        </section>

        <aside className="relay-aside">
          <section className="side-panel relay-publish-panel">
            <SectionHeader title="Protocol Model" meta="signed channels" />
            <div className="publish-steps">
              <div>
                <UploadCloud size={17} />
                <span>Teacher or curator publishes class media and source notes.</span>
              </div>
              <div>
                <ShieldCheck size={17} />
                <span>Manifest signs lesson metadata, hashes, and provenance.</span>
              </div>
              <div>
                <Download size={17} />
                <span>Learners fetch or auto-download approved enclosures.</span>
              </div>
            </div>
          </section>
          <LiveProviderMatrix />
        </aside>
      </div>

      <SectionHeader title="Live Lesson Capture" meta={`${visibleLiveSessions.length} sessions`} />
      <div className="live-session-list">
        {visibleLiveSessions.length ? (
          visibleLiveSessions.map((session) => (
            <LiveSessionRow
              key={session.id}
              session={session}
              teacher={teachers.get(session.teacherId)}
            />
          ))
        ) : (
          <EmptyState
            icon={RadioTower}
            title={relayQuery ? "No matching live lessons" : "No live lessons tracked"}
            detail={
              relayQuery
                ? "Clear search or try a provider, title, teacher, or status."
                : "Live capture stays manual until a teacher channel or provider setup is configured."
            }
          />
        )}
      </div>
    </div>
  );
};

const PublishView = ({
  publisherProfiles,
  onPublisherProfilesChanged,
  onPublisherResult,
}: {
  publisherProfiles: PublisherProfile[];
  onPublisherProfilesChanged: () => Promise<void>;
  onPublisherResult: (notice: string) => void;
}) => (
  <div className="wide-page publish-page">
    <div className="page-heading publish-heading">
      <div>
        <h2>Publish</h2>
        <p>
          Create a teacher-owned signed channel, publish verified lesson media, and share one link
          with learners.
        </p>
      </div>
      <div className="relays-heading-status">
        <StatusChip label="Local signing key" tone="positive" />
        <StatusChip label="Share link" tone="neutral" />
        <StatusChip label="No central catalog" tone="neutral" />
      </div>
    </div>
    <TeacherPublisherPanel
      profiles={publisherProfiles}
      onProfilesChanged={onPublisherProfilesChanged}
      onResult={onPublisherResult}
    />
  </div>
);

const TeacherPublisherPanel = ({
  profiles,
  onProfilesChanged,
  onResult,
}: {
  profiles: PublisherProfile[];
  onProfilesChanged: () => Promise<void>;
  onResult: (notice: string) => void;
}) => {
  const [profileId, setProfileId] = useState(profiles[0]?.id ?? "");
  const [displayName, setDisplayName] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [relayText, setRelayText] = useState(starterRelayText);
  const [blossomText, setBlossomText] = useState(starterBlossomText);
  const [archiveText, setArchiveText] = useState("");
  const [ipfsApiUrl, setIpfsApiUrl] = useState("");
  const [ipfsGatewayUrl, setIpfsGatewayUrl] = useState("");
  const [channelTitle, setChannelTitle] = useState("");
  const [channelDescription, setChannelDescription] = useState("");
  const [lessonDrafts, setLessonDrafts] = useState<PublishedLessonDraft[]>([]);
  const [publishResult, setPublishResult] = useState<ChannelPublishResult | null>(null);
  const [shareQrDataUrl, setShareQrDataUrl] = useState("");
  const [panelNotice, setPanelNotice] = useState("");
  const [endpointTestReport, setEndpointTestReport] =
    useState<PublisherEndpointTestReport | null>(null);
  const [testedEndpointSignature, setTestedEndpointSignature] = useState("");
  const [isWorking, setIsWorking] = useState(false);
  const selectedProfile = profiles.find((profile) => profile.id === profileId);

  useEffect(() => {
    if (!profileId && profiles[0]) {
      setProfileId(profiles[0].id);
    }
  }, [profileId, profiles]);

  useEffect(() => {
    if (!selectedProfile) {
      return;
    }

    setDisplayName(selectedProfile.displayName);
    setRelayText(selectedProfile.relays.map((relay) => relay.url).join("\n"));
    setBlossomText(selectedProfile.blossomServers.map((server) => server.url).join("\n"));
  }, [selectedProfile]);

  const relays = endpointLines(relayText).map((url) => ({ url }));
  const blossomServers = endpointLines(blossomText).map((url) => ({ url }));
  const endpointSignature = [
    relays.map((relay) => relay.url).join("|"),
    blossomServers.map((server) => server.url).join("|"),
  ].join("::");
  const endpointHealthHasFailures = endpointTestReport
    ? endpointTestHasFailures(endpointTestReport)
    : false;
  const endpointHealthPassed =
    Boolean(endpointTestReport?.passed) && testedEndpointSignature === endpointSignature;
  const archiveMirrors: ArchiveMirrorConfig[] = [
    ...endpointLines(archiveText).map((url) => ({
      service: "https",
      url,
      label: "Public archive mirror",
    })),
    ...(ipfsApiUrl.trim()
      ? [
          {
            service: "ipfs-http-api",
            url: ipfsApiUrl.trim(),
            gatewayUrl: ipfsGatewayUrl.trim() || undefined,
            label: "Local IPFS manifest pin",
          },
        ]
      : []),
  ];
  const publishReadiness = [
    {
      label: selectedProfile ? "Profile" : "Profile missing",
      tone: selectedProfile ? "positive" : "warning",
    },
    {
      label: selectedProfile?.vaultConfigured ? "Vault saved" : "Vault missing",
      tone: selectedProfile?.vaultConfigured ? "positive" : "warning",
    },
    {
      label: passphrase.length >= 8 ? "Passphrase" : "Passphrase needed",
      tone: passphrase.length >= 8 ? "positive" : "warning",
    },
    {
      label: relays.length
        ? `${relays.length} Nostr endpoint${relays.length === 1 ? "" : "s"}`
        : "Nostr endpoint needed",
      tone: relays.length ? "positive" : "warning",
    },
    {
      label: blossomServers.length
        ? `${blossomServers.length} Blossom server${blossomServers.length === 1 ? "" : "s"}`
        : "Storage needed",
      tone: blossomServers.length ? "positive" : "warning",
    },
    {
      label: endpointHealthPassed
        ? endpointHealthHasFailures
          ? "Network partial pass"
          : "Network tested"
        : "Health check needed",
      tone: endpointHealthPassed ? "positive" : "warning",
    },
    {
      label: lessonDrafts.length
        ? `${lessonDrafts.length} file${lessonDrafts.length === 1 ? "" : "s"}`
        : "Media needed",
      tone: lessonDrafts.length ? "positive" : "warning",
    },
    {
      label: channelTitle.trim() ? "Channel title" : "Title needed",
      tone: channelTitle.trim() ? "positive" : "warning",
    },
    {
      label: archiveMirrors.length
        ? `${archiveMirrors.length} archive mirror${archiveMirrors.length === 1 ? "" : "s"}`
        : "Archive optional",
      tone: archiveMirrors.length ? "positive" : "neutral",
    },
  ] satisfies Array<{ label: string; tone: "neutral" | "positive" | "warning" }>;
  const publishBlockedReason = !selectedProfile
    ? "Create or select a publisher profile."
    : passphrase.length < 8
      ? "Enter the vault passphrase."
      : relays.length === 0
        ? "Add at least one Nostr relay."
        : blossomServers.length === 0
          ? "Add at least one Blossom server."
          : !endpointHealthPassed
            ? "Run a passing endpoint test for the current relay and storage settings."
            : ipfsApiUrl.trim() && !ipfsGatewayUrl.trim()
              ? "Add the IPFS gateway URL for local IPFS archival."
              : !channelTitle.trim()
                ? "Add a channel title."
                : lessonDrafts.length === 0
                  ? "Select media to publish."
                  : "";
  const endpointTestBlockedReason = !selectedProfile
    ? "Create or select a publisher profile."
    : passphrase.length < 8
      ? "Enter the vault passphrase."
      : relays.length === 0
        ? "Add at least one Nostr relay."
        : blossomServers.length === 0
          ? "Add at least one Blossom server."
          : "";
  const publisherSteps = [
    {
      title: "Profile",
      detail: selectedProfile
        ? `Using ${selectedProfile.displayName}.`
        : "Create or select the local signing profile.",
      complete: Boolean(selectedProfile),
    },
    {
      title: "Channel Details",
      detail: channelTitle.trim()
        ? "Learner-facing channel name is ready."
        : "Name the channel learners will follow.",
      complete: Boolean(channelTitle.trim()),
    },
    {
      title: "Storage",
      detail: endpointHealthPassed
        ? endpointHealthHasFailures
          ? "Quorum passed; fix or remove endpoints that failed."
          : "Every configured relay and Blossom server accepted the probe."
        : "Test one working Nostr relay and one Blossom server.",
      complete: endpointHealthPassed,
    },
    {
      title: "Media",
      detail: lessonDrafts.length
        ? `${lessonDrafts.length} publishable file(s) selected.`
        : "Select video, audio, or PDF files.",
      complete: lessonDrafts.length > 0,
    },
    {
      title: "Mirrors",
      detail: archiveMirrors.length
        ? "Archive mirrors will be announced only after hash match."
        : "Optional archive mirrors can stay empty.",
      complete: archiveMirrors.length > 0,
      optional: true,
    },
    {
      title: "Publish",
      detail: passphrase.length >= 8 ? "Passphrase ready for signing." : "Enter the local vault passphrase.",
      complete: passphrase.length >= 8,
    },
  ] satisfies Array<{ title: string; detail: string; complete: boolean; optional?: boolean }>;
  const requiredPublisherSteps = publisherSteps.filter((step) => !step.optional);
  const completedRequiredPublisherSteps = requiredPublisherSteps.filter((step) => step.complete);
  const nextRequiredPublisherStep = requiredPublisherSteps.find((step) => !step.complete);
  const publisherProgressPercent = Math.round(
    (completedRequiredPublisherSteps.length / requiredPublisherSteps.length) * 100,
  );
  const publishInvite = useMemo(
    () => (publishResult ? buildChannelInvite(publishResult) : null),
    [publishResult],
  );

  useEffect(() => {
    let isMounted = true;

    if (!publishInvite) {
      setShareQrDataUrl("");
      return () => {
        isMounted = false;
      };
    }

    QRCode.toDataURL(publishInvite.canonicalChannelLink, {
      errorCorrectionLevel: "M",
      margin: 1,
      width: 148,
      color: {
        dark: "#13251f",
        light: "#ffffff",
      },
    })
      .then((url) => {
        if (isMounted) {
          setShareQrDataUrl(url);
        }
      })
      .catch(() => {
        if (isMounted) {
          setShareQrDataUrl("");
        }
      });

    return () => {
      isMounted = false;
    };
  }, [publishInvite]);

  const createProfile = async () => {
    setIsWorking(true);
    setPanelNotice("");

    try {
      const profile = await createPublisherProfile({
        displayName,
        passphrase,
        relays,
        blossomServers,
      });
      setProfileId(profile.id);
      setPanelNotice(`Publisher profile ready for ${profile.displayName}.`);
      await onProfilesChanged();
    } catch (error: unknown) {
      setPanelNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setIsWorking(false);
    }
  };

  const unlockProfile = async () => {
    if (!selectedProfile) {
      setPanelNotice("Create or select a publisher profile first.");
      return;
    }

    setIsWorking(true);
    setPanelNotice("");

    try {
      const profile = await unlockPublisherProfile(selectedProfile.id, passphrase);
      setPanelNotice(`Vault unlocked for ${profile.displayName}.`);
    } catch (error: unknown) {
      setPanelNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setIsWorking(false);
    }
  };

  const testEndpoints = async () => {
    if (!selectedProfile) {
      setPanelNotice("Create or select a publisher profile first.");
      return;
    }

    setIsWorking(true);
    setPanelNotice("");
    setEndpointTestReport(null);

    try {
      const report = await testPublisherEndpoints({
        profileId: selectedProfile.id,
        passphrase,
        relays,
        blossomServers,
      });
      setEndpointTestReport(report);
      setTestedEndpointSignature(endpointSignature);
      setPanelNotice(report.messages.join(" "));
    } catch (error: unknown) {
      setPanelNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setIsWorking(false);
    }
  };

  const selectMedia = async () => {
    setIsWorking(true);
    setPanelNotice("");

    try {
      const paths = await chooseLocalMediaPaths();
      if (!paths.length) {
        setPanelNotice("No publishable media selected.");
        return;
      }
      setLessonDrafts(
        paths.map((path) => ({
          path,
          title: titleFromPath(path),
          contentType: contentTypeFromPublishPath(path),
        })),
      );
    } catch (error: unknown) {
      setPanelNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setIsWorking(false);
    }
  };

  const updateDraft = (
    index: number,
    patch: Partial<PublishedLessonDraft>,
  ) => {
    setLessonDrafts((current) =>
      current.map((draft, draftIndex) =>
        draftIndex === index ? { ...draft, ...patch } : draft,
      ),
    );
  };

  const publishChannel = async () => {
    if (!selectedProfile) {
      setPanelNotice("Create or select a publisher profile first.");
      return;
    }
    if (!endpointHealthPassed) {
      setPanelNotice("Run a passing endpoint test for the current relay and storage settings first.");
      return;
    }

    setIsWorking(true);
    setPanelNotice("");
    setPublishResult(null);

    try {
      const result = await publishTeacherChannel({
        profileId: selectedProfile.id,
        passphrase,
        channelTitle,
        channelDescription,
        relays,
        blossomServers,
        archiveMirrors,
        lessons: lessonDrafts,
      });
      setPublishResult(result);
      setPanelNotice(result.messages.join(" "));
      onResult(`Published ${result.channelId}; ${result.manifestSha256}.`);
      await onProfilesChanged();
    } catch (error: unknown) {
      setPanelNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setIsWorking(false);
    }
  };

  const copyChannelLink = async () => {
    if (!publishInvite) {
      return;
    }

    try {
      await navigator.clipboard.writeText(publishInvite.canonicalChannelLink);
      setPanelNotice("Channel link copied.");
    } catch {
      setPanelNotice("Copy failed. Select and copy the channel link manually.");
    }
  };

  const copyInviteText = async () => {
    if (!publishInvite) {
      return;
    }

    try {
      await navigator.clipboard.writeText(publishInvite.inviteText);
      setPanelNotice("Channel invite copied.");
    } catch {
      setPanelNotice("Copy failed. Select and copy the invite text manually.");
    }
  };

  const shareInvite = async () => {
    if (!publishInvite) {
      return;
    }

    const sharePayload = {
      title: "Duroos channel invite",
      text: publishInvite.inviteText,
      url: publishInvite.canonicalChannelLink,
    };

    try {
      if ("share" in navigator && typeof navigator.share === "function") {
        await navigator.share(sharePayload);
        setPanelNotice("Channel invite shared.");
        return;
      }

      await navigator.clipboard.writeText(publishInvite.inviteText);
      setPanelNotice("Channel invite copied.");
    } catch (error: unknown) {
      if (error instanceof DOMException && error.name === "AbortError") {
        return;
      }
      setPanelNotice("Share failed. Select and copy the invite text manually.");
    }
  };

  const useStarterPresets = () => {
    setRelayText(starterRelayText);
    setBlossomText(starterBlossomText);
    setEndpointTestReport(null);
    setTestedEndpointSignature("");
    setPanelNotice("Starter presets loaded. Run Test Endpoints before publishing.");
  };

  return (
    <section className="publisher-panel">
      <div className="publisher-panel-header">
        <div className="publisher-title-copy">
          <span className="publisher-kicker">Teacher publisher</span>
          <h2>Publish a signed channel</h2>
          <p>
            Build a local profile, test network storage, then share one signed channel link.
          </p>
        </div>
        <div className="publisher-trust-stack" aria-label="Publisher model">
          <StatusChip label={`${profiles.length} profiles`} tone="neutral" />
          <StatusChip label="No central catalog" tone="positive" />
        </div>
      </div>
      <div className="publisher-setup-strip" aria-label="Publisher setup progress">
        <div className="publisher-setup-count">
          <span>Required</span>
          <strong>
            {completedRequiredPublisherSteps.length}/{requiredPublisherSteps.length}
          </strong>
        </div>
        <div
          className="publisher-setup-meter"
          aria-label="Required publisher setup"
          aria-valuemax={100}
          aria-valuemin={0}
          aria-valuenow={publisherProgressPercent}
          role="progressbar"
        >
          <span style={{ width: `${publisherProgressPercent}%` }} />
        </div>
        <div className="publisher-setup-next">
          <strong>{nextRequiredPublisherStep?.title ?? "Publish"}</strong>
          <p>{nextRequiredPublisherStep?.detail ?? "Review and publish the signed channel."}</p>
        </div>
      </div>
      {panelNotice ? (
        <p className="publisher-notice" role="status">
          {panelNotice}
        </p>
      ) : null}
      <PublisherSetupChecklist steps={publisherSteps} />

      <div className="publisher-grid">
        <div className="publisher-column">
          <SectionHeader title="1. Profile" meta="local key" />
          <label className="field">
            <span>Publisher profile</span>
            <select
              value={profileId}
              onChange={(event) => setProfileId(event.target.value)}
            >
              <option value="">New profile</option>
              {profiles.map((profile) => (
                <option key={profile.id} value={profile.id}>
                  {profile.displayName}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>Teacher display name</span>
            <input
              value={displayName}
              onChange={(event) => setDisplayName(event.target.value)}
            />
            <span className="field-hint">Scholar or institute name.</span>
          </label>
          <label className="field">
            <span>Vault passphrase</span>
            <input
              type="password"
              value={passphrase}
              onChange={(event) => setPassphrase(event.target.value)}
            />
            <span className="field-hint">Required for signing.</span>
          </label>
          <div className="publisher-actions">
            <button
              type="button"
              className="secondary-action"
              onClick={createProfile}
              disabled={isWorking}
            >
              <KeyRound size={16} />
              <span>Create Profile</span>
            </button>
            <button
              type="button"
              className="secondary-action"
              onClick={unlockProfile}
              disabled={isWorking || !selectedProfile}
            >
              <Lock size={16} />
              <span>Unlock</span>
            </button>
          </div>
        </div>

        <div className="publisher-column">
          <SectionHeader title="2. Storage" meta="test first" />
          <div className="preset-panel">
            <div>
              <strong>Starter presets</strong>
              <span>Third-party, editable, and not operated by Duroos.</span>
            </div>
            <button type="button" className="secondary-action" onClick={useStarterPresets}>
              <RefreshCcw size={15} />
              <span>Use Presets</span>
            </button>
          </div>
          <div className="preset-grid" aria-label="Starter network presets">
            {starterNostrRelayPresets.map((preset) => (
              <div className="preset-row" key={preset.url}>
                <StatusChip label="Nostr" tone="neutral" />
                <strong>{preset.name}</strong>
                <code>{preset.url}</code>
              </div>
            ))}
            {starterBlossomServerPresets.map((preset) => (
              <div className="preset-row" key={preset.url}>
                <StatusChip label="Blossom" tone="neutral" />
                <strong>{preset.name}</strong>
                <code>{preset.url}</code>
              </div>
            ))}
          </div>
          <details className="advanced-network-settings">
            <summary>Customize network settings</summary>
            <label className="field">
              <span>Nostr relays</span>
              <textarea
                value={relayText}
                onChange={(event) => {
                  setRelayText(event.target.value);
                  setEndpointTestReport(null);
                  setTestedEndpointSignature("");
                }}
              />
              <span className="field-hint">One relay per line, for example wss://relay.example.</span>
            </label>
            <label className="field">
              <span>Blossom servers</span>
              <textarea
                value={blossomText}
                onChange={(event) => {
                  setBlossomText(event.target.value);
                  setEndpointTestReport(null);
                  setTestedEndpointSignature("");
                }}
              />
              <span className="field-hint">One storage server per line, for example https://blossom.example.</span>
            </label>
            <label className="field">
              <span>Archive manifest mirrors</span>
              <textarea
                value={archiveText}
                onChange={(event) => setArchiveText(event.target.value)}
              />
              <span className="field-hint">Optional public mirror URLs, one per line.</span>
            </label>
            <div className="archive-ipfs-grid">
              <label className="field">
                <span>Local IPFS API</span>
                <input
                  value={ipfsApiUrl}
                  onChange={(event) => setIpfsApiUrl(event.target.value)}
                />
                <span className="field-hint">Local API URL, for example http://127.0.0.1:5001.</span>
              </label>
              <label className="field">
                <span>IPFS gateway</span>
                <input
                  value={ipfsGatewayUrl}
                  onChange={(event) => setIpfsGatewayUrl(event.target.value)}
                />
                <span className="field-hint">Gateway used to verify the pinned manifest.</span>
              </label>
            </div>
            <p className="publisher-inline-note">
              Archive mirrors are public and may be hard to remove. Only hash-matched manifest
              copies are announced.
            </p>
          </details>
          <div className="publisher-actions">
            <button
              type="button"
              className="secondary-action"
              onClick={testEndpoints}
              disabled={isWorking || Boolean(endpointTestBlockedReason)}
              title={endpointTestBlockedReason || "Upload a small probe and publish a test event."}
            >
              <RadioTower size={16} />
              <span>{isWorking ? "Working" : "Test Endpoints"}</span>
            </button>
          </div>
          {endpointTestBlockedReason ? (
            <p className="publisher-inline-note">{endpointTestBlockedReason}</p>
          ) : (
            <p className="publisher-inline-note">
              Endpoint testing creates a tiny public probe on any server or relay that accepts it.
            </p>
          )}
          {endpointTestReport ? (
            <EndpointTestReportView report={endpointTestReport} />
          ) : null}
        </div>
      </div>

      <div className="publisher-grid">
        <div className="publisher-column">
          <SectionHeader title="3. Channel Details" meta="learner-facing" />
          <label className="field">
            <span>Channel title</span>
            <input
              value={channelTitle}
              onChange={(event) => setChannelTitle(event.target.value)}
            />
            <span className="field-hint">Learners see this as the channel name.</span>
          </label>
          <label className="field">
            <span>Channel note</span>
            <textarea
              value={channelDescription}
              onChange={(event) => setChannelDescription(event.target.value)}
            />
            <span className="field-hint">Optional source or permission note.</span>
          </label>
          <div className="publisher-actions">
            <button
              type="button"
              className="secondary-action"
              onClick={selectMedia}
              disabled={isWorking}
            >
              <FolderOpen size={16} />
              <span>Select Media</span>
            </button>
            <button
              type="button"
              className="primary-action"
              onClick={publishChannel}
              disabled={isWorking || Boolean(publishBlockedReason)}
              title={publishBlockedReason || "Publish the signed channel update."}
            >
              <UploadCloud size={16} />
              <span>{isWorking ? "Working" : "Publish Channel"}</span>
            </button>
          </div>
          <div className="publisher-readiness" aria-label="Publish readiness">
            {publishReadiness.map((item) => (
              <StatusChip key={item.label} label={item.label} tone={item.tone} />
            ))}
          </div>
          {publishBlockedReason ? (
            <p className="publisher-inline-note">{publishBlockedReason}</p>
          ) : null}
        </div>

        <div className="publisher-column">
          <SectionHeader title="4. Media" meta={`${lessonDrafts.length} files`} />
          <div className="publish-draft-list">
            {lessonDrafts.length ? (
              lessonDrafts.map((draft, index) => (
                <div className="publish-draft-row" key={`${draft.path}-${index}`}>
                  <input
                    value={draft.title}
                    onChange={(event) => updateDraft(index, { title: event.target.value })}
                    aria-label="Lesson title"
                  />
                  <select
                    value={draft.contentType}
                    onChange={(event) =>
                      updateDraft(index, {
                        contentType: event.target.value as PublishedLessonDraft["contentType"],
                      })
                    }
                    aria-label="Content type"
                  >
                    <option value="video">Video</option>
                    <option value="audio">Audio</option>
                    <option value="pdf">PDF</option>
                  </select>
                  <code>{fileNameFromPath(draft.path)}</code>
                </div>
              ))
            ) : (
              <EmptyState
                icon={UploadCloud}
                title="No files selected"
                detail="Select video, audio, or PDF lessons to publish."
              />
            )}
          </div>
        </div>
      </div>

      {publishInvite && publishResult ? (
        <div className="publisher-result">
          <div className="share-qr-frame">
            {shareQrDataUrl ? (
              <img src={shareQrDataUrl} alt="Teacher channel QR code" />
            ) : (
              <QrCode size={34} aria-hidden="true" />
            )}
          </div>
          <div className="publisher-share-copy">
            <div>
              <strong>Share invite</strong>
              <span>Learners can scan the QR or paste the invite text into Teacher Feed.</span>
            </div>
            <code>{publishInvite.canonicalChannelLink}</code>
            <div className="publisher-share-meta" aria-label="Channel invite verification">
              <StatusChip label={publishInvite.verificationCode} tone="neutral" />
              <StatusChip label={publishResult.manifestSha256} tone="neutral" />
            </div>
            <details className="publisher-advanced-link">
              <summary>Advanced naddr</summary>
              <code>{publishResult.naddr}</code>
            </details>
          </div>
          <div className="publisher-share-actions">
            <button type="button" className="primary-action" onClick={shareInvite}>
              <Copy size={16} />
              <span>Share Invite</span>
            </button>
            <button type="button" className="secondary-action" onClick={copyInviteText}>
              <Copy size={16} />
              <span>Copy Invite Text</span>
            </button>
            <button type="button" className="secondary-action" onClick={copyChannelLink}>
              <Copy size={16} />
              <span>Copy Link</span>
            </button>
          </div>
        </div>
      ) : null}

    </section>
  );
};

const PublisherSetupChecklist = ({
  steps,
}: {
  steps: Array<{ title: string; detail: string; complete: boolean }>;
}) => (
  <div className="publisher-step-list" aria-label="Publisher setup steps">
    {steps.map((step, index) => (
      <div
        className={step.complete ? "publisher-step publisher-step-complete" : "publisher-step"}
        key={step.title}
      >
        <span>{index + 1}</span>
        <div>
          <strong>{step.title}</strong>
          <p>{step.detail}</p>
        </div>
      </div>
    ))}
  </div>
);

const endpointLines = (value: string): string[] =>
  value
    .split(/[\n,]/)
    .map((line) => line.trim())
    .filter(Boolean);

const EndpointTestReportView = ({ report }: { report: PublisherEndpointTestReport }) => {
  const status = endpointTestStatus(report);

  return (
    <div className="endpoint-test-report" role="status">
      <div className="endpoint-test-summary">
        <StatusChip label={status.label} tone={status.tone} />
        <span>{report.messages.join(" ")}</span>
      </div>
      <div className="endpoint-test-grid">
        {report.blossomResults.map((result) => (
          <div className="endpoint-test-row" key={`${result.serverUrl}-${result.hash}`}>
            <StatusChip
              label={result.uploaded ? "Storage ok" : "Storage failed"}
              tone={result.uploaded ? "positive" : "danger"}
            />
            <code>{result.serverUrl}</code>
            <span>{result.message}</span>
          </div>
        ))}
        {report.relayResults.map((result) => (
          <div className="endpoint-test-row" key={result.relayUrl}>
            <StatusChip
              label={result.accepted ? "Relay ok" : "Relay failed"}
              tone={result.accepted ? "positive" : "danger"}
            />
            <code>{result.relayUrl}</code>
            <span>{result.message || "No relay message."}</span>
          </div>
        ))}
      </div>
    </div>
  );
};

const fileNameFromPath = (path: string): string =>
  path.split(/[\\/]/).filter(Boolean).pop() ?? path;

const titleFromPath = (path: string): string =>
  fileNameFromPath(path)
    .replace(/\.[^.]+$/, "")
    .replace(/[_-]+/g, " ")
    .trim() || "Untitled lesson";

const contentTypeFromPublishPath = (
  path: string,
): PublishedLessonDraft["contentType"] => {
  const extension = path.split(".").pop()?.toLowerCase();

  if (extension === "pdf") {
    return "pdf";
  }

  if (["mp3", "m4a", "aac", "wav", "flac", "ogg"].includes(extension ?? "")) {
    return "audio";
  }

  return "video";
};

const LiveSessionRow = ({
  session,
  teacher,
}: {
  session: LiveSession;
  teacher?: Teacher;
}) => (
  <article className="live-session-row">
    <div className={`round-icon ${liveSessionToneClass(session.status)}`}>
      <RadioTower size={17} />
    </div>
    <div className="live-session-copy">
      <div>
        <h2>{session.title}</h2>
        <StatusChip label={session.status.replace("-", " ")} tone={liveSessionTone(session.status)} />
      </div>
      <p>{session.recordingPolicy}</p>
      <span>
        {teacher?.displayName ?? "Unknown teacher"} · {session.provider.replace("-", " ")} ·{" "}
        {formatDate(session.startsAt)}
      </span>
      <code>{session.providerUrl}</code>
    </div>
    <StatusChip
      label={session.autoPublishArchive ? "Auto-publish archive" : "Manual archive review"}
      tone={session.autoPublishArchive ? "positive" : "warning"}
    />
  </article>
);

const LiveProviderMatrix = () => {
  const providers = [
    {
      name: "YouTube Live",
      status: "API-backed",
      detail: "Official live events API can track broadcasts; archive availability depends on channel settings.",
      tone: "positive" as const,
    },
    {
      name: "Mixlr",
      status: "Recording import",
      detail: "Live audio recordings can be imported into a channel, but do not assume open API automation.",
      tone: "warning" as const,
    },
    {
      name: "Custom RTMP",
      status: "Future direct host",
      detail: "Best architecture for direct teacher hosting once private publishing infrastructure exists.",
      tone: "neutral" as const,
    },
  ];

  return (
    <section className="side-panel provider-panel">
      <SectionHeader title="Live Providers" meta="truthful support" />
      <div className="compact-list">
        {providers.map((provider) => (
          <div className="provider-row" key={provider.name}>
            <strong>{provider.name}</strong>
            <StatusChip label={provider.status} tone={provider.tone} />
            <span>{provider.detail}</span>
          </div>
        ))}
      </div>
    </section>
  );
};

const CoursesPanel = ({
  groups,
  selectedCollectionId,
  onSelectCollection,
}: {
  groups: LibraryGroup[];
  selectedCollectionId: string;
  onSelectCollection: (collectionId: string) => void;
}) => (
  <section className="side-panel">
    <SectionHeader title="Courses" meta={`${groups.length} routes`} />
    <div className="compact-list">
      {groups.length ? (
        groups.map((group) => (
        <button
          type="button"
          className={
            selectedCollectionId === group.id
              ? "compact-row compact-row-active"
              : "compact-row"
          }
          key={group.id}
          onClick={() =>
            onSelectCollection(selectedCollectionId === group.id ? "all" : group.id)
          }
        >
          <div className="round-icon">
            <ListVideo size={16} />
          </div>
          <div>
            <strong>{group.label}</strong>
            <span>{group.lessonCount} items · {group.completedCount} complete</span>
          </div>
        </button>
      ))
      ) : (
        <p className="panel-empty">No courses yet.</p>
      )}
    </div>
  </section>
);

const QueueCompact = ({ jobs }: { jobs: Job[] }) => (
  <section className="side-panel">
    <SectionHeader title="Updates" meta={`${jobs.length} jobs`} />
    <div className="compact-list">
      {jobs.length ? (
        jobs.slice(0, 3).map((job) => {
          const detail = displayJobDetail(job.detail, job.state);

          return (
            <div className="compact-row" key={job.id}>
              <JobIcon state={job.state} />
              <div>
                <strong>{job.label}</strong>
                <span>{detail.summary}</span>
              </div>
            </div>
          );
        })
      ) : (
        <p className="panel-empty">No import or refresh jobs yet.</p>
      )}
    </div>
  </section>
);

const SourcesView = ({
  sources,
  lessons,
  jobs,
  runtimeDiagnostics,
  trustedCurators,
  query,
  mediaStorageAudit,
  mediaStorageBusyAction,
  busySourceAction,
  onClearSource,
  onDownloadSource,
  onRemoveTrustedCurator,
  onAuditMediaStorage,
  onCleanupMediaStorage,
}: {
  sources: Source[];
  lessons: Lesson[];
  jobs: Job[];
  runtimeDiagnostics: RuntimeDiagnostics;
  trustedCurators: TrustedCurator[];
  query: string;
  mediaStorageAudit: MediaStorageAudit | null;
  mediaStorageBusyAction: MediaStorageBusyAction;
  busySourceAction: BusySourceAction;
  onClearSource: (source: Source) => void;
  onDownloadSource: (source: Source) => void;
  onRemoveTrustedCurator: (curator: TrustedCurator) => void;
  onAuditMediaStorage: () => void;
  onCleanupMediaStorage: () => void;
}) => {
  const sourceQuery = normalizeSearch(query);
  const visibleSources = sources.filter((source) =>
    includesQuery(sourceQuery, [
      source.label,
      source.identifier,
      source.platform,
      source.feedFormat,
      source.feedTransport,
      source.trustState,
      source.authMode,
      source.capability.note,
      source.capability.reliability,
    ]),
  );
  const visibleTrustedCurators = trustedCurators.filter((curator) =>
    includesQuery(sourceQuery, [
      curator.displayName,
      curator.publicKey,
      curator.trustNote,
    ]),
  );
  const { capabilitySources, addedSources } = splitSourceRows(visibleSources);

  return (
    <div className="wide-page">
      <div className="page-heading">
        <div>
          <h2>Source Capability Matrix</h2>
          <p>Platform-level support is separate from playlists, feeds, and channels you add.</p>
        </div>
        <StatusChip label="No credentials in exports" tone="positive" />
      </div>

      <SourceReadinessPanel sources={visibleSources} runtimeDiagnostics={runtimeDiagnostics} />

      <StorageHygienePanel
        audit={mediaStorageAudit}
        busyAction={mediaStorageBusyAction}
        onAudit={onAuditMediaStorage}
        onCleanup={onCleanupMediaStorage}
      />

      <div className="matrix">
        <div className="matrix-header">
          <span>Source Type</span>
          <span>Metadata</span>
          <span>Download</span>
          <span>Auto-update</span>
          <span>Auth</span>
          <span>Reliability</span>
        </div>
        {capabilitySources.map((source) => (
          <div className="matrix-row" key={source.id}>
            <div className="matrix-source">
              <SourceIcon platform={source.platform} />
              <div>
                <strong>{source.label}</strong>
                <span>{source.capability.note}</span>
              </div>
            </div>
            <StatusChip
              label={capabilityLabel(source.capability.metadata)}
              toneClass={capabilityClass(source.capability.metadata)}
            />
            <StatusChip
              label={capabilityLabel(source.capability.download)}
              toneClass={capabilityClass(source.capability.download)}
            />
            <StatusChip
              label={capabilityLabel(source.capability.autoUpdate)}
              toneClass={capabilityClass(source.capability.autoUpdate)}
            />
            <StatusChip
              label={source.capability.authRequired ? source.authMode : "None"}
              tone={source.capability.authRequired ? "warning" : "positive"}
            />
            <span className="reliability-label">{source.capability.reliability}</span>
          </div>
        ))}
      </div>

      <section className="managed-sources">
        <SectionHeader title="Added Sources" meta={`${addedSources.length} active`} />
        {addedSources.length ? (
          <div className="source-management-list">
            {addedSources.map((source) => {
              const stats = getSourceStats(source.id, lessons, jobs);
              const isDownloading =
                busySourceAction?.sourceId === source.id &&
                busySourceAction.action === "download";
              const isClearing =
                busySourceAction?.sourceId === source.id && busySourceAction.action === "clear";
              const downloadBlocked = source.capability.download === "blocked";
              const downloadDisabledReason = downloadBlocked
                ? "This source type does not currently support downloads."
                : stats.missingFileCount === 0
                  ? "All file-backed media from this source is already downloaded."
                  : isClearing
                    ? "Wait until source cleanup finishes."
                    : "";

              return (
                <div className="managed-source-row" key={source.id}>
                  <div className="managed-source-copy">
                    <div className="matrix-source">
                      <SourceIcon platform={source.platform} />
                      <div>
                        <strong>{source.label}</strong>
                        <span>{source.identifier}</span>
                      </div>
                    </div>
                    <div className="managed-source-meta">
                      <StatusChip label={`${stats.itemCount} items`} tone="neutral" />
                      <StatusChip label={`${stats.localFileCount} local files`} tone="positive" />
                      <StatusChip
                        label={`${stats.missingFileCount} missing files`}
                        tone={stats.missingFileCount > 0 ? "warning" : "positive"}
                      />
                      <StatusChip label={`${stats.postCount} posts`} tone="neutral" />
                      <StatusChip label={`${stats.jobCount} jobs`} tone="neutral" />
                      <StatusChip label={source.feedFormat.replace(/-/g, " ")} tone="neutral" />
                      <StatusChip
                        label={trustLabel(source.trustState)}
                        tone={trustTone(source.trustState)}
                      />
                    </div>
                  </div>
                  <div className="managed-source-actions">
                    <button
                      type="button"
                      className="secondary-action"
                      onClick={() => onDownloadSource(source)}
                      disabled={
                        isDownloading ||
                        isClearing ||
                        downloadBlocked ||
                        stats.missingFileCount === 0
                      }
                      title={
                        downloadDisabledReason || "Download missing media files into the local library."
                      }
                    >
                      <Download size={15} />
                      <span>{isDownloading ? "Downloading" : "Download Media"}</span>
                    </button>
                    <button
                      type="button"
                      className="danger-action"
                      onClick={() => onClearSource(source)}
                      disabled={isDownloading || isClearing}
                      title="Remove this source and clear its lessons, jobs, and copied media."
                    >
                      <Trash2 size={15} />
                      <span>{isClearing ? "Clearing" : "Clear Source"}</span>
                    </button>
                    {downloadDisabledReason ? (
                      <p className="managed-source-disabled-note">{downloadDisabledReason}</p>
                    ) : null}
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <EmptyState
            icon={Rss}
            title="No added sources"
            detail="Use Import to add a playlist, feed, Telegram channel, archive item, video URL, or teacher channel."
          />
        )}
      </section>

      <section className="trusted-curators">
        <SectionHeader title="Trusted Curator Keys" meta={`${visibleTrustedCurators.length} stored`} />
        {visibleTrustedCurators.length ? (
          <div className="trusted-curator-list">
            {visibleTrustedCurators.map((curator) => (
              <div className="trusted-curator-row" key={curator.id}>
                <div className="matrix-source">
                  <KeyRound size={18} />
                  <div>
                    <strong>{curator.displayName}</strong>
                    <code>{curator.publicKey}</code>
                  </div>
                </div>
                <div className="trusted-curator-meta">
                  <StatusChip label={`Added ${formatDate(curator.addedAt)}`} tone="positive" />
                  {curator.trustNote ? (
                    <span className="trusted-curator-note">{curator.trustNote}</span>
                  ) : null}
                </div>
                <button
                  type="button"
                  className="danger-action"
                  onClick={() => onRemoveTrustedCurator(curator)}
                  title="Remove this key from trusted curators."
                >
                  <Trash2 size={15} />
                  <span>Remove Key</span>
                </button>
              </div>
            ))}
          </div>
        ) : (
          <EmptyState
            icon={KeyRound}
            title="No trusted curator keys"
            detail="Validate a signed Duroos manifest, then trust its curator key from the Import drawer."
          />
        )}
      </section>
    </div>
  );
};

const QueueView = ({
  jobs,
  sources,
  query,
  isRefreshingSources,
  onRefreshEnabledSources,
}: {
  jobs: Job[];
  sources: Map<string, Source>;
  query: string;
  isRefreshingSources: boolean;
  onRefreshEnabledSources: () => void;
}) => {
  const [activeFilter, setActiveFilter] = useState<QueueFilter>("all");
  const queueQuery = normalizeSearch(query);
  const visibleJobs = filterQueueJobs({
    jobs,
    sourceById: sources,
    filter: activeFilter,
    query,
    formatDate,
  });

  return (
    <div className="wide-page">
      <div className="page-heading">
        <div>
          <h2>Update Queue</h2>
          <p>Refreshes and downloads are visible, reversible, and source-aware.</p>
        </div>
        <button
          type="button"
          className="secondary-action"
          onClick={onRefreshEnabledSources}
          disabled={isRefreshingSources}
        >
          <RefreshCcw size={17} />
          <span>{isRefreshingSources ? "Refreshing" : "Refresh Enabled Sources"}</span>
        </button>
      </div>

      <div className="queue-filter-row" aria-label="Update queue filters">
        {queueFilters.map((filter) => {
          const count = jobs.filter((job) => queueFilterMatches(filter, job)).length;
          return (
            <button
              key={filter}
              type="button"
              className={filter === activeFilter ? "scope-chip scope-chip-active" : "scope-chip"}
              onClick={() => setActiveFilter(filter)}
              aria-pressed={filter === activeFilter}
            >
              <span>{queueFilterLabel(filter)}</span>
              <strong>{count}</strong>
            </button>
          );
        })}
      </div>

      <div className="job-list">
        {visibleJobs.length ? (
          visibleJobs.map((job) => {
            const detail = displayJobDetail(job.detail, job.state);
            return (
              <div className="job-row" key={job.id}>
                <JobIcon state={job.state} />
                <div className="job-copy">
                  <div>
                    <h2>{job.label}</h2>
                    <StatusChip label={job.state.replace(/-/g, " ")} tone={jobTone(job.state)} />
                  </div>
                  <p>{detail.summary}</p>
                  {detail.technicalDetail ? (
                    <details className="job-detail-disclosure">
                      <summary>{detail.technicalLabel}</summary>
                      <code>{detail.technicalDetail}</code>
                    </details>
                  ) : null}
                  <span>
                    {sources.get(job.sourceId ?? "")?.label ?? "System"} · {formatDate(job.updatedAt)}
                  </span>
                </div>
              </div>
            );
          })
        ) : (
          <EmptyState
            icon={History}
            title={queueQuery ? "No matching jobs" : "No queue items"}
            detail={
              queueQuery
                ? "Clear search or try a source, state, job title, or date."
                : "Import, refresh, and download work will appear here."
            }
          />
        )}
      </div>
    </div>
  );
};

function importModeConfig(mode: ImportMode): { description: string } {
  switch (mode) {
    case "local":
      return {
        description: "Copy video, audio, or PDF files into the local app library.",
      };
    case "source":
      return {
        description: "Add a public source URL and keep remote fetching under your control.",
      };
    case "feed":
      return {
        description: "Preview and follow signed teacher feeds or shared channel links.",
      };
    case "manifest":
      return {
        description: "Validate a Duroos manifest before importing, sharing, or trusting it.",
      };
    case "keys":
      return {
        description: "Manually save curator keys only after outside verification.",
      };
  }
}

const ImportDrawer = ({
  isOnlineMode,
  initialMode,
  trustedCurators,
  onEnableFetching,
  close,
  onResult,
  onTrustCurator,
}: {
  isOnlineMode: boolean;
  initialMode: ImportMode;
  trustedCurators: TrustedCurator[];
  onEnableFetching: () => void;
  close: () => void;
  onResult: (notice: string) => void;
  onTrustCurator: (
    displayName: string,
    publicKey: string,
    trustNote?: string,
  ) => Promise<TrustedCurator>;
}) => {
  const [mode, setMode] = useState<ImportMode>(initialMode);
  const [sourceUrl, setSourceUrl] = useState("");
  const [channelPreview, setChannelPreview] = useState<NostrChannelPreview | null>(null);
  const [previewedChannelRef, setPreviewedChannelRef] = useState("");
  const [manifestJson, setManifestJson] = useState("");
  const [validationMessage, setValidationMessage] = useState("");
  const [manifestReport, setManifestReport] = useState<ManifestValidationReport | null>(null);
  const [manualDisplayName, setManualDisplayName] = useState("");
  const [manualPublicKey, setManualPublicKey] = useState("");
  const [manualTrustNote, setManualTrustNote] = useState("");
  const [isWorking, setIsWorking] = useState(false);
  const drawerRef = useRef<HTMLElement | null>(null);
  const titleRef = useRef<HTMLHeadingElement | null>(null);
  const validatedCurator = manifestReport?.valid ? manifestReport.curator : undefined;
  const validatedCuratorTrusted = Boolean(
    validatedCurator &&
      (manifestReport?.trustedCuratorId ||
        trustedCurators.some((curator) => curator.publicKey === validatedCurator.publicKey)),
  );
  const canTrustValidatedCurator = Boolean(
    validatedCurator &&
      manifestReport?.valid &&
      manifestReport.trustState === "signed-untrusted" &&
      !validatedCuratorTrusted,
  );
  const canonicalChannelRef = useMemo(() => canonicalizeChannelRef(sourceUrl), [sourceUrl]);
  const sourceIsNostrChannel = Boolean(canonicalChannelRef);
  const channelPreviewReady = Boolean(
    channelPreview && canonicalChannelRef && previewedChannelRef === canonicalChannelRef,
  );
  const previewCuratorTrusted = Boolean(
    channelPreview?.curatorPublicKey &&
      trustedCurators.some((curator) => curator.publicKey === channelPreview.curatorPublicKey),
  );
  const canTrustPreviewCurator = Boolean(
    channelPreview?.curatorPublicKey &&
      channelPreview.trustState === "signed-untrusted" &&
      !previewCuratorTrusted,
  );
  const modeConfig = importModeConfig(mode);

  useEffect(() => {
    setMode(initialMode);
  }, [initialMode]);

  useEffect(() => {
    const previousActiveElement =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    titleRef.current?.focus();

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        close();
        return;
      }

      if (event.key !== "Tab") {
        return;
      }

      const drawer = drawerRef.current;
      if (!drawer) {
        return;
      }

      const focusable = Array.from(
        drawer.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((element) => element.offsetParent !== null || element === titleRef.current);

      if (focusable.length === 0) {
        return;
      }

      const first = focusable[0];
      const last = focusable[focusable.length - 1];

      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", handleKeyDown);

    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      previousActiveElement?.focus();
    };
  }, [close]);

  useEffect(() => {
    setChannelPreview(null);
    setPreviewedChannelRef("");
  }, [sourceUrl]);

  const runLocalImport = async () => {
    if (!isTauriRuntime()) {
      onResult("Local file picking requires the Tauri desktop runtime.");
      return;
    }

    setIsWorking(true);
    const paths = await chooseLocalMediaPaths();

    if (paths.length === 0) {
      setValidationMessage("No video, audio, or PDF files selected.");
      setIsWorking(false);
      return;
    }

    try {
      const result = await importLocalFiles(paths);
      onResult(result.messages.join(" "));
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setValidationMessage(message);
    } finally {
      setIsWorking(false);
    }
  };

  const runSourceIngest = async () => {
    const trimmedUrl = sourceUrl.trim();
    const sourceInput = canonicalChannelRef ?? trimmedUrl;

    if (!trimmedUrl) {
      setValidationMessage("Add a public feed, playlist, Telegram channel, or source URL first.");
      return;
    }

    if (sourceIsNostrChannel && !channelPreviewReady) {
      setValidationMessage("Preview this signed channel before following it.");
      return;
    }

    if (!isOnlineMode) {
      setValidationMessage("Switch to online fetch mode before subscribing to remote feeds.");
      return;
    }

    setIsWorking(true);
    setValidationMessage("");

    try {
      const result = await ingestSourceUrl(sourceInput);
      onResult(
        `${result.discovered} discovered, ${result.imported} added, ${result.skipped} skipped, ${result.failed} failed. ${result.messages.join(" ")}`,
      );
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setValidationMessage(message);
    } finally {
      setIsWorking(false);
    }
  };

  const runChannelPreview = async () => {
    const trimmedUrl = sourceUrl.trim();
    const channelRef = canonicalChannelRef;

    if (!trimmedUrl) {
      setValidationMessage("Paste a Nostr channel link first.");
      return;
    }

    if (!channelRef) {
      setValidationMessage("Channel preview expects an naddr, nostr:naddr link, or Duroos invite text.");
      return;
    }

    if (!isOnlineMode) {
      setValidationMessage("Switch to online fetch mode before previewing a channel.");
      return;
    }

    setIsWorking(true);
    setValidationMessage("");
    setChannelPreview(null);
    setPreviewedChannelRef("");

    try {
      const preview = await previewNostrChannel(channelRef);
      setChannelPreview(preview);
      setPreviewedChannelRef(channelRef);
      setValidationMessage(
        `${preview.title} verified. ${preview.lessonCount} lesson(s), ${preview.mediaCount} media file(s).`,
      );
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setValidationMessage(message);
    } finally {
      setIsWorking(false);
    }
  };

  const pasteChannelInvite = async () => {
    if (!navigator.clipboard?.readText) {
      setValidationMessage("Clipboard reading is unavailable. Paste the invite text manually.");
      return;
    }

    try {
      const clipboardText = await navigator.clipboard.readText();
      const channelRef = canonicalizeChannelRef(clipboardText);
      if (!channelRef) {
        setValidationMessage("Clipboard did not contain a Duroos Nostr channel invite.");
        return;
      }

      setSourceUrl(channelRef);
      setValidationMessage("Channel invite pasted. Preview before following.");
    } catch {
      setValidationMessage("Could not read the clipboard. Paste the invite text manually.");
    }
  };

  const runManifestValidation = async () => {
    try {
      const report = await validateCollectionManifest(manifestJson);
      setManifestReport(report);
      setValidationMessage(
        report.valid
          ? `Manifest is safe to import. ${report.trustState ? trustLabel(report.trustState) : "Unsigned"}. ${report.warnings.join(" ")}`
          : report.errors.join(" "),
      );
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setManifestReport(null);
      setValidationMessage(message);
    }
  };

  const trustValidatedCurator = async () => {
    if (!validatedCurator || !canTrustValidatedCurator) {
      return;
    }

    setIsWorking(true);
    try {
      const curator = await onTrustCurator(
        validatedCurator.displayName,
        validatedCurator.publicKey,
        `Trusted from validated manifest curator ${validatedCurator.id}.`,
      );
      setManifestReport((current) =>
        current
          ? {
              ...current,
              trustState: "signed-trusted",
              trustedCuratorId: curator.id,
            }
          : current,
      );
      setValidationMessage(`Trusted curator key saved for ${curator.displayName}.`);
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setValidationMessage(message);
    } finally {
      setIsWorking(false);
    }
  };

  const trustManualCurator = async () => {
    if (!manualDisplayName.trim() || !manualPublicKey.trim()) {
      setValidationMessage("Manual trusted curators need a display name and Ed25519 public key.");
      return;
    }

    setIsWorking(true);
    try {
      const curator = await onTrustCurator(
        manualDisplayName,
        manualPublicKey,
        manualTrustNote,
      );
      setManualDisplayName("");
      setManualPublicKey("");
      setManualTrustNote("");
      setValidationMessage(`Trusted curator key saved for ${curator.displayName}.`);
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setValidationMessage(message);
    } finally {
      setIsWorking(false);
    }
  };

  const trustPreviewCurator = async () => {
    if (!channelPreview?.curatorPublicKey || !canTrustPreviewCurator) {
      return;
    }

    setIsWorking(true);
    try {
      const curator = await onTrustCurator(
        channelPreview.curatorDisplayName,
        channelPreview.curatorPublicKey,
        `Trusted from previewed Duroos channel ${channelPreview.title}.`,
      );
      setChannelPreview((current) =>
        current
          ? {
              ...current,
              trustState: "signed-trusted",
            }
          : current,
      );
      setValidationMessage(`Trusted teacher key saved for ${curator.displayName}.`);
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      setValidationMessage(message);
    } finally {
      setIsWorking(false);
    }
  };

  return (
    <div className="drawer-backdrop" role="presentation">
      <aside
        className="import-drawer"
        ref={drawerRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-drawer-title"
      >
        <div className="drawer-header">
          <div>
            <h1 id="import-drawer-title" ref={titleRef} tabIndex={-1}>
              Import Content
            </h1>
            <p>{modeConfig.description}</p>
          </div>
          <button type="button" className="icon-button" onClick={close} aria-label="Close import">
            ×
          </button>
        </div>

        <div className="import-options" role="tablist" aria-label="Import mode">
          <ImportOption
            icon={FolderOpen}
            title="Add Local Files"
            detail="Copy selected video, audio, or PDF files into the app library."
            isActive={mode === "local"}
            onSelect={() => setMode("local")}
          />
          <ImportOption
            icon={Globe2}
            title="Source URL"
            detail="Read Archive items, feeds, public Telegram previews, and direct YouTube/Rumble/Odysee URLs."
            isActive={mode === "source"}
            onSelect={() => setMode("source")}
          />
          <ImportOption
            icon={Rss}
            title="Teacher Feed"
            detail="Follow signed Duroos manifests or shared Nostr channel links."
            isActive={mode === "feed"}
            onSelect={() => setMode("feed")}
          />
          <ImportOption
            icon={FileArchive}
            title="Manifest"
            detail="Validate a Duroos v2 manifest before importing or sharing."
            isActive={mode === "manifest"}
            onSelect={() => setMode("manifest")}
          />
          <ImportOption
            icon={KeyRound}
            title="Trusted Keys"
            detail="Save a curator key after outside verification."
            isActive={mode === "keys"}
            onSelect={() => setMode("keys")}
          />
        </div>

        {mode === "local" ? (
          <div className="import-mode-panel">
            <div className="ingest-hint">
              <HardDrive size={17} />
              <span>
                Local imports copy selected video, audio, or PDF files into the app library.
                Duplicate files are skipped by content hash when possible.
              </span>
            </div>
            <div className="drawer-actions">
              <button
                type="button"
                className="primary-action"
                onClick={runLocalImport}
                disabled={isWorking}
              >
                <Download size={17} />
                <span>{isWorking ? "Working" : "Import Local Files"}</span>
              </button>
            </div>
          </div>
        ) : null}

        {mode === "source" || mode === "feed" ? (
          <div className="import-mode-panel">
            <label className="field">
              <span>
                {mode === "feed" ? "Teacher feed or channel link" : "Source URL or identifier"}
              </span>
              {mode === "feed" ? (
                <textarea
                  value={sourceUrl}
                  onChange={(event) => setSourceUrl(event.target.value)}
                  placeholder="Duroos invite, nostr:naddr..., or https://teacher.example/manifest.json"
                />
              ) : (
                <input
                  value={sourceUrl}
                  onChange={(event) => setSourceUrl(event.target.value)}
                  placeholder="https://archive.org/details/..."
                />
              )}
              <span className="field-hint">
                {mode === "feed"
                  ? "Paste invite text, a signed manifest URL, teacher feed, or shared Nostr channel link."
                  : "Archive item, RSS feed, t.me channel, video URL, or public source URL."}
              </span>
            </label>

            <div className="source-example-list" aria-label="Accepted examples">
              {(mode === "feed"
                ? ["Duroos channel invite", "nostr:naddr1...", "https://teacher.example/duroos.json"]
                : [
                    "https://archive.org/details/...",
                    "https://example.com/feed.xml",
                    "https://t.me/channel",
                    "https://youtube.com/watch?v=...",
                  ]
              ).map((example) => (
                <code key={example}>{example}</code>
              ))}
            </div>

            {mode === "feed" || sourceIsNostrChannel ? (
              <ChannelPreviewPanel
                preview={channelPreview}
                isLoading={isWorking}
                canTrustCurator={canTrustPreviewCurator}
                curatorTrusted={previewCuratorTrusted}
                onTrustCurator={trustPreviewCurator}
              />
            ) : null}

            <div className="ingest-hint">
              {mode === "feed" ? <ShieldCheck size={17} /> : <Rss size={17} />}
              <span>
                {mode === "feed"
                  ? "Preview signed teacher feeds before importing. Trust a curator key only after confirming the identity outside the manifest."
                  : "Public feeds and public Telegram previews can become reviewable library rows. X/Rumble may need app-local yt-dlp cookies or manual import."}
              </span>
            </div>

            <div className="drawer-actions">
              {mode === "feed" ? (
                <button
                  type="button"
                  className="secondary-action"
                  onClick={pasteChannelInvite}
                  disabled={isWorking}
                  title="Read a Duroos channel invite from the clipboard."
                >
                  <Copy size={17} />
                  <span>Paste Invite</span>
                </button>
              ) : null}
              <button
                type="button"
                className="primary-action"
                onClick={runSourceIngest}
                disabled={isWorking || !isOnlineMode || (sourceIsNostrChannel && !channelPreviewReady)}
                title={
                  sourceIsNostrChannel && !channelPreviewReady
                    ? "Preview this signed channel before following it."
                    : isOnlineMode
                    ? "Fetch or subscribe to the entered source."
                    : "Switch to online fetch mode before using remote subscriptions or ingest."
                }
              >
                <Archive size={17} />
                <span>
                  {isWorking
                    ? "Working"
                    : sourceIsNostrChannel
                      ? "Follow Channel"
                      : mode === "feed"
                        ? "Import Feed"
                      : "Subscribe / Ingest"}
                </span>
              </button>
              {mode === "feed" || sourceIsNostrChannel ? (
                <button
                  type="button"
                  className="secondary-action"
                  onClick={runChannelPreview}
                  disabled={isWorking || !isOnlineMode}
                  title={
                    isOnlineMode
                      ? "Resolve network hints and validate the channel manifest."
                      : "Switch to online fetch mode before previewing a channel."
                  }
                >
                  <ShieldCheck size={17} />
                  <span>Preview Channel</span>
                </button>
              ) : null}
              {!isOnlineMode ? (
                <button type="button" className="secondary-action" onClick={onEnableFetching}>
                  <Wifi size={17} />
                  <span>Enable Fetching</span>
                </button>
              ) : null}
            </div>
            {!isOnlineMode ? (
              <p className="offline-inline-note" role="status">
                Offline mode keeps remote fetches disabled. Local file import and pasted manifest
                validation still work.
              </p>
            ) : null}
          </div>
        ) : null}

        {mode === "manifest" ? (
          <div className="import-mode-panel">
            <label className="field">
              <span>Collection manifest</span>
              <textarea
                className="manifest-box"
                value={manifestJson}
                onChange={(event) => {
                  setManifestJson(event.target.value);
                  setManifestReport(null);
                }}
              />
              <span className="field-hint">Paste raw Duroos manifest JSON for validation.</span>
            </label>
            <button type="button" className="secondary-action" onClick={runManifestValidation}>
              <ShieldCheck size={17} />
              <span>Validate Manifest</span>
            </button>
            {validatedCurator ? (
              <div className="curator-trust-panel">
                <div>
                  <strong>{validatedCurator.displayName}</strong>
                  <StatusChip
                    label={
                      validatedCuratorTrusted
                        ? "Trusted key"
                        : trustLabel(manifestReport?.trustState ?? "unsigned")
                    }
                    tone={validatedCuratorTrusted ? "positive" : "warning"}
                  />
                </div>
                <code>{validatedCurator.publicKey}</code>
                <button
                  type="button"
                  className="primary-action"
                  onClick={trustValidatedCurator}
                  disabled={isWorking || !canTrustValidatedCurator}
                  title={
                    validatedCuratorTrusted
                      ? "This curator key is already trusted."
                      : "Trust this key only after confirming the curator identity outside the manifest."
                  }
                >
                  <KeyRound size={17} />
                  <span>{validatedCuratorTrusted ? "Already Trusted" : "Trust Curator"}</span>
                </button>
              </div>
            ) : null}

            <div className="transport-reference-note">
              <RadioTower size={17} />
              <span>
                Nostr channel links resolve signed Duroos manifests from configured network hints.
                Blossom, HTTP, and enclosure URLs remain review-first downloads; IPFS CIDs and
                BitTorrent magnets validate only as manifest references.
              </span>
            </div>
          </div>
        ) : null}

        {mode === "keys" ? (
          <div className="manual-trust-panel">
            <SectionHeader title="Manual Curator Key" meta="advanced" />
            <p className="publisher-inline-note">
              Save a key only after verifying the teacher or curator through another channel.
            </p>
            <label className="field">
              <span>Display name</span>
              <input
                value={manualDisplayName}
                onChange={(event) => setManualDisplayName(event.target.value)}
              />
              <span className="field-hint">Curator or teacher name.</span>
            </label>
            <label className="field">
              <span>Ed25519 public key</span>
              <input
                value={manualPublicKey}
                onChange={(event) => setManualPublicKey(event.target.value)}
              />
              <span className="field-hint">Hex or base64 public key.</span>
            </label>
            <label className="field">
              <span>Trust note</span>
              <input
                value={manualTrustNote}
                onChange={(event) => setManualTrustNote(event.target.value)}
              />
              <span className="field-hint">Where you verified this key.</span>
            </label>
            <button
              type="button"
              className="secondary-action"
              onClick={trustManualCurator}
              disabled={isWorking}
            >
              <KeyRound size={17} />
              <span>Save Trusted Key</span>
            </button>
          </div>
        ) : null}
        {validationMessage ? <p className="validation-message">{validationMessage}</p> : null}
      </aside>
    </div>
  );
};

const ChannelPreviewPanel = ({
  preview,
  isLoading,
  canTrustCurator,
  curatorTrusted,
  onTrustCurator,
}: {
  preview: NostrChannelPreview | null;
  isLoading: boolean;
  canTrustCurator: boolean;
  curatorTrusted: boolean;
  onTrustCurator: () => void;
}) => (
  <div className="channel-preview-panel">
    <div className="channel-preview-heading">
      <div>
        <strong>{preview?.title ?? "Nostr channel preview"}</strong>
        <span>
          {preview
            ? `${preview.curatorDisplayName} · ${formatDate(preview.publishedAt)}`
            : isLoading
              ? "Resolving channel pointer and validating manifest."
              : "Preview before importing to verify the teacher, trust state, and media count."}
        </span>
      </div>
      <StatusChip
        label={preview ? trustLabel(preview.trustState) : "Not previewed"}
        tone={preview ? trustTone(preview.trustState) : "neutral"}
      />
    </div>
    <div className="channel-preview-stats">
      <span>{preview ? `${preview.lessonCount} lessons` : "Lessons unknown"}</span>
      <span>{preview ? `${preview.mediaCount} media refs` : "Media unknown"}</span>
      <span>{preview ? `${preview.relayCount} network hints` : "Network unknown"}</span>
      <span>
        {preview ? `${preview.blossomServerCount} Blossom servers` : "Storage unknown"}
      </span>
      <span>{preview ? `${preview.archiveMirrorCount} archive mirrors` : "Archive unknown"}</span>
    </div>
    {preview ? (
      <>
        <code>{preview.manifestSha256}</code>
        {preview.curatorPublicKey ? (
          <div className="channel-preview-key">
            <span>Teacher key</span>
            <code>{preview.curatorPublicKey}</code>
            <button
              type="button"
              className="secondary-action"
              onClick={onTrustCurator}
              disabled={!canTrustCurator}
              title={
                curatorTrusted || preview.trustState === "signed-trusted"
                  ? "This teacher key is already trusted."
                  : "Trust this key only after confirming the teacher identity outside the invite."
              }
            >
              <KeyRound size={15} />
              <span>
                {curatorTrusted || preview.trustState === "signed-trusted"
                  ? "Trusted Key"
                  : "Trust Teacher Key"}
              </span>
            </button>
          </div>
        ) : null}
        <div className="channel-preview-endpoints">
          {preview.relays.slice(0, 2).map((relay) => (
            <span key={relay}>{relay}</span>
          ))}
          {preview.blossomServers.slice(0, 2).map((server) => (
            <span key={server}>{server}</span>
          ))}
          {preview.archiveMirrors.slice(0, 2).map((mirror) => (
            <span key={mirror}>{mirror}</span>
          ))}
        </div>
      </>
    ) : null}
  </div>
);

const ImportOption = ({
  icon: Icon,
  title,
  detail,
  isActive,
  onSelect,
}: {
  icon: typeof FolderOpen;
  title: string;
  detail: string;
  isActive: boolean;
  onSelect: () => void;
}) => (
  <button
    type="button"
    className={isActive ? "import-option import-option-active" : "import-option"}
    onClick={onSelect}
    role="tab"
    aria-selected={isActive}
  >
    <Icon size={19} />
    <div>
      <strong>{title}</strong>
      <span>{detail}</span>
    </div>
  </button>
);

const EmptyState = ({
  icon: Icon,
  title,
  detail,
}: {
  icon: typeof Play;
  title: string;
  detail: string;
}) => (
  <div className="empty-state">
    <Icon size={22} />
    <strong>{title}</strong>
    <span>{detail}</span>
  </div>
);

const StatusChip = ({
  label,
  tone = "neutral",
  toneClass,
}: {
  label: string;
  tone?: "neutral" | "positive" | "warning" | "danger";
  toneClass?: string;
}) => <span className={`status-chip ${toneClass ?? `status-${tone}`}`}>{label}</span>;

const JobIcon = ({ state }: { state: Job["state"] }) => {
  if (state === "downloaded" || state === "found") {
    return (
      <div className="round-icon status-positive">
        <CheckCircle2 size={16} />
      </div>
    );
  }

  if (state === "needs-permission" || state === "skipped" || state === "unsupported") {
    return (
      <div className="round-icon status-warning">
        <AlertTriangle size={16} />
      </div>
    );
  }

  if (state === "failed" || state === "failed-auth") {
    return (
      <div className="round-icon status-danger">
        <Lock size={16} />
      </div>
    );
  }

  return (
    <div className="round-icon">
      <Clock3 size={16} />
    </div>
  );
};

const SourceIcon = ({ platform }: { platform: Source["platform"] }) => {
  const props = { size: 18 };

  switch (platform) {
    case "local-files":
      return <FolderOpen {...props} />;
    case "telegram":
      return <Wifi {...props} />;
    case "rss-feed":
      return <Rss {...props} />;
    case "archive-org":
      return <Archive {...props} />;
    case "youtube":
      return <Play {...props} />;
    case "x":
      return <Globe2 {...props} />;
    case "rumble":
      return <Download {...props} />;
    case "odysee":
      return <Globe2 {...props} />;
    case "teacher-relay":
      return <Rss {...props} />;
  }
};

const jobTone = (state: Job["state"]): "neutral" | "positive" | "warning" | "danger" => {
  if (state === "downloaded" || state === "found" || state === "archived") {
    return "positive";
  }

  if (state === "failed" || state === "failed-auth") {
    return "danger";
  }

  if (state === "needs-permission" || state === "unsupported" || state === "skipped") {
    return "warning";
  }

  return "neutral";
};

const liveSessionTone = (
  status: LiveSession["status"],
): "neutral" | "positive" | "warning" | "danger" => {
  if (status === "live" || status === "recording" || status === "archived") {
    return "positive";
  }

  if (status === "processing" || status === "manual-import") {
    return "warning";
  }

  return "neutral";
};

const liveSessionToneClass = (status: LiveSession["status"]): string =>
  `status-${liveSessionTone(status)}`;

export default App;
