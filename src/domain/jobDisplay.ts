import type { Job } from "./types";

export interface DisplayJobDetail {
  summary: string;
  technicalDetail?: string;
  technicalLabel?: string;
}

const savedPathPrefixes = ["Saved /", "Saved [local path]"];

export const displayJobDetail = (detail: string, state: Job["state"]): DisplayJobDetail => {
  if (savedPathPrefixes.some((prefix) => detail.startsWith(prefix))) {
    return {
      summary: "Saved in the app library.",
      technicalDetail: detail.replace(/^Saved\s+/, ""),
      technicalLabel: "Show saved path",
    };
  }

  const isTechnicalState = state === "unsupported" || state === "failed" || state === "failed-auth";
  if (detail.length > 180 || isTechnicalState) {
    const sentenceMatch = detail.match(/^(.{1,180}?[.!?])(?:\s|$)/);
    const summary = sentenceMatch?.[1] ?? `${detail.slice(0, 176).trim()}...`;
    return {
      summary,
      technicalDetail: detail,
      technicalLabel: "Show technical detail",
    };
  }

  return { summary: detail };
};
