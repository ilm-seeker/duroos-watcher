import { describe, expect, it } from "vitest";
import {
  MANIFEST_SCHEMA_VERSION,
  parseCollectionManifest,
  sanitizeManifestForExport,
  type SharedCollectionManifest,
} from "./collectionManifest";

const validManifest = (): SharedCollectionManifest => ({
  schemaVersion: MANIFEST_SCHEMA_VERSION,
  exportedAt: "2026-06-16T05:00:00.000Z",
  curator: {
    id: "curator-foundations",
    displayName: "Foundations Curator",
    publicKey: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  },
  collection: {
    title: "Foundations Class",
    ownerLabel: "Local curation",
  },
  lessons: [
    {
      title: "Opening lesson",
      contentType: "video",
      sourceRefs: [
        {
          platform: "telegram",
          originUrl: "https://t.me/example/12",
        },
      ],
      retrievalRefs: [
        {
          kind: "enclosure-url",
          url: "https://example.org/opening-lesson.mp4",
          mediaType: "video/mp4",
        },
        {
          kind: "ipfs-cid",
          cid: "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
          mediaType: "video/mp4",
        },
      ],
      contentHashes: [
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      ],
      provenance: {
        adapterName: "TelegramAdapter",
      },
    },
  ],
});

describe("parseCollectionManifest", () => {
  it("accepts a safe shared collection manifest", () => {
    const report = parseCollectionManifest(JSON.stringify(validManifest()));

    expect(report.valid).toBe(true);
    expect(report.errors).toEqual([]);
  });

  it("allows description fields without treating them as script fields", () => {
    const manifest = {
      ...validManifest(),
      collection: {
        ...validManifest().collection,
        description: "Weekly lessons from the teacher.",
      },
      lessons: [
        {
          ...validManifest().lessons[0],
          description: "Recorded class notes.",
        },
      ],
    };

    const report = parseCollectionManifest(manifest);

    expect(report.valid).toBe(true);
    expect(report.errors).toEqual([]);
  });

  it("accepts Nostr publication metadata and Blossom retrieval refs", () => {
    const manifest = validManifest();
    manifest.curator = {
      ...manifest.curator!,
      nostrPubkey: "a".repeat(64),
    };
    manifest.publication = {
      transport: "nostr",
      naddr: "naddr1example",
      relays: ["wss://relay.example"],
      blossomServers: ["https://blossom.example"],
      manifestSha256:
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      publishedAt: "2026-06-16T05:00:00.000Z",
  };
  manifest.lessons[0].retrievalRefs = [
    {
      kind: "direct-url",
      url: "https://blossom.example/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.mp4",
        service: "blossom",
        sha256:
          "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        sizeBytes: 2048,
      mimeType: "video/mp4",
      mediaType: "video/mp4",
    },
    {
      kind: "direct-url",
      url: "https://blossom-two.example/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.mp4",
      service: "blossom",
      sha256:
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      sizeBytes: 2048,
      mimeType: "video/mp4",
      mediaType: "video/mp4",
    },
    {
      kind: "ipfs-cid",
      cid: "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
      gatewayUrl: "https://gateway.example/ipfs",
      sha256:
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      sizeBytes: 2048,
      mimeType: "video/mp4",
      mediaType: "video/mp4",
    },
  ];

    const report = parseCollectionManifest(manifest);

    expect(report.valid).toBe(true);
    expect(report.errors).toEqual([]);
  });

  it("accepts lbry source references without treating them as retrieval URLs", () => {
    const manifest = validManifest();
    manifest.lessons[0].sourceRefs[0] = {
      platform: "odysee",
      originUrl: "lbry://@teacher/class-1",
    };

    const report = parseCollectionManifest(manifest);

    expect(report.valid).toBe(true);
    expect(report.errors).toEqual([]);
  });

  it("rejects v2 manifests without curator identity", () => {
    const unsafe = validManifest();
    delete unsafe.curator;

    const report = parseCollectionManifest(unsafe);

    expect(report.valid).toBe(false);
    expect(report.errors.join(" ")).toContain("curator");
  });

  it("rejects credentials and local absolute paths", () => {
    const unsafe = {
      ...validManifest(),
      telegramSession: "secret-session",
      lessons: [
        {
          ...validManifest().lessons[0],
          localPath: "/Users/example/private/video.mp4",
        },
      ],
    };

    const report = parseCollectionManifest(unsafe);

    expect(report.valid).toBe(false);
    expect(report.errors.join(" ")).toContain("telegramSession");
    expect(report.errors.join(" ")).toContain("localPath");
  });

  it("rejects non-source URLs", () => {
    const unsafe = validManifest();
    unsafe.lessons[0].sourceRefs[0].originUrl = "file:///Users/example/video.mp4";

    const report = parseCollectionManifest(unsafe);

    expect(report.valid).toBe(false);
    expect(report.errors.join(" ")).toContain("originUrl");
  });

  it("rejects unknown content types", () => {
    const unsafe = validManifest();
    unsafe.lessons[0].contentType = "spreadsheet" as "video";

    const report = parseCollectionManifest(unsafe);

    expect(report.valid).toBe(false);
    expect(report.errors.join(" ")).toContain("contentType");
  });

  it("rejects unsafe retrieval refs", () => {
    const unsafe = validManifest();
    unsafe.lessons[0].retrievalRefs = [
      {
        kind: "direct-url",
        url: "file:///Users/example/private.mp4",
      },
    ];

    const report = parseCollectionManifest(unsafe);

    expect(report.valid).toBe(false);
    expect(report.errors.join(" ")).toContain("retrievalRefs");
  });

  it("rejects invalid Nostr publication metadata", () => {
    const unsafe = validManifest();
    unsafe.curator = {
      ...unsafe.curator!,
      nostrPubkey: "not-a-hex-key",
    };
    unsafe.publication = {
      transport: "nostr",
      naddr: "note1wrong",
      relays: ["https://not-a-relay.example"],
      blossomServers: ["file:///tmp/blossom"],
      manifestSha256: "sha256:not-a-hash",
      publishedAt: "not-a-date",
    };

    const report = parseCollectionManifest(unsafe);

    expect(report.valid).toBe(false);
    expect(report.errors.join(" ")).toContain("nostrPubkey");
    expect(report.errors.join(" ")).toContain("publication.relays");
    expect(report.errors.join(" ")).toContain("publication.blossomServers");
  });

  it("marks well-shaped signatures as signed but untrusted in browser validation", () => {
    const signed = {
      ...validManifest(),
      signature: {
        algorithm: "ed25519" as const,
        publicKey: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        value:
          "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      },
    };

    const report = parseCollectionManifest(signed);

    expect(report.valid).toBe(true);
    expect(report.trustState).toBe("signed-untrusted");
    expect(report.curator).toEqual(validManifest().curator);
    expect(report.trustedCuratorId).toBeUndefined();
  });

  it("does not treat base64 public keys as local paths", () => {
    const publicKey = "//////////////////////////////////////////8=";
    const signature =
      "/////////////////////////////////////////////////////////////////////////////////////w==";
    const signed = {
      ...validManifest(),
      curator: {
        ...validManifest().curator,
        publicKey,
      },
      signature: {
        algorithm: "ed25519" as const,
        publicKey,
        value: signature,
      },
    };

    const report = parseCollectionManifest(signed);

    expect(report.valid).toBe(true);
    expect(report.errors.join(" ")).not.toContain("absolute or file URL path");
  });
});

describe("sanitizeManifestForExport", () => {
  it("removes forbidden keys before export", () => {
    const manifest = {
      ...validManifest(),
      accessToken: "not-for-export",
    } as SharedCollectionManifest & { accessToken: string };

    const sanitized = sanitizeManifestForExport(manifest);

    expect("accessToken" in sanitized).toBe(false);
    expect(parseCollectionManifest(sanitized).valid).toBe(true);
  });
});
