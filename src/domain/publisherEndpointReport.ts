import type { PublisherEndpointTestReport } from "./types";

export type EndpointTestStatus = {
  label: "All endpoints passed" | "Partial endpoint pass" | "Endpoint issues";
  tone: "positive" | "warning";
};

export const endpointTestHasFailures = (report: PublisherEndpointTestReport): boolean =>
  report.blossomResults.some((result) => !result.uploaded) ||
  report.relayResults.some((result) => !result.accepted);

export const endpointTestStatus = (
  report: PublisherEndpointTestReport,
): EndpointTestStatus => {
  if (report.passed && endpointTestHasFailures(report)) {
    return { label: "Partial endpoint pass", tone: "warning" };
  }

  if (report.passed) {
    return { label: "All endpoints passed", tone: "positive" };
  }

  return { label: "Endpoint issues", tone: "warning" };
};
