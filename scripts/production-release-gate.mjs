import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { extname } from "node:path";

const args = new Map(
  process.argv.slice(2).map((arg) => {
    const [key, ...value] = arg.replace(/^--/, "").split("=");
    return [key, value.join("=") || "true"];
  }),
);

const evidencePath = args.get("evidence") ?? "docs/production-release-evidence.json";
const expectedCommit = args.get("commit") ?? execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim();
const failures = [];
const packageJson = readJson("package.json");
const mediaToolsManifest = readJson("src-tauri/binaries/media-tools.manifest.json");

if (!existsSync(evidencePath)) {
  failures.push(
    `Missing production evidence file: ${evidencePath}. Copy docs/production-release-evidence.example.json, fill it with real release evidence, and rerun this gate.`,
  );
  failIfNeeded();
}

const evidence = readJson(evidencePath);
const release = evidence.release ?? {};

requireEqual(evidence.schemaVersion, 1, "schemaVersion must be 1.");
requireEqual(release.version, packageJson.version, `release.version must match package.json version ${packageJson.version}.`);
requireEqual(release.commit, expectedCommit, `release.commit must match expected commit ${expectedCommit}.`);
requireString(release.tag, "release.tag is required.");
if (release.tag && !release.tag.startsWith(`v${packageJson.version}`)) {
  failures.push(`release.tag must start with v${packageJson.version}.`);
}
const platformScope = releasePlatformScope(release);

validateCi(evidence.ci);
validateReleaseWorkflow(evidence.releaseWorkflow, release.tag);
validateArtifactAudits(evidence.artifactAudits);
validateMediaToolReports(evidence.mediaToolReports);
validateSecurityAlerts(evidence.securityAlerts, platformScope);
validateSigning(evidence.signing, platformScope);
validateManualQa(evidence.manualQa, platformScope);

failIfNeeded();
console.log("Production release evidence gate passed.");

function releasePlatformScope(release) {
  const production = normalizePlatforms(release.productionPlatforms, "release.productionPlatforms");
  const alpha = normalizePlatforms(release.alphaPlatforms ?? [], "release.alphaPlatforms");
  const productionSet = new Set(production);
  const alphaSet = new Set(alpha);

  if (!productionSet.has("macos")) {
    failures.push("release.productionPlatforms must include macos.");
  }
  if (!productionSet.has("windows")) {
    failures.push("release.productionPlatforms must include windows.");
  }
  if (!productionSet.has("linux") && !alphaSet.has("linux")) {
    failures.push("release.alphaPlatforms must include linux when linux is not production.");
  }

  for (const platform of alphaSet) {
    if (productionSet.has(platform)) {
      failures.push(`release platform ${platform} cannot be both production and alpha.`);
    }
  }

  const knownPlatformBlockers = Array.isArray(release.knownPlatformBlockers)
    ? release.knownPlatformBlockers
    : [];
  if (release.knownPlatformBlockers !== undefined && !Array.isArray(release.knownPlatformBlockers)) {
    failures.push("release.knownPlatformBlockers must be an array when present.");
  }

  for (const blocker of knownPlatformBlockers) {
    requireKnownPlatform(blocker?.platform, "release.knownPlatformBlockers.platform");
    requireString(blocker?.id, "release.knownPlatformBlockers.id is required.");
    requireString(blocker?.status, "release.knownPlatformBlockers.status is required.");
    requireEvidenceList(blocker?.evidence, "release.knownPlatformBlockers.evidence");
    if (productionSet.has(blocker?.platform)) {
      failures.push(`release.knownPlatformBlockers cannot apply to production platform ${blocker.platform}.`);
    }
  }

  return { production, alpha, knownPlatformBlockers };
}

function normalizePlatforms(value, label) {
  const knownPlatforms = new Set(["macos", "windows", "linux"]);
  if (!Array.isArray(value) || value.length === 0) {
    failures.push(`${label} must be a non-empty array.`);
    return [];
  }

  const platforms = [];
  const seen = new Set();
  for (const platform of value) {
    requireKnownPlatform(platform, label);
    if (seen.has(platform)) {
      failures.push(`${label} contains duplicate platform ${platform}.`);
      continue;
    }
    if (knownPlatforms.has(platform)) {
      platforms.push(platform);
      seen.add(platform);
    }
  }
  return platforms;
}

function requireKnownPlatform(platform, label) {
  if (!["macos", "windows", "linux"].includes(platform)) {
    failures.push(`${label} must be one of macos, windows, or linux.`);
  }
}

