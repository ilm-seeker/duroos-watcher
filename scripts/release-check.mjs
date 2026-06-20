import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
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
const bundleResources = tauriConfig.bundle?.resources ?? [];
if (!bundleResources.includes("binaries/vendor/")) {
  failures.push("Tauri bundle.resources must include binaries/vendor/ for pinned media tools.");
}
if (!existsSync("src-tauri/icons/icon.png")) {
  failures.push("Missing Tauri PNG app icon: src-tauri/icons/icon.png");
}
if (
  (bundleTargets.includes("nsis") || bundleTargets.includes("msi")) &&
  !existsSync("src-tauri/icons/icon.ico")
) {
  failures.push("Missing Windows app icon required by tauri-build: src-tauri/icons/icon.ico");
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
  failures.push("media-tools.manifest.json must pin media-tool artifacts before release checks pass.");
}

if (mediaToolsManifest.status !== "pinned") {
  failures.push("media-tools.manifest.json status must be pinned before release checks pass.");
}

const requiredMediaTools = mediaToolsManifest.requiredTools ?? [];
const requiredMediaTargets = mediaToolsManifest.requiredTargets ?? [];
const mediaArtifactKeys = new Set();

for (const artifact of mediaToolsManifest.artifacts ?? []) {
  for (const field of mediaToolsManifest.artifactFields ?? []) {
    if (!artifact[field]) {
      failures.push(`Media tool artifact is missing ${field}.`);
    }
  }
  if (artifact.sha256 && !/^[a-f0-9]{64}$/i.test(artifact.sha256)) {
    failures.push(`Invalid SHA-256 for media tool artifact ${artifact.tool ?? "unknown"}.`);
  }
  if (artifact.sourceUrl) {
    try {
      const sourceUrl = new URL(artifact.sourceUrl);
      if (sourceUrl.protocol !== "https:") {
        failures.push(`Media tool artifact must use HTTPS: ${artifact.sourceUrl}`);
      }
      if (sourceUrl.pathname.split("/").includes("latest")) {
        failures.push(`Media tool artifact source URL must not use a mutable latest path: ${artifact.sourceUrl}`);
      }
    } catch {
      failures.push(`Media tool artifact has an invalid source URL: ${artifact.sourceUrl}`);
    }
  }

  const key = `${artifact.tool}:${artifact.target}`;
  if (mediaArtifactKeys.has(key)) {
    failures.push(`Duplicate media tool artifact for ${key}.`);
  }
  mediaArtifactKeys.add(key);
}

for (const tool of requiredMediaTools) {
  for (const target of requiredMediaTargets) {
    if (!mediaArtifactKeys.has(`${tool}:${target}`)) {
      failures.push(`Missing pinned media tool artifact for ${tool} on ${target}.`);
    }
  }
}

const releaseWorkflow = existsSync(".github/workflows/release.yml")
  ? readFileSync(".github/workflows/release.yml", "utf8")
  : "";
if (!releaseWorkflow.includes("media_tools_target")) {
  failures.push("Release workflow matrix must define media_tools_target for each platform.");
}
if (!releaseWorkflow.includes("npm run media-tools:fetch")) {
  failures.push("Release workflow must fetch pinned media tools before building Tauri artifacts.");
}
if (!releaseWorkflow.includes("media-tools-report.json")) {
  failures.push("Release workflow must upload media-tools-report.json with release evidence.");
}

if (warnings.length) {
  console.warn(warnings.map((warning) => `warning: ${warning}`).join("\n"));
}

if (failures.length) {
  console.error(failures.map((failure) => `failure: ${failure}`).join("\n"));
  process.exit(1);
}

console.log("Release repo checks passed.");
