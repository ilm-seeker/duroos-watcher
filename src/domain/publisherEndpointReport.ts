import type { PublisherEndpointTestReport } from "./types";

export type EndpointTestStatus = {
  label:
    | "All endpoints passed"
    | "Durability warning"
    | "Partial endpoint pass"
    | "Endpoint issues";
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

  if (report.passed && endpointDurabilityWarning(report)) {
    return { label: "Durability warning", tone: "warning" };
  }

  if (report.passed) {
    return { label: "All endpoints passed", tone: "positive" };
  }

  return { label: "Endpoint issues", tone: "warning" };
};

export const endpointDurabilityWarning = (
  report: PublisherEndpointTestReport,
): string | null => {
  if (!report.passed) {
    return null;
  }

  const uploadedCount = report.blossomResults.filter((result) => result.uploaded).length;
  const acceptedCount = report.relayResults.filter((result) => result.accepted).length;

  if (uploadedCount >= 2 && acceptedCount >= 2) {
    return null;
  }

  return `Durability warning: ${uploadedCount} Blossom server(s) and ${acceptedCount} relay(s) passed. Archive durability requires at least two of each before publishing.`;
};
