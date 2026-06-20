import { describe, expect, it } from "vitest";
import {
  createOnboardingState,
  onboardingStorageKey,
  readOnboardingState,
  recordOnboardingLane,
  type StorageLike,
} from "./onboardingState";

const memoryStorage = (initial?: Record<string, string>): StorageLike & { data: Map<string, string> } => {
  const data = new Map(Object.entries(initial ?? {}));

  return {
    data,
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => {
      data.set(key, value);
    },
  };
};

describe("onboardingState", () => {
  it("creates versioned local onboarding state for a chosen lane", () => {
    expect(createOnboardingState("publish", new Date("2026-06-20T18:00:00.000Z"))).toEqual({
      version: 1,
      preferredLane: "publish",
      startedAt: "2026-06-20T18:00:00.000Z",
      completedSteps: [],
    });
  });

  it("records a learner or teacher lane without remote state", () => {
    const storage = memoryStorage();

    recordOnboardingLane("study", storage, new Date("2026-06-20T18:00:00.000Z"));
    recordOnboardingLane("publish", storage, new Date("2026-06-20T19:00:00.000Z"));

    expect(readOnboardingState(storage)).toEqual({
      version: 1,
      preferredLane: "publish",
      startedAt: "2026-06-20T18:00:00.000Z",
      completedSteps: [],
    });
  });

  it("ignores invalid or future-shaped local state", () => {
    const storage = memoryStorage({
      [onboardingStorageKey]: JSON.stringify({
        version: 2,
        preferredLane: "study",
        startedAt: "2026-06-20T18:00:00.000Z",
        completedSteps: [],
      }),
    });

    expect(readOnboardingState(storage)).toBeNull();
  });
});
