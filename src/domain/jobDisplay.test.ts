import { describe, expect, it } from "vitest";
import { displayJobDetail } from "./jobDisplay";

describe("displayJobDetail", () => {
  it("summarizes saved absolute paths before they reach queue text", () => {
    expect(
      displayJobDetail(
        "Saved /Users/example/Library/Application Support/io.duroos.watcher/library/downloads/source-youtube/lesson/video.mp4",
        "downloaded",
      ),
    ).toEqual({
      summary: "Saved in the app library.",
      technicalDetail:
        "/Users/example/Library/Application Support/io.duroos.watcher/library/downloads/source-youtube/lesson/video.mp4",
      technicalLabel: "Show saved path",
    });
  });

  it("summarizes already-redacted saved local path text", () => {
    expect(
      displayJobDetail(
        "Saved [local path] Support/io.duroos.watcher/library/downloads/source-youtube/lesson/video.mp4",
        "downloaded",
      ),
    ).toEqual({
      summary: "Saved in the app library.",
      technicalDetail:
        "[local path] Support/io.duroos.watcher/library/downloads/source-youtube/lesson/video.mp4",
      technicalLabel: "Show saved path",
    });
  });

  it("collapses failed technical detail behind an explicit disclosure", () => {
    const detail =
      "Could not fetch https://example.test/feed.xml: HTTP 404 Not Found. YouTube playlist and channel imports require a public playlist_id or channel_id that YouTube exposes through its RSS feed.";

    expect(displayJobDetail(detail, "unsupported")).toEqual({
      summary: "Could not fetch https://example.test/feed.xml: HTTP 404 Not Found.",
      technicalDetail: detail,
      technicalLabel: "Show technical detail",
    });
  });
});
