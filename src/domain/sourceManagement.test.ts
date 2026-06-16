import { describe, expect, it } from "vitest";
import { getSourceStats, splitSourceRows } from "./sourceManagement";
import type { Job, Lesson, Source } from "./types";

const source = (id: string, label = id): Source => ({
  id,
  platform: "youtube",
  label,
  identifier: `https://example.test/${id}`,
  feedFormat: "rss",
  feedTransport: "https",
  trustState: "unsigned",
  authMode: "none",
  updateSchedule: "Manual",
  capability: {
    metadata: "supported",
    download: "limited",
    autoUpdate: "limited",
    authRequired: false,
    authMode: "none",
    reliability: "best-effort",
    note: "Test source",
  },
  enabled: true,
});

const lesson = (
  id: string,
  sourceId: string,
  mediaFileId?: string,
  contentType: Lesson["contentType"] = "video",
): Lesson => ({
  id,
  sourceId,
  title: `Lesson ${id}`,
  contentType,
  teacherId: "teacher-test",
  collectionId: "collection-test",
  sourceUrl: `https://example.test/watch/${id}`,
  thumbnailTone: "emerald",
  mediaFileId,
  provenanceId: `prov-${id}`,
});

const job = (id: string, sourceId: string): Job => ({
  id,
  sourceId,
  kind: "download",
  state: "downloaded",
  label: `Job ${id}`,
  detail: "Test job",
  retryCount: 0,
  updatedAt: "2026-06-16T00:00:00.000Z",
});

describe("splitSourceRows", () => {
  it("keeps added playlists out of the global capability matrix", () => {
    const rows = splitSourceRows([
      source("source-youtube", "YouTube"),
      source("source-odysee", "Odysee"),
      source("source-youtube-playlist-abc", "Playlist ABC"),
    ]);

    expect(rows.capabilitySources.map((item) => item.label)).toEqual(["YouTube", "Odysee"]);
    expect(rows.addedSources.map((item) => item.label)).toEqual(["Playlist ABC"]);
  });
});

describe("getSourceStats", () => {
  it("counts local and missing media for an added source", () => {
    expect(
      getSourceStats(
        "source-youtube-playlist-abc",
        [
          lesson("1", "source-youtube-playlist-abc", "media-1"),
          lesson("2", "source-youtube-playlist-abc"),
          lesson("4", "source-youtube-playlist-abc", undefined, "post"),
          lesson("3", "source-youtube"),
        ],
        [job("1", "source-youtube-playlist-abc")],
      ),
    ).toEqual({
      itemCount: 3,
      localFileCount: 1,
      missingFileCount: 1,
      postCount: 1,
      jobCount: 1,
    });
  });
});
