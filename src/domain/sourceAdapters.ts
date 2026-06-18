import type {
  AuthMode,
  CapabilityLevel,
  SourceCapability,
  SourcePlatform,
} from "./types";

export interface SourceAdapterDescriptor {
  platform: SourcePlatform;
  label: string;
  description: string;
  capability: SourceCapability;
  acceptedInputs: string[];
  privacyBoundary: string;
}

const capability = (
  metadata: CapabilityLevel,
  download: CapabilityLevel,
  autoUpdate: CapabilityLevel,
  authRequired: boolean,
  authMode: AuthMode,
  note: string,
  reliability: SourceCapability["reliability"] = "good",
): SourceCapability => ({
  metadata,
  download,
  autoUpdate,
  authRequired,
  authMode,
  note,
  reliability,
});

export const sourceAdapters: SourceAdapterDescriptor[] = [
  {
    platform: "local-files",
    label: "Local Files",
    description: "Import owned video, audio, and PDF study files into the app library.",
    capability: capability(
      "native",
      "native",
      "blocked",
      false,
      "none",
      "Imports video, audio, and PDF files the user selects and records local provenance.",
      "stable",
    ),
    acceptedInputs: ["Video files", "Audio files", "PDF files", "Folders"],
    privacyBoundary: "Files stay in the local app library.",
  },
  {
    platform: "telegram",
    label: "Telegram",
    description: "Follow channels the user can access and import released lessons or teacher posts.",
    capability: capability(
      "supported",
      "limited",
      "supported",
      false,
      "none",
      "Public channel previews can be read without sign-in; private channels still require a local session.",
      "best-effort",
    ),
    acceptedInputs: ["Public channel URL", "Private channel URL", "Message links", "Teacher posts"],
    privacyBoundary: "Telegram session state must remain in local protected storage.",
  },
  {
    platform: "rss-feed",
    label: "RSS/Atom/JSON Feed",
    description: "Subscribe to custom user-selected feeds that publish study media and posts.",
    capability: capability(
      "supported",
      "supported",
      "supported",
      false,
      "none",
      "RSS, Atom, and JSON Feed subscriptions can ingest videos, audio, PDFs, and teacher message posts.",
      "stable",
    ),
    acceptedInputs: ["RSS feed URL", "Atom feed URL", "JSON Feed URL", "Podcast feed", "Teacher post feed"],
    privacyBoundary: "Feed subscriptions store public URLs only by default.",
  },
  {
    platform: "archive-org",
    label: "Archive.org",
    description: "Read item metadata and downloadable files from the Internet Archive.",
    capability: capability(
      "supported",
      "supported",
      "supported",
      false,
      "none",
      "Uses Archive.org item metadata and file listings.",
      "stable",
    ),
    acceptedInputs: ["Item URL", "Collection URL", "Identifier", "PDF files"],
    privacyBoundary: "Only public metadata and files are fetched.",
  },
  {
    platform: "youtube",
    label: "YouTube",
    description: "Track video channels and playlists through official APIs or RSS where available.",
    capability: capability(
      "supported",
      "limited",
      "limited",
      true,
      "api-key",
      "Official API covers metadata. Download support depends on permitted content and local tools.",
      "best-effort",
    ),
    acceptedInputs: ["Playlist URL", "Channel URL", "Video URL"],
    privacyBoundary: "API keys and cookies must not be exported.",
  },
  {
    platform: "x",
    label: "X",
    description: "Capture source references, teacher posts, and metadata where API access allows.",
    capability: capability(
      "limited",
      "limited",
      "limited",
      true,
      "api-key",
      "API access is credential-bound and platform-constrained.",
      "credential-bound",
    ),
    acceptedInputs: ["Post URL", "Profile URL", "Teacher messages"],
    privacyBoundary: "Tokens remain local and are not included in collection files.",
  },
  {
    platform: "rumble",
    label: "Rumble",
    description: "Best-effort URL import for permitted media and metadata.",
    capability: capability(
      "limited",
      "limited",
      "limited",
      false,
      "none",
      "No broad public catalog API is assumed; URL extraction is best-effort.",
      "best-effort",
    ),
    acceptedInputs: ["Video URL", "Channel URL"],
    privacyBoundary: "Imports store only source URLs and provenance records.",
  },
  {
    platform: "odysee",
    label: "Odysee",
    description: "Best-effort import for Odysee/LBRY video links and local protocol references.",
    capability: capability(
      "limited",
      "limited",
      "limited",
      false,
      "none",
      "No broad native catalog adapter is assumed; user-initiated URLs can use local tooling where supported.",
      "best-effort",
    ),
    acceptedInputs: ["Odysee URL", "LBRY URL", "Video URL"],
    privacyBoundary: "LBRY daemon state and wallet data must stay outside shared collection exports.",
  },
  {
    platform: "teacher-relay",
    label: "Channels",
    description: "Subscribe to teacher or curator-owned signed feeds and shared Nostr channel links.",
    capability: capability(
      "native",
      "supported",
      "supported",
      false,
      "none",
      "Signed curator feeds and Nostr channel pointers publish videos, audio, PDFs, teacher posts, hashes, and provenance.",
      "stable",
    ),
    acceptedInputs: [
      "Teacher channel URL",
      "Signed Duroos manifest",
      "Nostr naddr channel link",
      "JSON Feed URL",
      "Media enclosure feed",
      "Teacher posts",
    ],
    privacyBoundary: "Feed subscriptions store no account credentials by default.",
  },
];

export const adapterByPlatform = new Map(
  sourceAdapters.map((adapter) => [adapter.platform, adapter]),
);

export const getAdapter = (platform: SourcePlatform): SourceAdapterDescriptor => {
  const adapter = adapterByPlatform.get(platform);

  if (!adapter) {
    throw new Error(`Unsupported source platform: ${platform}`);
  }

  return adapter;
};

export const isDownloadPermitted = (
  platform: SourcePlatform,
): boolean => {
  const adapter = getAdapter(platform);

  return adapter.capability.download !== "blocked";
};
