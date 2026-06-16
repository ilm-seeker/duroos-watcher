import { describe, expect, it } from "vitest";
import { getAdapter, isDownloadPermitted, sourceAdapters } from "./sourceAdapters";

describe("sourceAdapters", () => {
  it("registers every planned v1 source", () => {
    expect(sourceAdapters.map((adapter) => adapter.platform).sort()).toEqual([
      "archive-org",
      "local-files",
      "odysee",
      "rss-feed",
      "rumble",
      "teacher-relay",
      "telegram",
      "x",
      "youtube",
    ]);
  });

  it("marks credential-bound platforms truthfully", () => {
    expect(getAdapter("telegram").capability.authRequired).toBe(false);
    expect(getAdapter("rss-feed").capability.download).toBe("supported");
    expect(getAdapter("telegram").capability.reliability).toBe("best-effort");
    expect(getAdapter("teacher-relay").capability.autoUpdate).toBe("supported");
    expect(getAdapter("x").capability.reliability).toBe("credential-bound");
    expect(getAdapter("youtube").capability.download).toBe("limited");
    expect(getAdapter("odysee").capability.reliability).toBe("best-effort");
    expect(getAdapter("odysee").privacyBoundary).toContain("wallet");
  });

  it("allows downloads based on adapter capability", () => {
    expect(isDownloadPermitted("archive-org")).toBe(true);
    expect(isDownloadPermitted("local-files")).toBe(true);
  });
});
