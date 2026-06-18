import { describe, expect, it } from "vitest";
import { filterQueueJobs, queueFilterMatches } from "./jobQueue";
import type { Job } from "./types";

const job = (id: string, state: Job["state"], overrides: Partial<Job> = {}): Job => ({
  id,
  kind: "download",
  state,
  sourceId: "source-youtube",
  label: `Job ${id}`,
  detail: `Detail ${id}`,
  retryCount: 0,
  updatedAt: "2026-06-17T15:16:00.000Z",
  ...overrides,
});

const sourceById = new Map([
  ["source-youtube", { label: "YouTube: Poem Collection" }],
  ["source-archive", { label: "Archive.org" }],
]);

const formatDate = (value?: string) => (value ? "Jun 17 at 3:16 PM" : "");

describe("queueFilterMatches", () => {
  it("groups active, needs-attention, and downloaded-style queue states", () => {
    expect(queueFilterMatches("active", job("queued", "queued"))).toBe(true);
    expect(queueFilterMatches("active", job("running", "running"))).toBe(true);
    expect(queueFilterMatches("active", job("live", "live"))).toBe(true);
    expect(queueFilterMatches("needs-attention", job("auth", "failed-auth"))).toBe(true);
    expect(queueFilterMatches("needs-attention", job("unsupported", "unsupported"))).toBe(true);
    expect(queueFilterMatches("needs-attention", job("failed", "failed"))).toBe(true);
    expect(queueFilterMatches("downloaded", job("downloaded", "downloaded"))).toBe(true);
    expect(queueFilterMatches("downloaded", job("found", "found"))).toBe(true);
    expect(queueFilterMatches("downloaded", job("archived", "archived"))).toBe(true);
    expect(queueFilterMatches("downloaded", job("queued", "queued"))).toBe(false);
  });
});

describe("filterQueueJobs", () => {
  const jobs = [
    job("downloaded", "downloaded", {
      detail: "Saved in the app library.",
      label: "Download: Tawheed Poem",
    }),
    job("unsupported", "unsupported", {
      detail: "Could not fetch feed.",
      label: "Source ingest",
      sourceId: "source-archive",
    }),
    job("running", "running", {
      detail: "Refreshing source",
      kind: "refresh",
      label: "Refresh: YouTube",
    }),
  ];

  it("filters by queue tab before applying search", () => {
    expect(
      filterQueueJobs({
        jobs,
        sourceById,
        filter: "needs-attention",
        query: "",
        formatDate,
      }).map((item) => item.id),
    ).toEqual(["unsupported"]);
  });

  it("searches job label, source label, state, kind, detail, and date", () => {
    const cases: Array<[string, string[]]> = [
      ["tawheed", ["downloaded"]],
      ["archive.org", ["unsupported"]],
      ["unsupported", ["unsupported"]],
      ["refresh", ["running"]],
      ["jun 17", ["downloaded", "unsupported", "running"]],
    ];

    for (const [query, expectedIds] of cases) {
      expect(
        filterQueueJobs({
          jobs,
          sourceById,
          filter: "all",
          query,
          formatDate,
        }).map((item) => item.id),
      ).toEqual(expectedIds);
    }
  });
});
