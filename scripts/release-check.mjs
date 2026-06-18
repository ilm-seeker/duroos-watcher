import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { extname } from "node:path";

const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));
const failures = [];
const warnings = [];

const packageJson = readJson("package.json");
const tauriConfig = readJson("src-tauri/tauri.conf.json");
const cargoToml = readFileSync("src-tauri/Cargo.toml", "utf8");
const mediaToolsManifest = readJson("src-tauri/binaries/media-tools.manifest.json");

const cargoVersion = cargoToml.match(/^version = "([^"]+)"/m)?.[1];
if (packageJson.version !== tauriConfig.version || packageJson.version !== cargoVersion) {
  failures.push(
    `Version mismatch: package=${packageJson.version}, tauri=${tauriConfig.version}, cargo=${cargoVersion}`,
  );
}

const csp = tauriConfig.app?.security?.csp;
if (!csp || csp.includes("*")) {
  failures.push("Tauri CSP must be present and must not contain wildcard sources.");
}

const assetScope = tauriConfig.app?.security?.assetProtocol?.scope ?? [];
if (!assetScope.includes("$APPDATA/library/**/*")) {
  failures.push("Tauri asset protocol must be scoped to $APPDATA/library/**/*.");
}

const requiredBundleTargets = ["app", "dmg", "nsis", "msi", "appimage", "deb"];
const bundleTargets = tauriConfig.bundle?.targets ?? [];
for (const target of requiredBundleTargets) {
  if (!bundleTargets.includes(target)) {
    failures.push(`Missing bundle target: ${target}`);
  }
}
if (tauriConfig.bundle?.active !== true) {
  failures.push("Tauri bundle.active must be true for release builds.");
}

const trackedFiles = execFileSync("git", ["ls-files"], { encoding: "utf8" })
  .split("\n")
  .filter(Boolean);
const searchableExtensions = new Set([
  ".css",
  ".html",
  ".json",
  ".md",
  ".mjs",
  ".rs",
  ".toml",
  ".ts",
  ".tsx",
  ".yml",
  ".yaml",
]);
for (const file of trackedFiles) {
  if (file === "scripts/release-check.mjs") {
    continue;
  }
  if (!searchableExtensions.has(extname(file))) {
    continue;
  }
  const body = readFileSync(file, "utf8");
  if (/paltalk/i.test(body)) {
    failures.push(`Remove Paltalk reference from ${file}.`);
  }
}

const forbiddenTrackedNames = [
  ".env",
  "cookies.txt",
  "yt-dlp-cookies.txt",
  "session.sqlite",
  "telegram.session",
];
for (const file of trackedFiles) {
  if (forbiddenTrackedNames.some((name) => file.endsWith(name))) {
    failures.push(`Tracked secret or credential-like file is forbidden: ${file}`);
  }
}

if (!Array.isArray(mediaToolsManifest.artifacts)) {
  failures.push("media-tools.manifest.json must expose an artifacts array.");
} else if (mediaToolsManifest.artifacts.length === 0) {
  warnings.push("No bundled media tools are pinned yet; release artifacts are alpha/testing only.");
}

for (const artifact of mediaToolsManifest.artifacts ?? []) {
  for (const field of mediaToolsManifest.artifactFields ?? []) {
    if (!artifact[field]) {
      failures.push(`Media tool artifact is missing ${field}.`);
    }
  }
  if (artifact.sha256 && !/^[a-f0-9]{64}$/i.test(artifact.sha256)) {
    failures.push(`Invalid SHA-256 for media tool artifact ${artifact.tool ?? "unknown"}.`);
  }
}

if (warnings.length) {
  console.warn(warnings.map((warning) => `warning: ${warning}`).join("\n"));
}

if (failures.length) {
  console.error(failures.map((failure) => `failure: ${failure}`).join("\n"));
  process.exit(1);
}

console.log("Release repo checks passed.");
