export type OnboardingLane = "study" | "publish";

export interface OnboardingState {
  version: 1;
  preferredLane: OnboardingLane;
  startedAt: string;
  completedSteps: string[];
}

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export const onboardingStorageKey = "duroos.onboarding.v1";

export const createOnboardingState = (
  preferredLane: OnboardingLane,
  now = new Date(),
): OnboardingState => ({
  version: 1,
  preferredLane,
  startedAt: now.toISOString(),
  completedSteps: [],
});

export const readOnboardingState = (
  storage: StorageLike | undefined = globalThis.localStorage,
): OnboardingState | null => {
  if (!storage) {
    return null;
  }

  try {
    const raw = storage.getItem(onboardingStorageKey);
    if (!raw) {
      return null;
    }

    const parsed = JSON.parse(raw) as Partial<OnboardingState>;
    if (
      parsed.version !== 1 ||
      (parsed.preferredLane !== "study" && parsed.preferredLane !== "publish") ||
      typeof parsed.startedAt !== "string" ||
      !Array.isArray(parsed.completedSteps) ||
      parsed.completedSteps.some((step) => typeof step !== "string")
    ) {
      return null;
    }

    return {
      version: 1,
      preferredLane: parsed.preferredLane,
      startedAt: parsed.startedAt,
      completedSteps: parsed.completedSteps,
    };
  } catch {
    return null;
  }
};

export const writeOnboardingState = (
  state: OnboardingState,
  storage: StorageLike | undefined = globalThis.localStorage,
): void => {
  storage?.setItem(onboardingStorageKey, JSON.stringify(state));
};

export const recordOnboardingLane = (
  preferredLane: OnboardingLane,
  storage: StorageLike | undefined = globalThis.localStorage,
  now = new Date(),
): OnboardingState => {
  const current = readOnboardingState(storage);
  const next: OnboardingState = current
    ? { ...current, preferredLane }
    : createOnboardingState(preferredLane, now);

  writeOnboardingState(next, storage);
  return next;
};
