import type { Job } from "./types";

export type QueueFilter = "all" | "active" | "needs-attention" | "downloaded";

export const queueFilters: readonly QueueFilter[] = [
  "all",
  "active",
  "needs-attention",
  "downloaded",
];

export const queueFilterMatches = (filter: QueueFilter, job: Job): boolean => {
  switch (filter) {
    case "all":
      return true;
    case "active":
      return job.state === "queued" || job.state === "running" || job.state === "live";
    case "needs-attention":
      return (
        job.state === "needs-permission" ||
        job.state === "failed-auth" ||
        job.state === "unsupported" ||
        job.state === "failed"
      );
    case "downloaded":
      return job.state === "downloaded" || job.state === "found" || job.state === "archived";
  }
};

export const queueFilterLabel = (filter: QueueFilter): string => {
  switch (filter) {
    case "all":
      return "All";
    case "active":
      return "Active";
    case "needs-attention":
      return "Needs Attention";
    case "downloaded":
      return "Downloaded";
  }
};

const normalizeQueueQuery = (query: string): string => query.trim().toLowerCase();

const includesQueueQuery = (query: string, values: Array<string | undefined>): boolean => {
  if (!query) {
    return true;
  }

  return values.some((value) => value?.toLowerCase().includes(query));
};

export const filterQueueJobs = ({
  jobs,
  sourceById,
  filter,
  query,
  formatDate,
}: {
  jobs: Job[];
  sourceById: ReadonlyMap<string, { label: string }>;
  filter: QueueFilter;
  query: string;
  formatDate: (value?: string) => string;
}): Job[] => {
  const normalizedQuery = normalizeQueueQuery(query);

  return jobs.filter((job) => {
    const sourceLabel = sourceById.get(job.sourceId ?? "")?.label ?? "System";
    return (
      queueFilterMatches(filter, job) &&
      includesQueueQuery(normalizedQuery, [
        job.label,
        job.detail,
        job.state,
        job.kind,
        sourceLabel,
        formatDate(job.updatedAt),
      ])
    );
  });
};
