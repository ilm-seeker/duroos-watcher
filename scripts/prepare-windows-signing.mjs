import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

const outputPath = "src-tauri/tauri.windows.conf.json";
const thumbprint = requireEnv("WINDOWS_CERTIFICATE_THUMBPRINT").replace(/\s+/g, "").toUpperCase();
const timestampUrl = requireEnv("WINDOWS_TIMESTAMP_URL");

if (!/^[A-F0-9]{40}$/.test(thumbprint)) {
  fail("WINDOWS_CERTIFICATE_THUMBPRINT must be a 40-character SHA-1 certificate thumbprint.");
}

if (!/^https:\/\//i.test(timestampUrl)) {
  fail("WINDOWS_TIMESTAMP_URL must be an HTTPS timestamp server URL.");
}

const config = {
  $schema: "https://schema.tauri.app/config/2",
  bundle: {
    windows: {
      certificateThumbprint: thumbprint,
      digestAlgorithm: "sha256",
      timestampUrl,
    },
  },
};

mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, `${JSON.stringify(config, null, 2)}\n`, { mode: 0o600 });
console.log(`Wrote Windows signing config to ${outputPath}.`);

function requireEnv(name) {
  const value = process.env[name];
  if (typeof value !== "string" || value.trim() === "") {
    fail(`${name} is required for Windows production signing.`);
  }
  return value.trim();
}

function fail(message) {
  console.error(`failure: ${message}`);
  process.exit(1);
}