function validateCi(ci) {
  if (!ci || typeof ci !== "object") {
    failures.push("ci evidence is required.");
    return;
  }
  requireEqual(ci.headSha, expectedCommit, `ci.headSha must match expected commit ${expectedCommit}.`);
  requireSuccess(ci.conclusion, "ci.conclusion");
  requireString(ci.url, "ci.url is required.");

  const requiredJobs = ["macos-latest", "windows-latest", "ubuntu-22.04"];
  const jobs = Array.isArray(ci.jobs) ? ci.jobs : [];
  for (const name of requiredJobs) {
    const job = jobs.find((candidate) => candidate.name === name || candidate.os === name);
    if (!job) {
      failures.push(`ci.jobs is missing ${name}.`);
      continue;
    }
    requireSuccess(job.conclusion, `ci.jobs.${name}.conclusion`);
  }
}

function validateReleaseWorkflow(workflow, tag) {
  if (!workflow || typeof workflow !== "object") {
    failures.push("releaseWorkflow evidence is required.");
    return;
  }
  requireEqual(workflow.tag, tag, "releaseWorkflow.tag must match release.tag.");
  requireEqual(workflow.headSha, expectedCommit, `releaseWorkflow.headSha must match expected commit ${expectedCommit}.`);
  requireSuccess(workflow.conclusion, "releaseWorkflow.conclusion");
  requireString(workflow.url, "releaseWorkflow.url is required.");

  const requiredJobs = ["macOS Apple Silicon", "macOS Intel", "Windows", "Ubuntu"];
  const jobs = Array.isArray(workflow.jobs) ? workflow.jobs : [];
  for (const name of requiredJobs) {
    const job = jobs.find((candidate) => candidate.name === name);
    if (!job) {
      failures.push(`releaseWorkflow.jobs is missing ${name}.`);
      continue;
    }
    requireSuccess(job.conclusion, `releaseWorkflow.jobs.${name}.conclusion`);
  }
}

function validateArtifactAudits(audits) {
  const requiredPlatforms = [
    {
      platform: "macos-aarch64",
      suffixes: [".app.tar.gz", ".dmg"],
    },
    {
      platform: "macos-x86_64",
      suffixes: [".app.tar.gz", ".dmg"],
    },
    {
      platform: "windows-x86_64",
      suffixes: [".msi", ".exe"],
    },
    {
      platform: "linux-x86_64",
      suffixes: [".AppImage", ".deb"],
    },
  ];

  if (!Array.isArray(audits)) {
    failures.push("artifactAudits must be an array.");
    return;
  }

  for (const required of requiredPlatforms) {
    const audit = audits.find((candidate) => candidate.platform === required.platform);
    if (!audit) {
      failures.push(`artifactAudits is missing ${required.platform}.`);
      continue;
    }
    validateArtifactAudit(audit, required.suffixes);
  }
}

function validateArtifactAudit(audit, requiredSuffixes) {
  requireString(audit.auditJson, `artifactAudits.${audit.platform}.auditJson is required.`);
  requireString(audit.checksums, `artifactAudits.${audit.platform}.checksums is required.`);
  requireExistingFile(audit.auditJson, `artifactAudits.${audit.platform}.auditJson`);
  requireExistingFile(audit.checksums, `artifactAudits.${audit.platform}.checksums`);

  if (!audit.auditJson || !existsSync(audit.auditJson)) {
    return;
  }

  const auditBody = readJson(audit.auditJson);
  const artifacts = Array.isArray(auditBody.artifacts) ? auditBody.artifacts : [];
  if (!artifacts.length) {
    failures.push(`${audit.auditJson} must include at least one artifact.`);
    return;
  }

  for (const artifact of artifacts) {
    requireString(artifact.path, `${audit.auditJson} artifact.path is required.`);
    if (!Number.isInteger(artifact.sizeBytes) || artifact.sizeBytes <= 0) {
      failures.push(`${audit.auditJson} artifact ${artifact.path ?? "unknown"} must have a positive sizeBytes.`);
    }
    if (!/^[a-f0-9]{64}$/i.test(artifact.sha256 ?? "")) {
      failures.push(`${audit.auditJson} artifact ${artifact.path ?? "unknown"} must have a valid SHA-256.`);
    }
  }

  for (const suffix of requiredSuffixes) {
    if (!artifacts.some((artifact) => artifact.path?.endsWith(suffix))) {
      failures.push(`${audit.auditJson} is missing required ${suffix} artifact.`);
    }
  }
}

