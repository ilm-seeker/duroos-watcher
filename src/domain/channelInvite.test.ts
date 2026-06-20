import { describe, expect, it } from "vitest";
import {
  buildChannelInvite,
  buildPublisherChannelInvite,
  canonicalizeChannelRef,
  extractChannelRef,
  verificationCodeFromManifestHash,
} from "./channelInvite";
import type { ChannelPublishResult, PublisherChannel } from "./types";

const naddr =
  "naddr1qqnkgatjdahhxttrdpskumn9dsaxx6rpdehx2mpd8quxzcf3x9skyerx8yungdrzxuq3gamnwvaz7tmjv4kxz7fwv3sk6atn9e5k7qgdwaehxw309ahx7uewd3hkcqgkwaehxw309aex2mrp0yh8qunfd4skctnwv46qyg8c24fgxfu4d5d7lgexh6gjyqlvxqlwc4vwc3rutjwwewzs6gx9f5psgqqqw4lqq7sz59";

const publishResult = (): ChannelPublishResult => ({
  channelId: "channel-test",
  channelTitle: "Foundations",
  naddr,
  canonicalChannelLink: `nostr:${naddr}`,
  inviteText: [
    "Duroos channel invite",
    "Channel: Foundations",
    "Teacher: Example Teacher",
    `Open in Duroos Watcher: nostr:${naddr}`,
    "Manifest: sha256:83829c50baca669812884d16505873dd9d7318c8ab88e9630c9bfcd1d970570b",
    "Check code: DW-8382-9C50-BACA",
    "Preview before trusting this teacher key.",
  ].join("\n"),
  verificationCode: "DW-8382-9C50-BACA",
  manifestJson: "{}",
  manifestSha256: "sha256:83829c50baca669812884d16505873dd9d7318c8ab88e9630c9bfcd1d970570b",
  manifestUrl:
    "https://blossom.example/83829c50baca669812884d16505873dd9d7318c8ab88e9630c9bfcd1d970570b.json",
  nostrEventId: "event-test",
  blossomResults: [],
  archiveResults: [],
  relayResults: [],
  mediaCount: 1,
  postCount: 0,
  totalItemCount: 1,
  messages: [],
});

const publisherChannel = (patch: Partial<PublisherChannel> = {}): PublisherChannel => ({
  id: "channel-test",
  profileId: "publisher-test",
  title: "Foundations",
  description: "A saved teacher channel.",
  channelIdentifier: "duroos-channel:channel-test",
  naddr,
  canonicalChannelLink: `nostr:${naddr}`,
  lastManifestSha256:
    "sha256:83829c50baca669812884d16505873dd9d7318c8ab88e9630c9bfcd1d970570b",
  lastManifestUrl:
    "https://blossom.example/83829c50baca669812884d16505873dd9d7318c8ab88e9630c9bfcd1d970570b.json",
  lastPublishedAt: "2026-06-20T18:00:00.000Z",
  mediaCount: 1,
  postCount: 1,
  createdAt: "2026-06-20T17:00:00.000Z",
  updatedAt: "2026-06-20T18:00:00.000Z",
  ...patch,
});

describe("channel invite helpers", () => {
  it("extracts raw naddr refs from raw links, nostr URIs, and full invite text", () => {
    expect(extractChannelRef(naddr)).toBe(naddr);
    expect(extractChannelRef(`nostr:${naddr}`)).toBe(naddr);
    expect(
      extractChannelRef(`Duroos channel invite\nOpen in Duroos Watcher: nostr:${naddr}.`),
    ).toBe(naddr);
  });

  it("canonicalizes channel refs to NIP-21 nostr URIs", () => {
    expect(canonicalizeChannelRef(naddr)).toBe(`nostr:${naddr}`);
    expect(canonicalizeChannelRef(`Open ${naddr}`)).toBe(`nostr:${naddr}`);
  });

  it("rejects non-channel Nostr entities and unrelated text", () => {
    expect(extractChannelRef("nostr:npub1abc")).toBeNull();
    expect(extractChannelRef("note1abc")).toBeNull();
    expect(extractChannelRef("https://example.com/feed.xml")).toBeNull();
  });

  it("builds stable non-secret verification codes from manifest hashes", () => {
    expect(
      verificationCodeFromManifestHash(
        "sha256:83829c50baca669812884d16505873dd9d7318c8ab88e9630c9bfcd1d970570b",
      ),
    ).toBe("DW-8382-9C50-BACA");
    expect(verificationCodeFromManifestHash("not-a-hash")).toBe("DW-UNVERIFIED");
  });

  it("preserves backend invite metadata when building the share payload", () => {
    const invite = buildChannelInvite(publishResult());

    expect(invite.rawNaddr).toBe(naddr);
    expect(invite.canonicalChannelLink).toBe(`nostr:${naddr}`);
    expect(invite.verificationCode).toBe("DW-8382-9C50-BACA");
    expect(invite.inviteText).toContain("Channel: Foundations");
  });

  it("builds copyable invites from persisted publisher channels", () => {
    const invite = buildPublisherChannelInvite(publisherChannel());

    expect(invite?.rawNaddr).toBe(naddr);
    expect(invite?.canonicalChannelLink).toBe(`nostr:${naddr}`);
    expect(invite?.verificationCode).toBe("DW-8382-9C50-BACA");
    expect(invite?.inviteText).toContain("Channel: Foundations");
    expect(invite?.inviteText).toContain("Preview before trusting this teacher key.");
  });

  it("falls back to an unverified check code when persisted manifest hash is missing", () => {
    const invite = buildPublisherChannelInvite(
      publisherChannel({ lastManifestSha256: undefined }),
    );

    expect(invite?.verificationCode).toBe("DW-UNVERIFIED");
    expect(invite?.inviteText).toContain("Manifest: unavailable");
  });

  it("does not build share payloads for saved channels without subscriber links", () => {
    expect(
      buildPublisherChannelInvite(
        publisherChannel({ canonicalChannelLink: undefined, naddr: undefined }),
      ),
    ).toBeNull();
  });
});
