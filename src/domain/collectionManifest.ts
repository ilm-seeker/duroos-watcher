import type { TrustState } from "./types";

export const MANIFEST_SCHEMA_VERSION = 2;

const FORBIDDEN_KEY_TOKENS = new Set([
  "credential",
  "credentials",
  "token",
  "cookie",
  "cookies",
  "session",
  "secret",
  "password",
  "command",
  "script",
  "hook",
]);
const FORBIDDEN_NORMALIZED_KEYS = new Set([
  "accesstoken",
  "refreshtoken",
  "telegramsession",
  "apikey",
  "privatekey",
  "localpath",
  "absolutepath",
]);

const ABSOLUTE_PATH_PATTERN = /^(\/|~\/|[a-zA-Z]:[\\/]|file:\/\/)/;

export interface SharedCollectionManifest {
  schemaVersion: 1 | 2;
  exportedAt: string;
  curator?: SharedCuratorIdentity;
  publication?: SharedPublicationRef;
  collection: {
    title: string;
    ownerLabel: string;
    description?: string;
  };
  lessons: SharedLessonRef[];
  signature?: {
    algorithm: "ed25519";
    publicKey: string;
    value: string;
  };
}

export interface SharedCuratorIdentity {
  id: string;
  displayName: string;
  publicKey: string;
  nostrPubkey?: string;
}

export interface SharedPublicationRef {
  transport: "nostr";
  naddr: string;
  relays: string[];
  blossomServers: string[];
  manifestSha256: string;
  publishedAt: string;
}

export interface SharedLessonRef {
  title: string;
  contentType?: "video" | "audio" | "pdf" | "post";
  sourceRefs: {
    platform: string;
    originUrl: string;
    publishedAt?: string;
  }[];
  retrievalRefs?: SharedRetrievalRef[];
  contentHashes: string[];
  provenance: {
    permissionNote?: string;
    adapterName: string;
    importedAt?: string;
  };
  durationSeconds?: number;
  description?: string;
}

export type SharedRetrievalRef =
  | {
      kind: "direct-url" | "enclosure-url";
      url: string;
      mediaType?: string;
      service?: "blossom";
      sha256?: string;
      sizeBytes?: number;
      mimeType?: string;
    }
  | {
      kind: "ipfs-cid";
      cid: string;
      gatewayUrl?: string;
      sha256?: string;
      sizeBytes?: number;
      mimeType?: string;
      mediaType?: string;
    }
  | {
      kind: "magnet";
      magnetUri: string;
      mediaType?: string;
    };

export interface ManifestValidationReport {
  valid: boolean;
  errors: string[];
  warnings: string[];
  trustState?: TrustState;
  curator?: SharedCuratorIdentity;
  trustedCuratorId?: string;
  manifest?: SharedCollectionManifest;
}

const isPlainObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const isSafeRelativePath = (value: string): boolean => {
  if (ABSOLUTE_PATH_PATTERN.test(value)) {
    return false;
  }

  return !value.split(/[\\/]/).some((segment) => segment === "..");
};

const isEncodedCryptoPath = (path: string): boolean =>
  path.endsWith(".publicKey") || path.endsWith(".signature.value");

const isForbiddenManifestKey = (key: string): boolean => {
  const tokens = manifestKeyTokens(key);
  const normalized = tokens.join("");
  return (
    FORBIDDEN_NORMALIZED_KEYS.has(normalized) ||
    tokens.some((token) => FORBIDDEN_KEY_TOKENS.has(token))
  );
};

const manifestKeyTokens = (key: string): string[] =>
  key
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/[^a-zA-Z0-9]+/g, " ")
    .split(" ")
    .map((token) => token.trim().toLowerCase())
    .filter(Boolean);

const walkForUnsafeValues = (
  value: unknown,
  path: string,
  errors: string[],
): void => {
  if (Array.isArray(value)) {
    value.forEach((item, index) => walkForUnsafeValues(item, `${path}[${index}]`, errors));
    return;
  }

  if (!isPlainObject(value)) {
    if (
      typeof value === "string" &&
      !isEncodedCryptoPath(path) &&
      ABSOLUTE_PATH_PATTERN.test(value)
    ) {
      errors.push(`${path} contains an absolute or file URL path`);
    }
    return;
  }

  Object.entries(value).forEach(([key, nested]) => {
    const nestedPath = path ? `${path}.${key}` : key;

    if (isForbiddenManifestKey(key)) {
      errors.push(`${nestedPath} is not allowed in shared collection manifests`);
    }

    walkForUnsafeValues(nested, nestedPath, errors);
  });
};

const isValidUrl = (value: string): boolean => {
  try {
    const url = new URL(value);
    return ["http:", "https:", "tg:", "telegram:", "lbry:"].includes(url.protocol);
  } catch {
    return false;
  }
};