function validateMediaToolReports(reports) {
  if (!Array.isArray(reports)) {
    failures.push("mediaToolReports must be an array.");
    return;
  }

  const manifestArtifacts = new Map(
    (mediaToolsManifest.artifacts ?? []).map((artifact) => [`${artifact.target}:${artifact.tool}`, artifact]),
  );

  for (const target of mediaToolsManifest.requiredTargets ?? []) {
    const report = reports.find((candidate) => candidate.target === target);
    if (!report) {
      failures.push(`mediaToolReports is missing ${target}.`);
      continue;
    }
    requireString(report.reportJson, `mediaToolReports.${target}.reportJson is required.`);
    requireExistingFile(report.reportJson, `mediaToolReports.${target}.reportJson`);
    if (!report.reportJson || !existsSync(report.reportJson)) {
      continue;
    }

    const reportBody = readJson(report.reportJson);
    requireEqual(reportBody.target, target, `${report.reportJson} target must be ${target}.`);
    const artifactsByTool = new Map((reportBody.artifacts ?? []).map((artifact) => [artifact.tool, artifact]));
    for (const tool of mediaToolsManifest.requiredTools ?? []) {
      const artifact = artifactsByTool.get(tool);
      const manifestArtifact = manifestArtifacts.get(`${target}:${tool}`);
      if (!artifact) {
        failures.push(`${report.reportJson} is missing fetched ${tool}.`);
        continue;
      }
      requireEqual(artifact.sha256, manifestArtifact?.sha256, `${report.reportJson} ${tool} SHA-256 must match manifest.`);
      requireString(artifact.path, `${report.reportJson} ${tool}.path is required.`);
      requireString(artifact.license, `${report.reportJson} ${tool}.license is required.`);
      if (!Number.isInteger(artifact.sizeBytes) || artifact.sizeBytes <= 0) {
        failures.push(`${report.reportJson} ${tool} must have a positive sizeBytes.`);
      }
    }
  }
}

function validateSecurityAlerts(securityAlerts, platformScope) {
  if (!securityAlerts || typeof securityAlerts !== "object") {
    failures.push("securityAlerts evidence is required.");
    return;
  }

  requireIsoDate(securityAlerts.checkedAt, "securityAlerts.checkedAt");
  requireString(securityAlerts.repository, "securityAlerts.repository is required.");
  requireString(securityAlerts.commit, "securityAlerts.commit is required.");
  requireEqual(
    securityAlerts.commit,
    expectedCommit,
    `securityAlerts.commit must match expected commit ${expectedCommit}.`,
  );

  validateAlertGroup(securityAlerts.codeScanning, "securityAlerts.codeScanning", []);
  validateAlertGroup(
    securityAlerts.dependabot,
    "securityAlerts.dependabot",
    allowedAlphaSecurityAlertIds(platformScope),
  );
}

function allowedAlphaSecurityAlertIds(platformScope) {
  const alphaPlatforms = new Set(platformScope.alpha);
  const productionPlatforms = new Set(platformScope.production);
  return platformScope.knownPlatformBlockers
    .filter((blocker) => alphaPlatforms.has(blocker.platform) && !productionPlatforms.has(blocker.platform))
    .map((blocker) => blocker.id);
}

function validateAlertGroup(alertGroup, label, allowedOpenAlertIds) {
  if (!alertGroup || typeof alertGroup !== "object") {
    failures.push(`${label} evidence is required.`);
    return;
  }

  requireNonNegativeInteger(alertGroup.openAlerts, `${label}.openAlerts`);
  requireEvidenceList(alertGroup.evidence, `${label}.evidence`);

  if (alertGroup.openAlerts === 0) {
    if (alertGroup.openAlertIds !== undefined) {
      if (!Array.isArray(alertGroup.openAlertIds)) {
        failures.push(`${label}.openAlertIds must be an array when present.`);
      } else if (alertGroup.openAlertIds.length > 0) {
        failures.push(`${label}.openAlertIds must be empty when openAlerts is 0.`);
      }
    }
    return;
  }

  if (!allowedOpenAlertIds.length) {
    failures.push(`${label}.openAlerts must be 0 for production release. Actual: ${alertGroup.openAlerts}.`);
    return;
  }

  if (alertGroup.openAlertIds !== undefined) {
    if (!Array.isArray(alertGroup.openAlertIds)) {
      failures.push(`${label}.openAlertIds must be an array when present.`);
    }
  } else {
    failures.push(`${label}.openAlertIds must list each allowed alpha-platform alert.`);
    return;
  }

  if (!Array.isArray(alertGroup.openAlertIds)) {
    return;
  }

  if (alertGroup.openAlertIds.length !== alertGroup.openAlerts) {
    failures.push(`${label}.openAlertIds count must match ${label}.openAlerts.`);
  }

  for (const alertId of alertGroup.openAlertIds) {
    if (typeof alertId !== "string" || !alertId.trim()) {
      failures.push(`${label}.openAlertIds entries must be non-empty strings.`);
      continue;
    }
    if (!allowedOpenAlertIds.some((allowedId) => alertId === allowedId || alertId.includes(allowedId))) {
      failures.push(`${label}.openAlertIds contains unapproved production blocker: ${alertId}.`);
    }
  }
}

