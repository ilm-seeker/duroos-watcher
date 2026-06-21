import type { ChannelPublishResult, PublisherChannel } from "./types";

const NADDR_PATTERN = /(?:nostr:)?(naddr1[023456789acdefghjklmnpqrstuvwxyz]+)/i;
const SHA256_PATTERN = /^[a-f0-9]{64}$/i;

export interface ChannelInvite {
  canonicalChannelLink: string;
  inviteText: string;
  rawNaddr: string;
  verificationCode: string;
  manifestUrls: string[];
  relays: string[];
  blossomServers: string[];
  archiveMirrors: string[];
  curatorPublicKeyFingerprint?: string;
}

export const extractChannelRef = (input: string): string | null => {
  const match = input.match(NADDR_PATTERN);
  return match?.[1]?.toLowerCase() ?? null;
};

export const canonicalizeChannelRef = (input: string): string | null => {
  const rawNaddr = extractChannelRef(input);
  return rawNaddr ? `nostr:${rawNaddr}` : null;
};

export const verificationCodeFromManifestHash = (manifestSha256: string): string => {
  const hex = manifestSha256.trim().replace(/^sha256:/i, "");

  if (!SHA256_PATTERN.test(hex)) {
    return "DW-UNVERIFIED";
  }

  const prefix = hex.slice(0, 12).toUpperCase();
  return `DW-${prefix.slice(0, 4)}-${prefix.slice(4, 8)}-${prefix.slice(8, 12)}`;
};

const uniqueNonEmpty = (values: Array<string | undefined>): string[] => {
  const output: string[] = [];
  for (const value of values) {
    const trimmed = value?.trim();
    if (!trimmed || output.includes(trimmed)) {
      continue;
    }
    output.push(trimmed);
  }
  return output;
};

const inviteListLine = (label: string, values: string[]): string | null =>
  values.length ? `${label}: ${values.join(", ")}` : null;

export const buildChannelInvite = (result: ChannelPublishResult): ChannelInvite => {
  const rawNaddr = extractChannelRef(result.naddr) ?? result.naddr.trim();
  const canonicalChannelLink =
    result.canonicalChannelLink || canonicalizeChannelRef(rawNaddr) || rawNaddr;
  const verificationCode =
    result.verificationCode || verificationCodeFromManifestHash(result.manifestSha256);
  const manifestUrls = uniqueNonEmpty([...(result.manifestUrls ?? []), result.manifestUrl]);
  const relays = uniqueNonEmpty(result.relays ?? []);
  const blossomServers = uniqueNonEmpty(result.blossomServers ?? []);
  const archiveMirrors = uniqueNonEmpty(result.archiveMirrors ?? []);
  const inviteText =
    result.inviteText ||
    [
      "Duroos channel invite",
      `Channel: ${result.channelTitle}`,
      `Open in Duroos Watcher: ${canonicalChannelLink}`,
      `Manifest: ${result.manifestSha256}`,
      `Check code: ${verificationCode}`,
      result.curatorPublicKeyFingerprint
        ? `Curator public-key fingerprint: ${result.curatorPublicKeyFingerprint}`
        : null,
      inviteListLine("Relays", relays),
      inviteListLine("Manifest URLs", manifestUrls),
      inviteListLine("Blossom servers", blossomServers),
      inviteListLine("Archive mirrors", archiveMirrors),
      "Preview before trusting this teacher key.",
    ]
      .filter((line): line is string => Boolean(line))
      .join("\n");

  return {
    canonicalChannelLink,
    inviteText,
    rawNaddr,
    verificationCode,
    manifestUrls,
    relays,
    blossomServers,
    archiveMirrors,
    curatorPublicKeyFingerprint: result.curatorPublicKeyFingerprint,
  };
};

export const buildPublisherChannelInvite = (
  channel: PublisherChannel,
): ChannelInvite | null => {
  const rawNaddr = extractChannelRef(channel.naddr ?? channel.canonicalChannelLink ?? "");
  const canonicalChannelLink =
    channel.canonicalChannelLink ||
    (rawNaddr ? canonicalizeChannelRef(rawNaddr) : null);

  if (!canonicalChannelLink) {
    return null;
  }

  const manifestSha256 = channel.lastManifestSha256 ?? "";
  const verificationCode = verificationCodeFromManifestHash(manifestSha256);
  const manifestUrls = uniqueNonEmpty([channel.lastManifestUrl]);
  const inviteText = [
    "Duroos channel invite",
    `Channel: ${channel.title}`,
    `Open in Duroos Watcher: ${canonicalChannelLink}`,
    `Manifest: ${manifestSha256 || "unavailable"}`,
    `Check code: ${verificationCode}`,
    inviteListLine("Manifest URLs", manifestUrls),
    "Preview before trusting this teacher key.",
  ]
    .filter((line): line is string => Boolean(line))
    .join("\n");

  return {
    canonicalChannelLink,
    inviteText,
    rawNaddr: rawNaddr ?? canonicalChannelLink,
    verificationCode,
    manifestUrls,
    relays: [],
    blossomServers: [],
    archiveMirrors: [],
  };
};
