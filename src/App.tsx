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
} from "lucide-react";
import QRCode from "qrcode";
import { useEffect, useMemo, useState } from "react";
import {
  addTrustedCurator,
  chooseLocalMediaPaths,
  clearSourceContent,
  downloadSourceMedia,
  getPhoneMediaSession,
  getAppSnapshot,
  getRuntimeDiagnostics,
  ingestSourceUrl,
  importLocalFiles,
  isTauriRuntime,
  removeTrustedCurator,
  resolveMediaFileUrl,
  startPhoneMediaSession,
  stopPhoneMediaSession,
  validateCollectionManifest,
} from "./lib/tauri";
import type { ManifestValidationReport } from "./domain/collectionManifest";
import type {
  AppSnapshot,
  CapabilityLevel,
  Collection,
  ContentType,
  Job,
  Lesson,
  LiveSession,
  MediaFile,
  PhoneMediaSession,
  ProvenanceRecord,
  RuntimeDiagnostics,
  Source,
  SourcePlatform,
  Teacher,
  TeacherRelay,
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
import { seedSnapshot } from "./data/seed";

type ViewMode = "library" | "relays" | "sources" | "queue";
type BusySourceAction = { sourceId: string; action: "clear" | "download" } | null;
type PhoneAccessBusyAction = "start" | "stop" | null;

const defaultRuntimeDiagnostics: RuntimeDiagnostics = {
  desktopRuntimeAvailable: isTauriRuntime(),
  ytDlpAvailable: false,
  ytDlpCookiesConfigured: false,
  messages: ["Runtime diagnostics have not been checked yet."],
};

const downloaderStatus = (
  runtimeDiagnostics: RuntimeDiagnostics,
): { label: string; tone: "neutral" | "positive" | "warning" } => {
  if (!runtimeDiagnostics.desktopRuntimeAvailable) {
    return { label: "Desktop check", tone: "neutral" };
  }

  return runtimeDiagnostics.ytDlpAvailable
    ? { label: "yt-dlp ready", tone: "positive" }
    : { label: "yt-dlp needed", tone: "warning" };
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

  return mediaFile.hashVerificationState === "matched" ? "Verified local file" : "Downloaded";
};

const availabilityTone = (
  lesson: Lesson,
  mediaFile?: MediaFile,
): "neutral" | "positive" | "warning" => {
  if (!isFileBackedContentType(lesson.contentType)) {
    return "neutral";
  }

  return mediaFile ? "positive" : "warning";
};

const App = () => {
  const [snapshot, setSnapshot] = useState<AppSnapshot>(seedSnapshot);
  const [query, setQuery] = useState("");
  const [selectedLessonId, setSelectedLessonId] = useState(seedSnapshot.lessons[0]?.id ?? "");
  const [viewMode, setViewMode] = useState<ViewMode>("library");
  const [isOnlineMode, setIsOnlineMode] = useState(false);
  const [isImportOpen, setIsImportOpen] = useState(false);
  const [systemNotice, setSystemNotice] = useState("");
  const [busySourceAction, setBusySourceAction] = useState<BusySourceAction>(null);
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

  const filteredLessons = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();

    if (!normalizedQuery) {
      return snapshot.lessons;
    }

    return snapshot.lessons.filter((lesson) => {
      const teacher = teacherById.get(lesson.teacherId);
      const collection = collectionById.get(lesson.collectionId);
      const source = sourceById.get(lesson.sourceId);

      return [
        lesson.title,
        lesson.description,
        lesson.sourceUrl,
        teacher?.displayName,
        collection?.title,
        source?.label,
      ].some((value) => value?.toLowerCase().includes(normalizedQuery));
    });
  }, [collectionById, query, snapshot.lessons, sourceById, teacherById]);

  const selectedLesson =
    filteredLessons.find((lesson) => lesson.id === selectedLessonId) ??
    snapshot.lessons.find((lesson) => lesson.id === selectedLessonId) ??
    snapshot.lessons[0];
  const selectedMediaFile = selectedLesson ? mediaByLessonId.get(selectedLesson.id) : undefined;

  useEffect(() => {
    let isMounted = true;

    setSelectedMediaUrl("");
    setSelectedMediaError("");

    if (!selectedLesson || !selectedMediaFile || selectedLesson.contentType === "post") {
      return () => {
        isMounted = false;
      };
    }

    resolveMediaFileUrl(selectedMediaFile.id)
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
  }, [selectedLesson, selectedMediaFile]);

  const continueLessons = snapshot.lessons
    .filter((lesson) => {
      const progress = watchByLessonId.get(lesson.id);
      return progress && !progress.completed;
    })
    .slice(0, 4);

  const newLessons = snapshot.lessons
    .filter((lesson) => !mediaByLessonId.has(lesson.id))
    .concat(snapshot.lessons.filter((lesson) => mediaByLessonId.has(lesson.id)).slice(0, 2))
    .slice(0, 4);

  const phoneEligibleMediaCount = snapshot.lessons.filter((lesson) => {
    const mediaFile = mediaByLessonId.get(lesson.id);
    return (
      mediaFile?.importStatus === "ready" &&
      (lesson.contentType === "video" || lesson.contentType === "audio")
    );
  }).length;

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

  const handleCopyPhoneLink = async () => {
    if (!phoneSession?.playlistUrl) {
      return;
    }

    try {
      await navigator.clipboard.writeText(phoneSession.playlistUrl);
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

  const handleDownloadSource = async (source: Source) => {
    const missingCount = getSourceStats(source.id, snapshot.lessons, snapshot.jobs).missingFileCount;

    if (missingCount === 0) {
      setSystemNotice(`No missing video, audio, or PDF files for ${source.label}.`);
      return;
    }

    let refreshTimer: number | undefined;

    try {
      setBusySourceAction({ sourceId: source.id, action: "download" });
      setSystemNotice(`Downloading ${missingCount} file-backed item(s) from ${source.label}...`);
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
        const result = await ingestSourceUrl(source.identifier);
        summaries.push(
          `${source.label}: ${result.imported} added, ${result.skipped} duplicate(s), ${result.failed} failed.`,
        );
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
    <div className="app-shell">
      <Sidebar
        viewMode={viewMode}
        setViewMode={setViewMode}
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
          openImport={() => setIsImportOpen(true)}
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
            selectedLesson={selectedLesson}
            selectedMediaFile={selectedMediaFile}
            selectedMediaUrl={selectedMediaUrl}
            selectedMediaError={selectedMediaError}
            teacherById={teacherById}
            collectionById={collectionById}
            sourceById={sourceById}
            mediaByLessonId={mediaByLessonId}
            provenanceById={provenanceById}
            watchByLessonId={watchByLessonId}
            setSelectedLessonId={setSelectedLessonId}
            onOpenImport={() => setIsImportOpen(true)}
            runtimeDiagnostics={runtimeDiagnostics}
            phoneSession={phoneSession}
            phoneEligibleMediaCount={phoneEligibleMediaCount}
            phoneAccessBusyAction={phoneAccessBusyAction}
            phoneAccessNotice={phoneAccessNotice}
            onStartPhoneAccess={handleStartPhoneAccess}
            onStopPhoneAccess={handleStopPhoneAccess}
            onCopyPhoneLink={handleCopyPhoneLink}
          />
        ) : null}

        {viewMode === "relays" ? (
          <RelaysView
            relays={snapshot.teacherRelays}
            liveSessions={snapshot.liveSessions}
            teachers={teacherById}
          />
        ) : null}

        {viewMode === "sources" ? (
          <SourcesView
            sources={snapshot.sources}
            lessons={snapshot.lessons}
            jobs={snapshot.jobs}
            runtimeDiagnostics={runtimeDiagnostics}
            trustedCurators={snapshot.trustedCurators}
            busySourceAction={busySourceAction}
            onClearSource={handleClearSource}
            onDownloadSource={handleDownloadSource}
            onRemoveTrustedCurator={handleRemoveTrustedCurator}
          />
        ) : null}

        {viewMode === "queue" ? (
          <QueueView
            jobs={snapshot.jobs}
            sources={sourceById}
            isRefreshingSources={isRefreshingSources}
            onRefreshEnabledSources={handleRefreshEnabledSources}
          />
        ) : null}
      </main>

      {isImportOpen ? (
        <ImportDrawer
          isOnlineMode={isOnlineMode}
          trustedCurators={snapshot.trustedCurators}
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
}

const Sidebar = ({
  viewMode,
  setViewMode,
}: SidebarProps) => {
  const navItems: { mode: ViewMode; label: string; icon: typeof Library }[] = [
    { mode: "library", label: "Library", icon: Library },
    { mode: "relays", label: "Curator Relays", icon: Rss },
    { mode: "sources", label: "Sources", icon: Database },
    { mode: "queue", label: "Update Queue", icon: History },
  ];

  return (
    <aside className="sidebar" aria-label="Primary">
      <div className="brand-lockup">
        <div className="brand-mark">
          <BookOpen size={22} />
        </div>
        <div>
          <p className="brand-name">Duroos Watcher</p>
          <p className="brand-subtitle">Local study library</p>
        </div>
      </div>

      <nav className="nav-stack">
        {navItems.map(({ mode, label, icon: Icon }) => (
          <button
            key={mode}
            type="button"
            className={viewMode === mode ? "nav-item nav-item-active" : "nav-item"}
            onClick={() => setViewMode(mode)}
          >
            <Icon size={18} />
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

  const downloader = downloaderStatus(runtimeDiagnostics);

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
          <div className="runtime-pill">
            <HardDrive size={16} />
            <span>{isTauriRuntime() ? "Desktop runtime" : "Browser preview"}</span>
          </div>
          <div
            className={
              downloader.tone === "positive"
                ? "runtime-pill status-positive"
                : downloader.tone === "warning"
                  ? "runtime-pill status-warning"
                  : "runtime-pill"
            }
            title={runtimeDiagnostics.messages.join(" ")}
          >
            <Download size={16} />
            <span>{downloader.label}</span>
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
      <div className="top-bar-search">
        <div className="search-wrap">
          <Search size={18} />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search videos, audio, PDFs, posts, teachers, sources"
            aria-label="Search library"
          />
        </div>
      </div>
    </header>
  );
};

const viewTitle = (viewMode: ViewMode): string => {
  switch (viewMode) {
    case "library":
      return "Media Library";
    case "relays":
      return "Teacher Relays";
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
  selectedLesson?: Lesson;
  selectedMediaFile?: MediaFile;
  selectedMediaUrl: string;
  selectedMediaError: string;
  teacherById: Map<string, Teacher>;
  collectionById: Map<string, Collection>;
  sourceById: Map<string, Source>;
  mediaByLessonId: Map<string, MediaFile>;
  provenanceById: Map<string, ProvenanceRecord>;
  watchByLessonId: Map<string, WatchState>;
  setSelectedLessonId: (lessonId: string) => void;
  onOpenImport: () => void;
  runtimeDiagnostics: RuntimeDiagnostics;
  phoneSession: PhoneMediaSession | null;
  phoneEligibleMediaCount: number;
  phoneAccessBusyAction: PhoneAccessBusyAction;
  phoneAccessNotice: string;
  onStartPhoneAccess: () => void;
  onStopPhoneAccess: () => void;
  onCopyPhoneLink: () => void;
}

const Dashboard = ({
  snapshot,
  filteredLessons,
  continueLessons,
  newLessons,
  selectedLesson,
  selectedMediaFile,
  selectedMediaUrl,
  selectedMediaError,
  teacherById,
  collectionById,
  sourceById,
  mediaByLessonId,
  provenanceById,
  watchByLessonId,
  setSelectedLessonId,
  onOpenImport,
  runtimeDiagnostics,
  phoneSession,
  phoneEligibleMediaCount,
  phoneAccessBusyAction,
  phoneAccessNotice,
  onStartPhoneAccess,
  onStopPhoneAccess,
  onCopyPhoneLink,
}: DashboardProps) => (
  <div className="dashboard-grid">
    <section className="content-column">
      {selectedLesson ? (
        <PlayerPanel
          lesson={selectedLesson}
          teacher={teacherById.get(selectedLesson.teacherId)}
          collection={collectionById.get(selectedLesson.collectionId)}
          source={sourceById.get(selectedLesson.sourceId)}
          mediaFile={selectedMediaFile}
          mediaUrl={selectedMediaUrl}
          mediaError={selectedMediaError}
          provenance={provenanceById.get(selectedLesson.provenanceId)}
          progress={getLessonProgress(
            selectedLesson,
            watchByLessonId.get(selectedLesson.id),
          )}
        />
      ) : (
        <PlayerEmptyPanel onOpenImport={onOpenImport} runtimeDiagnostics={runtimeDiagnostics} />
      )}

      <SectionHeader title="Continue" meta={`${continueLessons.length} active items`} />
      <div className="lesson-row">
        {continueLessons.length ? (
          continueLessons.map((lesson) => (
            <LessonCard
              key={lesson.id}
              lesson={lesson}
              teacher={teacherById.get(lesson.teacherId)}
              collection={collectionById.get(lesson.collectionId)}
              source={sourceById.get(lesson.sourceId)}
              mediaFile={mediaByLessonId.get(lesson.id)}
              progress={getLessonProgress(lesson, watchByLessonId.get(lesson.id))}
              onSelect={() => setSelectedLessonId(lesson.id)}
            />
          ))
        ) : (
          <EmptyState
            icon={Play}
            title="No active study items"
            detail="Start any imported video, audio, PDF, or post and it will appear here."
          />
        )}
      </div>

      <SectionHeader title="Feed Inbox" meta="Subscribed source updates" />
      <div className="lesson-grid">
        {newLessons.length ? (
          newLessons.map((lesson) => (
            <LessonCard
              key={lesson.id}
              lesson={lesson}
              teacher={teacherById.get(lesson.teacherId)}
              collection={collectionById.get(lesson.collectionId)}
              source={sourceById.get(lesson.sourceId)}
              mediaFile={mediaByLessonId.get(lesson.id)}
              progress={getLessonProgress(lesson, watchByLessonId.get(lesson.id))}
              onSelect={() => setSelectedLessonId(lesson.id)}
            />
          ))
        ) : (
          <EmptyState
            icon={Rss}
            title="No feed items"
            detail="Use Import to add local media, a public feed, Archive item, or direct video URL."
          />
        )}
      </div>

      <SectionHeader
        title="Curator Relay Feeds"
        meta={`${snapshot.teacherRelays.length} subscriptions`}
      />
      <div className="relay-grid">
        {snapshot.teacherRelays.length ? (
          snapshot.teacherRelays.map((relay) => (
            <RelayCard
              key={relay.id}
              relay={relay}
              teacher={teacherById.get(relay.teacherId)}
            />
          ))
        ) : (
          <EmptyState
            icon={Rss}
            title="No relay subscriptions"
            detail="Add a signed curator manifest or teacher feed when one is available."
          />
        )}
      </div>

      <SectionHeader title="Library Search" meta={`${filteredLessons.length} items`} />
      <div className="library-list">
        {filteredLessons.length ? (
          filteredLessons.map((lesson) => (
            <LessonRow
              key={lesson.id}
              lesson={lesson}
              teacher={teacherById.get(lesson.teacherId)}
              collection={collectionById.get(lesson.collectionId)}
              source={sourceById.get(lesson.sourceId)}
              mediaFile={mediaByLessonId.get(lesson.id)}
              onSelect={() => setSelectedLessonId(lesson.id)}
            />
          ))
        ) : (
          <EmptyState
            icon={Search}
            title="No study items imported"
            detail="Import video, audio, PDF files, teacher posts, source feeds, or direct video URLs."
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
      <SourceReadinessPanel sources={snapshot.sources} runtimeDiagnostics={runtimeDiagnostics} />
      <TeacherPanel teachers={snapshot.teachers} lessons={snapshot.lessons} />
      <LiveSessionPanel
        liveSessions={snapshot.liveSessions}
        teachers={teacherById}
      />
      <CoursesPanel collections={snapshot.collections} />
      <QueueCompact jobs={snapshot.jobs} />
    </aside>
  </div>
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
  progress: number;
  onSelect: () => void;
}

const LessonCard = ({
  lesson,
  teacher,
  collection,
  source,
  mediaFile,
  progress,
  onSelect,
}: LessonCardProps) => (
  <button type="button" className="lesson-card" onClick={onSelect}>
    <LessonThumb
      tone={lesson.thumbnailTone}
      contentType={lesson.contentType}
      badge={lessonBadge(lesson)}
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
}: {
  tone: Lesson["thumbnailTone"];
  contentType: Lesson["contentType"];
  badge: string;
}) => (
  <div className={`lesson-thumb thumb-${tone}`} aria-hidden="true">
    <div className="thumb-book">
      {(() => {
        const Icon = contentTypeIcon(contentType);
        return <Icon size={28} />;
      })()}
    </div>
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
  onSelect: () => void;
}

const LessonRow = ({
  lesson,
  teacher,
  collection,
  source,
  mediaFile,
  onSelect,
}: LessonRowProps) => (
  <button type="button" className="lesson-row-item" onClick={onSelect}>
    <LessonThumb
      tone={lesson.thumbnailTone}
      contentType={lesson.contentType}
      badge={lessonBadge(lesson)}
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
  mediaUrl: string;
  mediaError: string;
  provenance?: ProvenanceRecord;
  progress: number;
}

const PlayerPanel = ({
  lesson,
  teacher,
  collection,
  source,
  mediaFile,
  mediaUrl,
  mediaError,
  provenance,
  progress,
}: PlayerPanelProps) => (
  <section className="player-panel" aria-label="Selected lesson">
    <PlayerSurface
      lesson={lesson}
      mediaFile={mediaFile}
      mediaUrl={mediaUrl}
      mediaError={mediaError}
    />
    <div className="player-copy">
      <h1>{lesson.title}</h1>
      <p>{teacher?.displayName ?? "Unknown teacher"}</p>
      <div className="player-tags">
        <StatusChip label={collection?.title ?? "Unsorted"} tone="neutral" />
        <StatusChip label={source?.label ?? "Source unknown"} tone="neutral" />
        <StatusChip label={contentTypeLabel(lesson.contentType)} tone="neutral" />
        <StatusChip label={availabilityLabel(lesson, mediaFile)} tone={availabilityTone(lesson, mediaFile)} />
      </div>
    </div>
    <div className="progress-track progress-large" aria-label={`${progress}% watched`}>
      <span style={{ width: `${progress}%` }} />
    </div>
    <div className="provenance-box">
      <div>
        <ShieldCheck size={17} />
        <strong>Source Record</strong>
      </div>
      <p>{provenance?.permissionNote ?? "No source record found."}</p>
      <code>{provenance?.originUrl ?? lesson.sourceUrl}</code>
    </div>
  </section>
);

const PlayerSurface = ({
  lesson,
  mediaFile,
  mediaUrl,
  mediaError,
}: {
  lesson: Lesson;
  mediaFile?: MediaFile;
  mediaUrl: string;
  mediaError: string;
}) => {
  if (mediaUrl && lesson.contentType === "video") {
    return (
      <div className="player-frame player-frame-live">
        <video className="media-player" controls preload="metadata" src={mediaUrl} />
      </div>
    );
  }

  if (mediaUrl && lesson.contentType === "audio") {
    return (
      <div className={`player-frame audio-player-frame thumb-${lesson.thumbnailTone}`}>
        <div className="audio-player-icon" aria-hidden="true">
          <Volume2 size={34} />
        </div>
        <audio className="audio-player" controls preload="metadata" src={mediaUrl} />
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
    <div className={`player-frame thumb-${lesson.thumbnailTone}`}>
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
        <h1>Ready for video, audio, and PDFs</h1>
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

interface PhoneAccessPanelProps {
  session: PhoneMediaSession | null;
  eligibleMediaCount: number;
  busyAction: PhoneAccessBusyAction;
  notice: string;
  desktopRuntimeAvailable: boolean;
  onStart: () => void;
  onStop: () => void;
  onCopyLink: () => void;
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
  const playlistUrl = session?.playlistUrl ?? "";
  const isActive = Boolean(session?.active && playlistUrl);
  const startDisabled =
    !desktopRuntimeAvailable || busyAction !== null || eligibleMediaCount === 0;

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
          <div className="phone-actions">
            <button
              type="button"
              className="secondary-action"
              onClick={onCopyLink}
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
        </div>
      )}

      <div className="phone-access-footnote">
        <Wifi size={15} />
        <span>Only available on your Wi-Fi while sharing is on.</span>
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
  runtimeDiagnostics,
}: {
  sources: Source[];
  runtimeDiagnostics: RuntimeDiagnostics;
}) => {
  const sourceByPlatform = new Map(sources.map((source) => [source.platform, source]));
  const orderedSources = sourcePriority
    .map((platform) => sourceByPlatform.get(platform))
    .filter((source): source is Source => Boolean(source));

  return (
    <section className="side-panel source-readiness-panel">
      <SectionHeader title="Source Readiness" meta="v1 pulls" />
      <div className="source-readiness-list">
        {orderedSources.slice(0, 7).map((source) => {
          const readiness = sourceReadiness(source, runtimeDiagnostics);

          return (
            <div className="source-readiness-row" key={source.id}>
              <div className="round-icon">
                <SourceIcon platform={source.platform} />
              </div>
              <div>
                <strong>{source.label}</strong>
                <span>{readiness.detail}</span>
              </div>
              <StatusChip label={readiness.label} tone={readiness.tone} />
            </div>
          );
        })}
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
          ? runtimeDiagnostics.ytDlpAvailable
            ? "User pull"
            : "Needs tool"
          : "Desktop check",
        tone: runtimeDiagnostics.desktopRuntimeAvailable && runtimeDiagnostics.ytDlpAvailable
          ? "positive"
          : runtimeDiagnostics.desktopRuntimeAvailable
            ? "warning"
            : "neutral",
        detail: runtimeDiagnostics.desktopRuntimeAvailable
          ? "Metadata via feeds/API; permitted downloads use local yt-dlp."
          : "Desktop app checks local yt-dlp before media downloads.",
      };
    case "rumble":
      return {
        label: runtimeDiagnostics.desktopRuntimeAvailable
          ? runtimeDiagnostics.ytDlpAvailable
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

const TeacherPanel = ({
  teachers,
  lessons,
}: {
  teachers: Teacher[];
  lessons: Lesson[];
}) => (
  <section className="side-panel">
    <SectionHeader title="Teachers" meta={`${teachers.length} saved`} />
    <div className="compact-list">
      {teachers.map((teacher) => (
        <div className="compact-row" key={teacher.id}>
          <div className="round-icon">
            <UserRound size={16} />
          </div>
          <div>
            <strong>{teacher.displayName}</strong>
            <span>
              {lessons.filter((lesson) => lesson.teacherId === teacher.id).length} items
            </span>
          </div>
        </div>
      ))}
    </div>
  </section>
);

const RelayCard = ({ relay, teacher }: { relay: TeacherRelay; teacher?: Teacher }) => (
  <article className="relay-card">
    <div className="relay-card-header">
      <div className="round-icon">
        <Rss size={16} />
      </div>
      <StatusChip
        label={relay.autoDownload ? "Auto-download on" : "Review first"}
        tone={relay.autoDownload ? "positive" : "warning"}
      />
    </div>
    <h3>{relay.title}</h3>
    <p>{relay.description}</p>
    <div className="relay-meta">
      <span>{teacher?.displayName ?? "Unknown teacher"}</span>
      <span>{relay.visibility}</span>
      <span>{relay.trustPolicy.replace(/-/g, " ")}</span>
    </div>
    <div className="managed-source-meta">
      <StatusChip label={relay.feedFormat.replace(/-/g, " ")} tone="neutral" />
      <StatusChip label={trustLabel(relay.trustState)} tone={trustTone(relay.trustState)} />
    </div>
    <code>{relay.feedUrl}</code>
    <div className="relay-footer">
      <span>{relay.subscriberCount} subscribers</span>
      <span>Last publish {formatDate(relay.lastPublishedAt)}</span>
    </div>
  </article>
);

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
  relays,
  liveSessions,
  teachers,
}: {
  relays: TeacherRelay[];
  liveSessions: LiveSession[];
  teachers: Map<string, Teacher>;
}) => (
  <div className="wide-page">
    <div className="page-heading">
      <div>
        <h1>Curator Relays</h1>
        <p>
          Teacher and curator-owned feeds publish uploaded classes, signed manifests, live archives,
          and downloadable media enclosures.
        </p>
      </div>
      <StatusChip label="Teacher upload tools later" tone="neutral" />
    </div>

    <div className="relay-layout">
      <section className="relay-main">
        <SectionHeader title="Subscribed Feeds" meta={`${relays.length} relays`} />
        <div className="relay-grid">
          {relays.map((relay) => (
            <RelayCard
              key={relay.id}
              relay={relay}
              teacher={teachers.get(relay.teacherId)}
            />
          ))}
        </div>
      </section>

      <aside className="relay-aside">
        <section className="side-panel relay-publish-panel">
          <SectionHeader title="Publish Model" meta="signed feeds" />
          <div className="publish-steps">
            <div>
              <UploadCloud size={17} />
              <span>Teacher or curator publishes class media and source notes.</span>
            </div>
            <div>
              <ShieldCheck size={17} />
              <span>Feed signs lesson metadata, hashes, and provenance.</span>
            </div>
            <div>
              <Download size={17} />
              <span>Subscribers fetch or auto-download approved enclosures.</span>
            </div>
          </div>
        </section>
        <LiveProviderMatrix />
      </aside>
    </div>

    <SectionHeader title="Live Lesson Capture" meta={`${liveSessions.length} sessions`} />
    <div className="live-session-list">
      {liveSessions.map((session) => (
        <LiveSessionRow
          key={session.id}
          session={session}
          teacher={teachers.get(session.teacherId)}
        />
      ))}
    </div>
  </div>
);

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
      detail: "Live audio recordings can be brought into a relay, but do not assume open API automation.",
      tone: "warning" as const,
    },
    {
      name: "Paltalk",
      status: "Manual import",
      detail: "No official ingest API is assumed; teacher uploads recordings before publishing.",
      tone: "warning" as const,
    },
    {
      name: "Custom RTMP",
      status: "Future relay",
      detail: "Best architecture for direct teacher hosting once a private relay server exists.",
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

const CoursesPanel = ({ collections }: { collections: Collection[] }) => (
  <section className="side-panel">
    <SectionHeader title="Courses" meta={`${collections.length} routes`} />
    <div className="compact-list">
      {collections.map((collection) => (
        <div className="compact-row" key={collection.id}>
          <div className="round-icon">
            <ListVideo size={16} />
          </div>
          <div>
            <strong>{collection.title}</strong>
            <span>{collection.lessonCount} items · {collection.ownerLabel}</span>
          </div>
        </div>
      ))}
    </div>
  </section>
);

const QueueCompact = ({ jobs }: { jobs: Job[] }) => (
  <section className="side-panel">
    <SectionHeader title="Updates" meta={`${jobs.length} jobs`} />
    <div className="compact-list">
      {jobs.length ? (
        jobs.slice(0, 3).map((job) => (
          <div className="compact-row" key={job.id}>
            <JobIcon state={job.state} />
            <div>
              <strong>{job.label}</strong>
              <span>{job.detail}</span>
            </div>
          </div>
        ))
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
  busySourceAction,
  onClearSource,
  onDownloadSource,
  onRemoveTrustedCurator,
}: {
  sources: Source[];
  lessons: Lesson[];
  jobs: Job[];
  runtimeDiagnostics: RuntimeDiagnostics;
  trustedCurators: TrustedCurator[];
  busySourceAction: BusySourceAction;
  onClearSource: (source: Source) => void;
  onDownloadSource: (source: Source) => void;
  onRemoveTrustedCurator: (curator: TrustedCurator) => void;
}) => {
  const { capabilitySources, addedSources } = splitSourceRows(sources);

  return (
    <div className="wide-page">
      <div className="page-heading">
        <div>
          <h1>Source Capability Matrix</h1>
          <p>Platform-level support is separate from playlists, feeds, and channels you add.</p>
        </div>
        <StatusChip label="No credentials in exports" tone="positive" />
      </div>

      <SourceReadinessPanel sources={sources} runtimeDiagnostics={runtimeDiagnostics} />

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
                        downloadBlocked
                          ? "This source type does not currently support downloads."
                          : "Download missing media files into the local library."
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
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <EmptyState
            icon={Rss}
            title="No added sources"
            detail="Use Import to add a playlist, feed, Telegram channel, archive item, video URL, or teacher relay."
          />
        )}
      </section>

      <section className="trusted-curators">
        <SectionHeader title="Trusted Curator Keys" meta={`${trustedCurators.length} stored`} />
        {trustedCurators.length ? (
          <div className="trusted-curator-list">
            {trustedCurators.map((curator) => (
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
  isRefreshingSources,
  onRefreshEnabledSources,
}: {
  jobs: Job[];
  sources: Map<string, Source>;
  isRefreshingSources: boolean;
  onRefreshEnabledSources: () => void;
}) => (
  <div className="wide-page">
    <div className="page-heading">
      <div>
        <h1>Update Queue</h1>
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
    <div className="job-list">
      {jobs.map((job) => (
        <div className="job-row" key={job.id}>
          <JobIcon state={job.state} />
          <div className="job-copy">
            <div>
              <h2>{job.label}</h2>
              <StatusChip label={job.state.replace(/-/g, " ")} tone={jobTone(job.state)} />
            </div>
            <p>{job.detail}</p>
            <span>
              {sources.get(job.sourceId ?? "")?.label ?? "System"} · {formatDate(job.updatedAt)}
            </span>
          </div>
        </div>
      ))}
    </div>
  </div>
);

const ImportDrawer = ({
  isOnlineMode,
  trustedCurators,
  close,
  onResult,
  onTrustCurator,
}: {
  isOnlineMode: boolean;
  trustedCurators: TrustedCurator[];
  close: () => void;
  onResult: (notice: string) => void;
  onTrustCurator: (
    displayName: string,
    publicKey: string,
    trustNote?: string,
  ) => Promise<TrustedCurator>;
}) => {
  const [sourceUrl, setSourceUrl] = useState("");
  const [manifestJson, setManifestJson] = useState("");
  const [validationMessage, setValidationMessage] = useState("");
  const [manifestReport, setManifestReport] = useState<ManifestValidationReport | null>(null);
  const [manualDisplayName, setManualDisplayName] = useState("");
  const [manualPublicKey, setManualPublicKey] = useState("");
  const [manualTrustNote, setManualTrustNote] = useState("");
  const [isWorking, setIsWorking] = useState(false);
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

  const runLocalImport = async () => {
    setIsWorking(true);
    const paths = await chooseLocalMediaPaths();

    if (!isTauriRuntime()) {
      onResult("Local file picking requires the Tauri desktop runtime.");
      setIsWorking(false);
      return;
    }

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

    if (!trimmedUrl) {
      setValidationMessage("Add a public feed, playlist, Telegram channel, or source URL first.");
      return;
    }

    if (!isOnlineMode) {
      setValidationMessage("Switch to online fetch mode before subscribing to remote feeds.");
      return;
    }

    setIsWorking(true);
    setValidationMessage("");

    try {
      const result = await ingestSourceUrl(trimmedUrl);
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

  return (
    <div className="drawer-backdrop" role="presentation">
      <aside className="import-drawer" aria-label="Import content">
        <div className="drawer-header">
          <div>
            <h1>Import Content</h1>
            <p>Add local media, public feeds, Archive items, direct video URLs, or Duroos manifests.</p>
          </div>
          <button type="button" className="icon-button" onClick={close} aria-label="Close import">
            ×
          </button>
        </div>

        <div className="import-options">
          <ImportOption
            icon={FolderOpen}
            title="Add Local Files"
            detail="Copy selected video, audio, or PDF files into the app library."
          />
          <ImportOption
            icon={Globe2}
            title="Subscribe or Ingest"
            detail="Read Archive items, feeds, public Telegram previews, and direct YouTube/Rumble/Odysee URLs."
          />
          <ImportOption
            icon={KeyRound}
            title="Private Source Limits"
            detail="X/Rumble can retry with app-local yt-dlp cookies; private Telegram still needs a later session adapter."
          />
          <ImportOption
            icon={UploadCloud}
            title="Curator Feed"
            detail="Follow signed Duroos manifests when teachers or curators publish them."
          />
          <ImportOption
            icon={FileArchive}
            title="Import Collection"
            detail="Validate a Duroos v2 manifest before importing or sharing."
          />
        </div>

        <label className="field">
          <span>Source URL or identifier</span>
          <input
            value={sourceUrl}
            onChange={(event) => setSourceUrl(event.target.value)}
            placeholder="Archive item, RSS feed, t.me channel, YouTube, Rumble, Odysee, X, or relay URL"
          />
        </label>

        <div className="ingest-hint">
          <Rss size={17} />
          <span>
            Public Telegram channels are tried through the no-login t.me/s preview. Private
            channels, invite-only groups, and restricted media still need a local Telegram session
            or manual export. Archive.org item URLs, feeds, and direct video URLs can become
            reviewable library rows. If X or Rumble blocks anonymous fetches, add
            yt-dlp-cookies.txt in app data or import the downloaded file manually. Duplicate source
            URLs and matching hashes are skipped. Offline mode blocks remote subscription fetches.
          </span>
        </div>

        <div className="drawer-actions">
          <button
            type="button"
            className="secondary-action"
            onClick={runLocalImport}
            disabled={isWorking}
          >
            <Download size={17} />
            <span>Import Local Files</span>
          </button>
          <button
            type="button"
            className="primary-action"
            onClick={runSourceIngest}
            disabled={isWorking || !isOnlineMode}
          >
            <Archive size={17} />
            <span>{isWorking ? "Working" : "Subscribe / Ingest"}</span>
          </button>
        </div>

        <label className="field">
          <span>Collection manifest</span>
          <textarea
            className="manifest-box"
            value={manifestJson}
            onChange={(event) => {
              setManifestJson(event.target.value);
              setManifestReport(null);
            }}
            placeholder='{"schemaVersion":2,"exportedAt":"...","curator":{...},"collection":{...},"lessons":[...]}'
          />
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
            Nostr is modeled for future discovery and study-circle references only. IPFS CIDs and
            BitTorrent magnets can validate as manifest references, but this app downloads only
            HTTP or enclosure URLs.
          </span>
        </div>

        <div className="manual-trust-panel">
          <SectionHeader title="Manual Curator Key" meta="fallback" />
          <label className="field">
            <span>Display name</span>
            <input
              value={manualDisplayName}
              onChange={(event) => setManualDisplayName(event.target.value)}
              placeholder="Curator or teacher name"
            />
          </label>
          <label className="field">
            <span>Ed25519 public key</span>
            <input
              value={manualPublicKey}
              onChange={(event) => setManualPublicKey(event.target.value)}
              placeholder="Hex or base64 public key"
            />
          </label>
          <label className="field">
            <span>Trust note</span>
            <input
              value={manualTrustNote}
              onChange={(event) => setManualTrustNote(event.target.value)}
              placeholder="Where you verified this key"
            />
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
        {validationMessage ? <p className="validation-message">{validationMessage}</p> : null}
      </aside>
    </div>
  );
};

const ImportOption = ({
  icon: Icon,
  title,
  detail,
}: {
  icon: typeof FolderOpen;
  title: string;
  detail: string;
}) => (
  <div className="import-option">
    <Icon size={19} />
    <div>
      <strong>{title}</strong>
      <span>{detail}</span>
    </div>
  </div>
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
