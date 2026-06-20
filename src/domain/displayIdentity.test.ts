import { describe, expect, it } from "vitest";
import {
  compactHashReference,
  compactKeyFingerprint,
  compactUrlReference,
} from "./displayIdentity";

describe("display identity helpers", () => {
  it("formats long Blossom manifest URLs by host and short fingerprint", () => {
    const full =
      "https://blossom.primal.net/fbb7b02edfb6d4c123fef5885e98634f32f2c030c9e5de8d9f6823af40929f6e.json";

    expect(compactUrlReference(full)).toEqual({
      label: "blossom.primal.net · fbb7b02e...",
      full,
    });
  });

  it("keeps the full URL available separately from the label", () => {
    const full = "https://teacher.example/manifests/duroos-channel-foundations.json";

    expect(compactUrlReference(full)).toEqual({
      label: "teacher.example · duroos-c...",
      full,
    });
  });

  it("shortens public keys consistently", () => {
    const full = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    expect(compactKeyFingerprint(full)).toEqual({
      label: "Key 01234567...",
      full,
    });
  });

  it("formats sha256 hashes without exposing the full hash in the label", () => {
    const full = "sha256:fbb7b02edfb6d4c123fef5885e98634f32f2c030c9e5de8d9f6823af40929f6e";

    expect(compactHashReference(full)).toEqual({
      label: "sha256 fbb7b02e...",
      full,
    });
  });
});
