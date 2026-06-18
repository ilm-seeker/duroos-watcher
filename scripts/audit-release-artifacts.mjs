import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";

const suffixes = [".dmg", ".app.tar.gz", ".msi", ".exe", ".AppImage", ".deb"];
const args = new Map(
  process.argv.slice(2).map((arg) => {
    const [key, ...value] = arg.replace(/^--/, "").split("=");
    return [key, value.join("=") || "true"];
  }),
);
const root = args.get("root") ?? "src-tauri/target";
const checksumOut = args.get("out") ?? "artifact-checksums.txt";
const auditOut = args.get("json") ?? "artifact-audit.json";

const failures = [];

if (!existsSync(root)) {
  failures.push(`Artifact root does not exist: ${root}`);
}

const artifactPaths = [];
const walk = (dir) => {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      walk(path);
    } else if (suffixes.some((suffix) => path.endsWith(suffix))) {
      artifactPaths.push(path);
    }
  }
};

if (!failures.length) {
  walk(root);
}

artifactPaths.sort();

if (!artifactPaths.length) {
  failures.push(`No release artifacts found under ${root}.`);
}

const artifacts = artifactPaths.map((path) => {
  const bytes = readFileSync(path);
  const sizeBytes = bytes.length;
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  if (sizeBytes <= 0) {
    failures.push(`Artifact is empty: ${path}`);
  }
  return {
    path: relative(process.cwd(), path),
    sizeBytes,
    sha256,
  };
});

if (failures.length) {
  console.error(failures.map((failure) => `failure: ${failure}`).join("\n"));
  process.exit(1);
}

writeFileSync(
  checksumOut,
  `${artifacts.map((artifact) => `${artifact.sha256}  ${artifact.path}`).join("\n")}\n`,
);
writeFileSync(
  auditOut,
  `${JSON.stringify(
    {
      generatedAt: new Date().toISOString(),
      artifactRoot: root,
      artifacts,
    },
    null,
    2,
  )}\n`,
);

console.log(`Audited ${artifacts.length} release artifact(s).`);
