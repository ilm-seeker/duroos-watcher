import { describe, expect, it } from "vitest";
import type { PublisherEndpointTestReport } from "./types";
import { endpointTestHasFailures, endpointTestStatus } from "./publisherEndpointReport";

const report = (
  passed: boolean,
  blossomUploaded: boolean[],
  relayAccepted: boolean[],
): PublisherEndpointTestReport => ({
  passed,
  messages: [],
  blossomResults: blossomUploaded.map((uploaded, index) => ({
    serverUrl: `https://blossom-${index}.example`,
    hash: `${index}`.repeat(64).slice(0, 64),
    url: uploaded ? `https://blossom-${index}.example/blob` : undefined,
    uploaded,
    message: uploaded ? "Blob stored by server." : "Upload failed.",
  })),
  relayResults: relayAccepted.map((accepted, index) => ({
    relayUrl: `wss://relay-${index}.example`,
    accepted,
    message: accepted ? "" : "Relay rejected the event.",
  })),
});

describe("publisher endpoint report status", () => {
  it("marks every configured endpoint passing as a full pass", () => {
    const fullPass = report(true, [true, true], [true, true]);

    expect(endpointTestHasFailures(fullPass)).toBe(false);
    expect(endpointTestStatus(fullPass)).toEqual({
      label: "All endpoints passed",
      tone: "positive",
    });
  });

  it("marks quorum success with failed endpoints as a partial pass", () => {
    const partialPass = report(true, [true, false], [true, false]);

    expect(endpointTestHasFailures(partialPass)).toBe(true);
    expect(endpointTestStatus(partialPass)).toEqual({
      label: "Partial endpoint pass",
      tone: "warning",
    });
  });

  it("keeps failed quorum as endpoint issues", () => {
    const failedQuorum = report(false, [false], [true]);

    expect(endpointTestStatus(failedQuorum)).toEqual({
      label: "Endpoint issues",
      tone: "warning",
    });
  });
});
