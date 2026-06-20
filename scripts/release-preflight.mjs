import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";

const packageJson = readJson("package.json");
const tauriConfig = readJson("src-tauri/tauri.conf.json");
const releaseWorkflow = readText(".github/workflows/release.yml");
const expectedTagPrefix = `v${packageJson.version}`;
const expectedEvidencePath = "docs/production-release-evidence.json";
const productionEvidence = existsSync(expectedEvidencePath) ? readJson(expectedEvidencePath) : null;
const repo = resolveRepo();
const headSha = run("git", ["rev-parse", "HEAD"], { required: true }).stdout.trim();

const passes = [];
const blockers = [];
const warnings = [];

checkCleanWorktree();
checkReleaseRepoRules();
checkReleaseWorkflow();
checkGithubState();
checkProductionEvidence();

printReport();
process.exitCode = blockers.length > 0 ? 1 : 0;

function checkCleanWorktree() {
  const status = run("git", ["status", "--porcelain"], { required: true }).stdout.trim();
  if (status) {
    warnings.push("Worktree has uncommitted changes; run this preflight from the exact commit you plan to release.");
    return;
  }
  passes.push("Worktree is clean.");
}

function checkReleaseRepoRules() {
  const result = run("node", ["scripts/release-check.mjs"]);
  if (result.ok) {
    passes.push("Release repo checks passed.");
    return;
  }
  blockers.push(`Release repo checks failed:\n${indent(result.output.trim())}`);
}

function checkReleaseWorkflow() {
  const bundleTargets = new Set(tauriConfig.bundle?.targets ?? []);
  const requiredWorkflowSecrets = [
    "TAURI_SIGNING_PRIVATE_KEY",
    "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
    "APPLE_CERTIFICATE",
    "APPLE_CERTIFICATE_PASSWORD",
    "APPLE_SIGNING_IDENTITY",
    "APPLE_ID",
    "APPLE_PASSWORD",
    "APPLE_TEAM_ID",
  ];

  const missingWorkflowSecrets = requiredWorkflowSecrets.filter((secret) => !releaseWorkflow.includes(secret));
  if (missingWorkflowSecrets.length) {
    blockers.push(`Release workflow does not pass required signing secrets: ${missingWorkflowSecrets.join(", ")}.`);
  } else {
    passes.push("Release workflow passes Tauri and Apple signing secret names to the build step.");
  }

  if (bundleTargets.has("nsis") || bundleTargets.has("msi")) {
    const windowsConfig = tauriConfig.bundle?.windows ?? {};
    const hasWindowsConfig =
      typeof windowsConfig.signCommand === "string" ||
      typeof windowsConfig.certificateThumbprint === "string" ||
      releaseWorkflow.includes("WINDOWS_CERTIFICATE");

    if (!hasWindowsConfig) {
      blockers.push(
        "Windows installer signing is not configured. Add a Tauri Windows signing path before production: PFX import plus certificateThumbprint, Azure Key Vault signCommand, Azure Trusted Signing, or another verified custom signCommand.",
      );
    } else {
      passes.push("Windows signing configuration is present.");
    }
  }
}

function checkGithubState() {
  if (!repo) {
    blockers.push("Could not resolve GitHub repository. Set a GitHub origin remote before running release preflight.");
    return;
  }

  const secretNames = listGhNames(["secret", "list", "--repo", repo]);
  if (!secretNames) {
    blockers.push("Could not read GitHub Actions secrets with gh. Run `gh auth status` and grant repository access.");
  } else {
    const requiredSecrets = [
      "TAURI_SIGNING_PRIVATE_KEY",
      "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
      "APPLE_CERTIFICATE",
      "APPLE_CERTIFICATE_PASSWORD",
      "APPLE_SIGNING_IDENTITY",
      "APPLE_ID",
      "APPLE_PASSWORD",
      "APPLE_TEAM_ID",
      "WINDOWS_CERTIFICATE",
      "WINDOWS_CERTIFICATE_PASSWORD",
    ];
    const missingSecrets = requiredSecrets.filter((name) => !secretNames.has(name));
    if (missingSecrets.length) {
      blockers.push(`Missing GitHub Actions secrets: ${missingSecrets.join(", ")}.`);
    } else {
      passes.push("Required GitHub Actions signing secret names exist.");
    }
  }

  const variableNames = listGhNames(["variable", "list", "--repo", repo]);
  if (!variableNames) {
    warnings.push("Could not read GitHub Actions variables with gh; Windows thumbprint/timestamp variables were not checked.");
  } else {
    const requiredVariables = ["WINDOWS_CERTIFICATE_THUMBPRINT", "WINDOWS_TIMESTAMP_URL"];
    const missingVariables = requiredVariables.filter((name) => !variableNames.has(name));
    if (missingVariables.length) {
      blockers.push(`Missing GitHub Actions variables for Windows signing: ${missingVariables.join(", ")}.`);
    } else {
      passes.push("Required Windows signing variable names exist.");
    }
  }

  const codeScanningAlerts = listCodeScanningAlerts();
  if (codeScanningAlerts === null) {
    blockers.push("Could not read GitHub code-scanning alerts with gh.");
  } else if (codeScanningAlerts.length) {
    blockers.push(`Open code-scanning alerts remain: ${codeScanningAlerts.join(", ")}.`);
  } else {
    passes.push("No open GitHub code-scanning alerts.");
  }

  const dependabotAlerts = listDependabotAlerts();
  if (dependabotAlerts === null) {
    blockers.push("Could not read GitHub Dependabot alerts with gh.");
  } else if (dependabotAlerts.length) {
    const allowedAlertIds = allowedAlphaDependabotAlertIds();
    const blockingAlerts = dependabotAlerts.filter(
      (alert) => !allowedAlertIds.some((allowedId) => alert.includes(allowedId)),
    );
    if (blockingAlerts.length) {
      blockers.push(`Open production-blocking Dependabot alerts remain: ${blockingAlerts.join(", ")}.`);
    } else {
      warnings.push(`Open Dependabot alerts are limited to documented alpha-platform blockers: ${dependabotAlerts.join(", ")}.`);
    }
  } else {
    passes.push("No open GitHub Dependabot alerts.");
  }

  const releases = run("gh", ["release", "list", "--repo", repo, "--limit", "100"]);
  if (!releases.ok) {
    blockers.push("Could not read GitHub releases with gh.");
  } else if (!lineStartsWith(releases.stdout, expectedTagPrefix)) {
    blockers.push(`No GitHub release found for ${expectedTagPrefix}*. Push a ${expectedTagPrefix} tag after signing secrets are configured.`);
  } else {
    passes.push(`A GitHub release exists for ${expectedTagPrefix}*.`);
  }
}