const isValidHttpUrl = (value: string): boolean => {
  try {
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol);
  } catch {
    return false;
  }
};

const looksLikeHash = (value: string): boolean =>
  /^[a-f0-9]{64}$/i.test(value) || /^sha256:[a-f0-9]{64}$/i.test(value);

const looksLikePublicKey = (value: string): boolean =>
  /^[a-f0-9]{64}$/i.test(value) || /^[A-Za-z0-9+/=_-]{32,}$/.test(value);

const looksLikeSignature = (value: string): boolean =>
  /^[a-f0-9]{128}$/i.test(value) || /^[A-Za-z0-9+/=_-]{64,}$/.test(value);

const looksLikeNostrPubkey = (value: string): boolean => /^[a-f0-9]{64}$/i.test(value);

const looksLikeIpfsCid = (value: string): boolean =>
  /^(Qm[1-9A-HJ-NP-Za-km-z]{44}|b[a-z2-7]{20,})$/i.test(value);

const isValidWsUrl = (value: string): boolean => {
  try {
    const url = new URL(value);
    return ["ws:", "wss:"].includes(url.protocol);
  } catch {
    return false;
  }
};

export const parseCollectionManifest = (
  input: string | unknown,
): ManifestValidationReport => {
  const errors: string[] = [];
  const warnings: string[] = [];
  let parsed: unknown = input;

  if (typeof input === "string") {
    try {
      parsed = JSON.parse(input);
    } catch {
      return {
        valid: false,
        errors: ["Manifest is not valid JSON"],
        warnings,
      };
    }
  }

  walkForUnsafeValues(parsed, "manifest", errors);

  if (!isPlainObject(parsed)) {
    errors.push("Manifest root must be an object");
    return { valid: false, errors, warnings };
  }

  if (![1, MANIFEST_SCHEMA_VERSION].includes(Number(parsed.schemaVersion))) {
    errors.push(`schemaVersion must be 1 or ${MANIFEST_SCHEMA_VERSION}`);
  }

  if (typeof parsed.exportedAt !== "string" || Number.isNaN(Date.parse(parsed.exportedAt))) {
    errors.push("exportedAt must be an ISO date string");
  }

  if (parsed.schemaVersion === MANIFEST_SCHEMA_VERSION) {
    if (!isPlainObject(parsed.curator)) {
      errors.push("curator must be an object for schemaVersion 2");
    } else {
      if (typeof parsed.curator.id !== "string" || parsed.curator.id.trim() === "") {
        errors.push("curator.id is required");
      }

      if (
        typeof parsed.curator.displayName !== "string" ||
        parsed.curator.displayName.trim() === ""
      ) {
        errors.push("curator.displayName is required");
      }

      if (
        typeof parsed.curator.publicKey !== "string" ||
        !looksLikePublicKey(parsed.curator.publicKey)
      ) {
        errors.push("curator.publicKey must be an Ed25519 public key");
      }

      if (
        "nostrPubkey" in parsed.curator &&
        (typeof parsed.curator.nostrPubkey !== "string" ||
          !looksLikeNostrPubkey(parsed.curator.nostrPubkey))
      ) {
        errors.push("curator.nostrPubkey must be a 32-byte hex Nostr public key");
      }
    }
  }

  if ("publication" in parsed) {
    if (!isPlainObject(parsed.publication)) {
      errors.push("publication must be an object");
    } else {
      if (parsed.publication.transport !== "nostr") {
        errors.push("publication.transport must be nostr");
      }

      if (
        typeof parsed.publication.naddr !== "string" ||
        !parsed.publication.naddr.startsWith("naddr")
      ) {
        errors.push("publication.naddr is required");
      }

      if (
        !Array.isArray(parsed.publication.relays) ||
        parsed.publication.relays.length === 0 ||
        !parsed.publication.relays.every(
          (relay): relay is string => typeof relay === "string" && isValidWsUrl(relay),
        )
      ) {
        errors.push("publication.relays must contain websocket relay URLs");
      }

      if (
        !Array.isArray(parsed.publication.blossomServers) ||
        parsed.publication.blossomServers.length === 0 ||
        !parsed.publication.blossomServers.every(
          (server): server is string => typeof server === "string" && isValidHttpUrl(server),
        )
      ) {
        errors.push("publication.blossomServers must contain http or https URLs");
      }

      if (
        typeof parsed.publication.manifestSha256 !== "string" ||
        !looksLikeHash(parsed.publication.manifestSha256)
      ) {
        errors.push("publication.manifestSha256 must be a sha256 hash");
      }

      if (
        typeof parsed.publication.publishedAt !== "string" ||
        Number.isNaN(Date.parse(parsed.publication.publishedAt))
      ) {
        errors.push("publication.publishedAt must be an ISO date string");
      }
    }
  }

  if (!isPlainObject(parsed.collection)) {
    errors.push("collection must be an object");
  } else {
    if (typeof parsed.collection.title !== "string" || parsed.collection.title.trim() === "") {
      errors.push("collection.title is required");
    }

    if (
      typeof parsed.collection.ownerLabel !== "string" ||
      parsed.collection.ownerLabel.trim() === ""
    ) {
      errors.push("collection.ownerLabel is required");
    }
  }

  if (!Array.isArray(parsed.lessons) || parsed.lessons.length === 0) {
    errors.push("lessons must contain at least one lesson");
  } else {
    parsed.lessons.forEach((lesson, index) => {
      const prefix = `lessons[${index}]`;

      if (!isPlainObject(lesson)) {
        errors.push(`${prefix} must be an object`);
        return;
      }

      if (typeof lesson.title !== "string" || lesson.title.trim() === "") {
        errors.push(`${prefix}.title is required`);
      }

      if (
        "contentType" in lesson &&
        !["video", "audio", "pdf", "post"].includes(String(lesson.contentType))
      ) {
        errors.push(`${prefix}.contentType must be video, audio, pdf, or post`);
      }

      if (!Array.isArray(lesson.sourceRefs) || lesson.sourceRefs.length === 0) {
        errors.push(`${prefix}.sourceRefs must contain at least one source`);
      } else {
        lesson.sourceRefs.forEach((sourceRef, sourceIndex) => {
          const sourcePrefix = `${prefix}.sourceRefs[${sourceIndex}]`;

          if (!isPlainObject(sourceRef)) {
            errors.push(`${sourcePrefix} must be an object`);
            return;
          }

          if (
            typeof sourceRef.platform !== "string" ||
            sourceRef.platform.trim() === ""
          ) {
            errors.push(`${sourcePrefix}.platform is required`);
          }

          if (
            typeof sourceRef.originUrl !== "string" ||
            !isValidUrl(sourceRef.originUrl)
          ) {
            errors.push(`${sourcePrefix}.originUrl must be a safe source URL`);
          }
        });
      }

      if ("retrievalRefs" in lesson) {
        if (!Array.isArray(lesson.retrievalRefs)) {
          errors.push(`${prefix}.retrievalRefs must be an array`);
        } else {
          lesson.retrievalRefs.forEach((retrievalRef, refIndex) => {
            const refPrefix = `${prefix}.retrievalRefs[${refIndex}]`;

            if (!isPlainObject(retrievalRef)) {
              errors.push(`${refPrefix} must be an object`);
              return;
            }

            if (
              !["direct-url", "enclosure-url", "ipfs-cid", "magnet"].includes(
                String(retrievalRef.kind),
              )
            ) {
              errors.push(`${refPrefix}.kind is not supported`);
              return;
            }

            if (
              (retrievalRef.kind === "direct-url" || retrievalRef.kind === "enclosure-url") &&
              (typeof retrievalRef.url !== "string" || !isValidHttpUrl(retrievalRef.url))
            ) {
              errors.push(`${refPrefix}.url must be an http or https URL`);
            }

            if (
              (retrievalRef.kind === "direct-url" || retrievalRef.kind === "enclosure-url") &&
              "service" in retrievalRef &&
              retrievalRef.service !== "blossom"
            ) {
              errors.push(`${refPrefix}.service is not supported`);
            }

            if (
              (retrievalRef.kind === "direct-url" || retrievalRef.kind === "enclosure-url") &&
              "sha256" in retrievalRef &&
              (typeof retrievalRef.sha256 !== "string" || !looksLikeHash(retrievalRef.sha256))
            ) {
              errors.push(`${refPrefix}.sha256 must be a sha256 hash`);
            }

            if (
              (retrievalRef.kind === "direct-url" || retrievalRef.kind === "enclosure-url") &&
              "sizeBytes" in retrievalRef &&
              (typeof retrievalRef.sizeBytes !== "number" || retrievalRef.sizeBytes <= 0)
            ) {
              errors.push(`${refPrefix}.sizeBytes must be positive`);
            }

            if (
              (retrievalRef.kind === "direct-url" || retrievalRef.kind === "enclosure-url") &&
              "mimeType" in retrievalRef &&
              (typeof retrievalRef.mimeType !== "string" ||
                retrievalRef.mimeType.trim() === "" ||
                /[\r\n]/.test(retrievalRef.mimeType))
            ) {
              errors.push(`${refPrefix}.mimeType must be a MIME type`);
            }

            if (
              retrievalRef.kind === "ipfs-cid" &&
              (typeof retrievalRef.cid !== "string" || !looksLikeIpfsCid(retrievalRef.cid))
            ) {
              errors.push(`${refPrefix}.cid must be a valid IPFS CID`);
            }

            if (
              retrievalRef.kind === "ipfs-cid" &&
              "gatewayUrl" in retrievalRef &&
              (typeof retrievalRef.gatewayUrl !== "string" ||
                !isValidHttpUrl(retrievalRef.gatewayUrl))
            ) {
              errors.push(`${refPrefix}.gatewayUrl must be an http or https URL`);
            }

            if (
              retrievalRef.kind === "ipfs-cid" &&
              "sha256" in retrievalRef &&
              (typeof retrievalRef.sha256 !== "string" || !looksLikeHash(retrievalRef.sha256))
            ) {
              errors.push(`${refPrefix}.sha256 must be a sha256 hash`);
            }

            if (
              retrievalRef.kind === "ipfs-cid" &&
              "sizeBytes" in retrievalRef &&
              (typeof retrievalRef.sizeBytes !== "number" || retrievalRef.sizeBytes <= 0)
            ) {
              errors.push(`${refPrefix}.sizeBytes must be positive`);
            }

            if (
              retrievalRef.kind === "ipfs-cid" &&
              "mimeType" in retrievalRef &&
              (typeof retrievalRef.mimeType !== "string" ||
                retrievalRef.mimeType.trim() === "" ||
                /[\r\n]/.test(retrievalRef.mimeType))
            ) {
              errors.push(`${refPrefix}.mimeType must be a MIME type`);
            }

            if (
              retrievalRef.kind === "magnet" &&
              (typeof retrievalRef.magnetUri !== "string" ||
                !retrievalRef.magnetUri.startsWith("magnet:?"))
            ) {
              errors.push(`${refPrefix}.magnetUri must be a magnet URI`);
            }
          });
        }
      }

      if (!Array.isArray(lesson.contentHashes)) {
        errors.push(`${prefix}.contentHashes must be an array`);
      } else if (lesson.contentHashes.length === 0) {
        warnings.push(`${prefix}.contentHashes is empty; downloads cannot be verified`);
      } else {
        lesson.contentHashes.forEach((hash, hashIndex) => {
          if (typeof hash !== "string" || !looksLikeHash(hash)) {
            errors.push(`${prefix}.contentHashes[${hashIndex}] must be a sha256 hash`);
          }
        });
      }

      if (!isPlainObject(lesson.provenance)) {
        errors.push(`${prefix}.provenance is required`);
      } else {
        if (
          typeof lesson.provenance.adapterName !== "string" ||
          lesson.provenance.adapterName.trim() === ""
        ) {
          errors.push(`${prefix}.provenance.adapterName is required`);
        }
      }

      if (typeof lesson.description === "string" && !isSafeRelativePath(lesson.description)) {
        errors.push(`${prefix}.description contains unsafe path content`);
      }
    });
  }

  let trustState: TrustState = "unsigned";
  if ("signature" in parsed) {
    if (!isPlainObject(parsed.signature)) {
      errors.push("signature must be an object");
    } else {
      if (parsed.signature.algorithm !== "ed25519") {
        errors.push("signature.algorithm must be ed25519");
      }

      if (
        typeof parsed.signature.publicKey !== "string" ||
        !looksLikePublicKey(parsed.signature.publicKey)
      ) {
        errors.push("signature.publicKey must be an Ed25519 public key");
      }

      if (
        typeof parsed.signature.value !== "string" ||
        !looksLikeSignature(parsed.signature.value)
      ) {
        errors.push("signature.value must be an Ed25519 signature");
      }

      trustState = "signed-untrusted";
    }
  }

  return {
    valid: errors.length === 0,
    errors,
    warnings,
    trustState,
    curator:
      errors.length === 0 && isPlainObject(parsed.curator)
        ? (parsed.curator as unknown as SharedCuratorIdentity)
        : undefined,
    manifest:
      errors.length === 0 ? (parsed as unknown as SharedCollectionManifest) : undefined,
  };
};

export const sanitizeManifestForExport = (
  manifest: SharedCollectionManifest,
): SharedCollectionManifest => {
  const clone = structuredClone(manifest) as unknown;
  const clean = (value: unknown): unknown => {
    if (Array.isArray(value)) {
      return value.map(clean);
    }

    if (!isPlainObject(value)) {
      return typeof value === "string" && ABSOLUTE_PATH_PATTERN.test(value) ? "" : value;
    }

    return Object.fromEntries(
      Object.entries(value)
        .filter(([key]) => !isForbiddenManifestKey(key))
        .map(([key, nested]) => [key, clean(nested)]),
    );
  };

  const sanitized = clean(clone) as unknown as SharedCollectionManifest;
  const report = parseCollectionManifest(sanitized);

  if (!report.valid) {
    throw new Error(`Cannot export unsafe manifest: ${report.errors.join("; ")}`);
  }

  return sanitized;
};