function validateSigning(signing, platformScope) {
  if (!signing || typeof signing !== "object") {
    failures.push("signing evidence is required.");
    return;
  }

  const production = new Set(platformScope.production);

  if (production.has("macos")) {
    requireTrue(signing.macos?.appSigned, "signing.macos.appSigned must be true.");
    requireTrue(signing.macos?.dmgSigned, "signing.macos.dmgSigned must be true.");
    requireTrue(signing.macos?.notarized, "signing.macos.notarized must be true.");
    requireTrue(signing.macos?.stapled, "signing.macos.stapled must be true.");
    requireEvidenceList(signing.macos?.evidence, "signing.macos.evidence");
  }

  if (production.has("windows")) {
    requireTrue(signing.windows?.installerSigned, "signing.windows.installerSigned must be true.");
    requireEvidenceList(signing.windows?.evidence, "signing.windows.evidence");
  }

  if (production.has("linux")) {
    requireTrue(signing.linux?.packageReviewPassed, "signing.linux.packageReviewPassed must be true.");
    requireEvidenceList(signing.linux?.evidence, "signing.linux.evidence");
  } else if (platformScope.alpha.includes("linux") && signing.linux !== undefined) {
    requireEvidenceList(signing.linux?.evidence, "signing.linux.evidence");
  }
}

function validateManualQa(manualQa, platformScope) {
  if (!Array.isArray(manualQa)) {
    failures.push("manualQa must be an array.");
    return;
  }

  const platforms = platformScope.production;
  const requiredChecks = [
    "installAndLaunch",
    "importVideoAudioPdf",
    "followSignedChannel",
    "refreshFollowedChannel",
    "downloadMissingMedia",
    "publishChannelWithRelayAndBlossom",
    "offlineModeBlocksRemoteFetch",
    "webviewPlayback",
    "nativePlayerFallback",
    "phoneSharingStartStop",
    "privacyLeakInspection",
  ];

  for (const platform of platforms) {
    const run = manualQa.find((candidate) => candidate.platform === platform);
    if (!run) {
      failures.push(`manualQa is missing ${platform}.`);
      continue;
    }
    requireSuccess(run.result, `manualQa.${platform}.result`);
    requireIsoDate(run.date, `manualQa.${platform}.date`);
    requireString(run.tester, `manualQa.${platform}.tester is required.`);
    requireString(run.artifact, `manualQa.${platform}.artifact is required.`);
    const checks = run.checks ?? {};
    for (const check of requiredChecks) {
      requireTrue(checks[check], `manualQa.${platform}.checks.${check} must be true.`);
    }
    requireEvidenceList(run.notes, `manualQa.${platform}.notes`);
  }
}

function requireEqual(actual, expected, message) {
  if (actual !== expected) {
    failures.push(`${message} Actual: ${String(actual ?? "missing")}.`);
  }
}

function requireString(value, message) {
  if (typeof value !== "string" || value.trim() === "") {
    failures.push(message);
  }
}

function requireTrue(value, message) {
  if (value !== true) {
    failures.push(message);
  }
}

function requireSuccess(value, label) {
  if (value !== "success" && value !== "pass") {
    failures.push(`${label} must be success/pass. Actual: ${String(value ?? "missing")}.`);
  }
}

function requireNonNegativeInteger(value, label) {
  if (!Number.isInteger(value) || value < 0) {
    failures.push(`${label} must be a non-negative integer.`);
  }
}

function requireIsoDate(value, label) {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(value)) {
    failures.push(`${label} must use YYYY-MM-DD.`);
  }
}

function requireEvidenceList(value, label) {
  if (!Array.isArray(value) || value.length === 0 || value.some((entry) => typeof entry !== "string" || !entry.trim())) {
    failures.push(`${label} must include at least one evidence note or command output pointer.`);
  }
}

function requireExistingFile(path, label) {
  if (typeof path !== "string" || !path.trim()) {
    return;
  }
  if (!existsSync(path)) {
    failures.push(`${label} does not exist: ${path}`);
    return;
  }
  const stat = statSync(path);
  if (!stat.isFile() || stat.size <= 0) {
    failures.push(`${label} must be a non-empty file: ${path}`);
  }
}

function readJson(path) {
  if (extname(path) !== ".json") {
    failures.push(`${path} must be a JSON file.`);
  }
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    failures.push(`Could not parse JSON ${path}: ${error.message}`);
    failIfNeeded();
  }
}

function failIfNeeded() {
  if (!failures.length) {
    return;
  }
  console.error(failures.map((failure) => `failure: ${failure}`).join("\n"));
  process.exit(1);
}