function allowedAlphaDependabotAlertIds() {
  const release = productionEvidence?.release ?? {};
  const alphaPlatforms = new Set(Array.isArray(release.alphaPlatforms) ? release.alphaPlatforms : []);
  const productionPlatforms = new Set(Array.isArray(release.productionPlatforms) ? release.productionPlatforms : []);
  if (!alphaPlatforms.size) {
    return [];
  }

  return (Array.isArray(release.knownPlatformBlockers) ? release.knownPlatformBlockers : [])
    .filter((blocker) => alphaPlatforms.has(blocker.platform) && !productionPlatforms.has(blocker.platform))
    .map((blocker) => blocker.id)
    .filter((id) => typeof id === "string" && id.trim());
}

function checkProductionEvidence() {
  if (!existsSync(expectedEvidencePath)) {
    blockers.push(
      `Missing ${expectedEvidencePath}. After signed artifacts and manual QA exist, copy docs/production-release-evidence.example.json, fill real evidence, and run npm run release:production-gate.`,
    );
    return;
  }
  passes.push(`${expectedEvidencePath} exists. Run npm run release:production-gate for full evidence validation.`);
}

function listGhNames(args) {
  const result = run("gh", args);
  if (!result.ok) {
    return null;
  }
  return new Set(
    result.stdout
      .trim()
      .split(/\r?\n/)
      .filter(Boolean)
      .map((line) => line.split(/\s+/)[0]),
  );
}

function listCodeScanningAlerts() {
  const result = run("gh", [
    "api",
    `repos/${repo}/code-scanning/alerts?state=open`,
    "--paginate",
    "--jq",
    '.[] | "#" + (.number|tostring) + " " + .rule.id + " " + .most_recent_instance.location.path',
  ]);
  if (!result.ok) {
    return null;
  }
  return lines(result.stdout);
}

function listDependabotAlerts() {
  const result = run("gh", [
    "api",
    `repos/${repo}/dependabot/alerts?state=open`,
    "--paginate",
    "--jq",
    '.[] | "#" + (.number|tostring) + " " + .dependency.package.name + " " + .dependency.manifest_path + " " + .security_advisory.ghsa_id',
  ]);
  if (!result.ok) {
    return null;
  }
  return lines(result.stdout);
}

function lineStartsWith(value, prefix) {
  return value
    .trim()
    .split(/\r?\n/)
    .some((line) => line.startsWith(prefix));
}

function resolveRepo() {
  const repoView = run("gh", ["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"]);
  if (repoView.ok && repoView.stdout.trim()) {
    return repoView.stdout.trim();
  }

  const remote = run("git", ["remote", "get-url", "origin"]);
  if (!remote.ok) {
    return "";
  }

  const match = remote.stdout.trim().match(/github\.com[:/](?<owner>[^/]+)\/(?<name>[^/.]+)(?:\.git)?$/);
  return match?.groups ? `${match.groups.owner}/${match.groups.name}` : "";
}

function printReport() {
  console.log(`Release preflight for ${repo || "unknown repository"}`);
  console.log(`Commit: ${headSha}`);
  console.log(`Expected release tag prefix: ${expectedTagPrefix}`);
  console.log("");

  printSection("Passed", passes);
  printSection("Warnings", warnings);
  printSection("Blockers", blockers);

  if (blockers.length) {
    console.log("");
    console.log("Release preflight failed.");
    return;
  }
  console.log("");
  console.log("Release preflight passed.");
}

function printSection(title, items) {
  console.log(`${title}:`);
  if (!items.length) {
    console.log("  - None");
    return;
  }
  for (const item of items) {
    console.log(`  - ${item}`);
  }
}

function readJson(path) {
  return JSON.parse(readText(path));
}

function readText(path) {
  return readFileSync(path, "utf8");
}

function lines(value) {
  return value.trim() ? value.trim().split(/\r?\n/) : [];
}

function indent(value) {
  return value
    .split(/\r?\n/)
    .map((line) => `  ${line}`)
    .join("\n");
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    env: process.env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;
  const ok = result.status === 0;
  if (!ok && options.required) {
    throw new Error(`${command} ${args.join(" ")} failed:\n${output}`);
  }
  return {
    ok,
    output,
    status: result.status,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
}
