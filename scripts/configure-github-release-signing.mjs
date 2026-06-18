import { spawnSync } from "node:child_process";

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

const requiredVariables = ["WINDOWS_CERTIFICATE_THUMBPRINT", "WINDOWS_TIMESTAMP_URL"];

const args = new Map(
  process.argv.slice(2).map((arg) => {
    const [key, ...value] = arg.replace(/^--/, "").split("=");
    return [key, value.join("=") || "true"];
  }),
);

if (args.has("help")) {
  printUsage();
  process.exit(0);
}

const dryRun = args.has("dry-run");
const repo = args.get("repo") ?? resolveRepo();
const failures = [];

if (!repo) {
  failures.push("Could not resolve GitHub repository. Pass --repo=OWNER/REPO.");
}

const secretValues = new Map(requiredSecrets.map((name) => [name, requireEnv(name)]));
const variableValues = new Map(requiredVariables.map((name) => [name, requireEnv(name)]));

validateBase64Secret("APPLE_CERTIFICATE", secretValues.get("APPLE_CERTIFICATE"));
validateBase64Secret("WINDOWS_CERTIFICATE", secretValues.get("WINDOWS_CERTIFICATE"));

const windowsThumbprint = variableValues.get("WINDOWS_CERTIFICATE_THUMBPRINT")?.replace(/\s+/g, "").toUpperCase();
if (!/^[A-F0-9]{40}$/.test(windowsThumbprint ?? "")) {
  failures.push("WINDOWS_CERTIFICATE_THUMBPRINT must be a 40-character SHA-1 certificate thumbprint.");
}
variableValues.set("WINDOWS_CERTIFICATE_THUMBPRINT", windowsThumbprint ?? "");

const timestampUrl = variableValues.get("WINDOWS_TIMESTAMP_URL")?.trim() ?? "";
try {
  const parsedTimestampUrl = new URL(timestampUrl);
  if (parsedTimestampUrl.protocol !== "https:") {
    failures.push("WINDOWS_TIMESTAMP_URL must be an HTTPS URL.");
  }
} catch {
  failures.push("WINDOWS_TIMESTAMP_URL must be a valid HTTPS URL.");
}
variableValues.set("WINDOWS_TIMESTAMP_URL", timestampUrl);

if (failures.length) {
  fail(failures);
}

console.log(`Target repository: ${repo}`);

if (dryRun) {
  console.log("Dry run only. No GitHub secrets or variables were changed.");
  console.log(`Validated secret env vars: ${requiredSecrets.join(", ")}`);
  console.log(`Validated variable env vars: ${requiredVariables.join(", ")}`);
  process.exit(0);
}

for (const [name, value] of secretValues) {
  runGh(["secret", "set", name, "--repo", repo], value, `set GitHub Actions secret ${name}`);
  console.log(`Set GitHub Actions secret ${name}.`);
}

for (const [name, value] of variableValues) {
  runGh(["variable", "set", name, "--repo", repo], value, `set GitHub Actions variable ${name}`);
  console.log(`Set GitHub Actions variable ${name}.`);
}

verifyNames("secret", requiredSecrets);
verifyNames("variable", requiredVariables);

console.log("GitHub release signing inputs are configured.");

function requireEnv(name) {
  const value = process.env[name];
  if (typeof value !== "string" || value.trim() === "") {
    failures.push(`${name} is required in the environment.`);
    return "";
  }
  return value;
}

function validateBase64Secret(name, value) {
  const compactValue = value?.replace(/\s+/g, "") ?? "";
  if (!compactValue) {
    return;
  }

  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(compactValue) || compactValue.length % 4 !== 0) {
    failures.push(`${name} must be a base64-encoded certificate payload.`);
  }
}

function verifyNames(kind, expectedNames) {
  const result = runGh([kind, "list", "--repo", repo], "", `list GitHub Actions ${kind}s`);
  const existingNames = new Set(
    result.stdout
      .trim()
      .split(/\r?\n/)
      .filter(Boolean)
      .map((line) => line.split(/\s+/)[0]),
  );
  const missingNames = expectedNames.filter((name) => !existingNames.has(name));
  if (missingNames.length) {
    fail([`GitHub did not report configured ${kind}s: ${missingNames.join(", ")}.`]);
  }
}

function resolveRepo() {
  const repoView = spawnSync("gh", ["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"], {
    encoding: "utf8",
  });
  if (repoView.status === 0 && repoView.stdout.trim()) {
    return repoView.stdout.trim();
  }

  const remote = spawnSync("git", ["remote", "get-url", "origin"], { encoding: "utf8" });
  if (remote.status !== 0) {
    return "";
  }

  const match = remote.stdout.trim().match(/github\.com[:/](?<owner>[^/]+)\/(?<name>[^/.]+)(?:\.git)?$/);
  return match?.groups ? `${match.groups.owner}/${match.groups.name}` : "";
}

function runGh(ghArgs, input, description) {
  const result = spawnSync("gh", ghArgs, {
    input,
    encoding: "utf8",
    maxBuffer: 1024 * 1024,
  });
  if (result.status !== 0) {
    fail([`Could not ${description}.\n${indent((result.stderr || result.stdout).trim())}`]);
  }
  return result;
}

function fail(messages) {
  console.error(messages.map((message) => `failure: ${message}`).join("\n"));
  process.exit(1);
}

function indent(value) {
  return value
    .split(/\r?\n/)
    .map((line) => `  ${line}`)
    .join("\n");
}

function printUsage() {
  console.log(`Usage: npm run release:configure-signing -- [--repo=OWNER/REPO] [--dry-run]

Reads the required signing values from environment variables, then writes the GitHub Actions secrets
and variables used by .github/workflows/release.yml.

Required secret env vars:
  ${requiredSecrets.join("\n  ")}

Required variable env vars:
  ${requiredVariables.join("\n  ")}
`);
}
