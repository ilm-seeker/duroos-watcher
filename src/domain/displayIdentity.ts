const compactLength = 8;

export interface CompactIdentifier {
  label: string;
  full: string;
}

const compactValue = (value: string, length = compactLength): string => {
  const trimmed = value.trim();
  if (trimmed.length <= length + 3) {
    return trimmed;
  }

  return `${trimmed.slice(0, length)}...`;
};

const lastMeaningfulPathSegment = (pathname: string): string | undefined => {
  const segments = pathname.split("/").filter(Boolean);
  return segments
    .slice()
    .reverse()
    .find((segment) => segment.length >= compactLength);
};

export const compactUrlReference = (value: string): CompactIdentifier => {
  const full = value.trim();
  if (!full) {
    return { label: "No reference", full };
  }

  try {
    const parsed = new URL(full);
    const candidate =
      lastMeaningfulPathSegment(parsed.pathname) ?? parsed.search.replace(/^\?/, "") ?? "";
    const fingerprint = candidate ? compactValue(candidate.replace(/\.[^.]+$/, "")) : "";

    return {
      label: fingerprint ? `${parsed.hostname} · ${fingerprint}` : parsed.hostname,
      full,
    };
  } catch {
    return { label: compactValue(full), full };
  }
};

export const compactKeyFingerprint = (value: string): CompactIdentifier => {
  const full = value.trim();
  if (!full) {
    return { label: "Key unavailable", full };
  }

  return { label: `Key ${compactValue(full)}`, full };
};

export const compactHashReference = (value: string): CompactIdentifier => {
  const full = value.trim();
  if (!full) {
    return { label: "Hash unavailable", full };
  }

  const hash = full.replace(/^sha256:/i, "");
  return { label: `sha256 ${compactValue(hash)}`, full };
};
