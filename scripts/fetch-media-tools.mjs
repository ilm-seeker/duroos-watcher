import { createHash } from "node:crypto";
import { createWriteStream, mkdirSync, readdirSync, rmSync, statSync } from "node:fs";
import { chmod, copyFile, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { get } from "node:https";
import { tmpdir } from "node:os";
import { basename, dirname, join, relative } from "node:path";
import { spawnSync } from "node:child_process";

const manifestPath = "src-tauri/binaries/media-tools.manifest.json";
const args = new Map(
  process.argv.slice(2).map((arg) => {
    const [key, ...value] = arg.replace(/^--/, "").split("=");
    return [key, value.join("=") || "true"];
  }),
);

const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const target = args.get("target") ?? hostTarget();
const outputRoot = args.get("out") ?? join("src-tauri", "binaries", "vendor");
const targetOutputDir = join(outputRoot, target);
const reportPath = args.get("report") ?? join(targetOutputDir, "media-tools-report.json");
const requiredTools = manifest.requiredTools ?? [];
const artifacts = (manifest.artifacts ?? []).filter((artifact) => artifact.target === target);
const artifactByTool = new Map(artifacts.map((artifact) => [artifact.tool, artifact]));
const failures = [];

for (const tool of requiredTools) {
  if (!artifactByTool.has(tool)) {
    failures.push(`Missing manifest artifact for ${tool} on ${target}.`);
  }
}

if (failures.length) {
  fail(failures);
}

mkdirSync(targetOutputDir, { recursive: true });
const scratchDir = await mkdtemp(join(tmpdir(), "duroos-media-tools-"));
const stagedArtifacts = [];

try {
  for (const tool of requiredTools) {
    const artifact = artifactByTool.get(tool);
    const archivePath = join(scratchDir, `${tool}-${basename(new URL(artifact.sourceUrl).pathname)}`);

    await download(artifact.sourceUrl, archivePath);
    const actualSha256 = await sha256File(archivePath);
    if (actualSha256 !== artifact.sha256) {
      fail([
        `${tool} checksum mismatch for ${target}.`,
        `Expected ${artifact.sha256}.`,
        `Actual   ${actualSha256}.`,
      ]);
    }

    const outputName = executableName(tool, target);
    const outputPath = join(targetOutputDir, outputName);
    if (artifact.sourceUrl.endsWith(".tgz")) {
      await extractExecutableFromTarball(archivePath, scratchDir, tool, outputPath);
    } else {
      await copyFile(archivePath, outputPath);
    }
    await chmod(outputPath, target.includes("windows") ? 0o644 : 0o755);

    const outputStat = statSync(outputPath);
    if (!outputStat.isFile() || outputStat.size <= 0) {
      fail([`Staged media tool is missing or empty: ${outputPath}`]);
    }

    stagedArtifacts.push({
      tool,
      target,
      version: artifact.version,
      sourceUrl: artifact.sourceUrl,
      sha256: actualSha256,
      license: artifact.license,
      path: relative(process.cwd(), outputPath),
      sizeBytes: outputStat.size,
    });
  }
} finally {
  rmSync(scratchDir, { recursive: true, force: true });
}

await writeFile(
  reportPath,
  `${JSON.stringify(
    {
      generatedAt: new Date().toISOString(),
      manifestPath,
      target,
      outputDir: relative(process.cwd(), targetOutputDir),
      artifacts: stagedArtifacts,
    },
    null,
    2,
  )}\n`,
);

console.log(`Fetched and verified ${stagedArtifacts.length} media tool(s) for ${target}.`);
console.log(`Report: ${relative(process.cwd(), reportPath)}`);

function hostTarget() {
  const platform = process.platform;
  const arch = process.arch;
  if (platform === "darwin" && arch === "arm64") {
    return "aarch64-apple-darwin";
  }
  if (platform === "darwin" && arch === "x64") {
    return "x86_64-apple-darwin";
  }
  if (platform === "win32" && arch === "x64") {
    return "x86_64-pc-windows-msvc";
  }
  if (platform === "linux" && arch === "x64") {
    return "x86_64-unknown-linux-gnu";
  }
  throw new Error(`No default media-tool target for ${platform}/${arch}; pass --target.`);
}

async function download(url, destination) {
  await new Promise((resolve, reject) => {
    const request = get(url, (response) => {
      if (
        response.statusCode &&
        [301, 302, 303, 307, 308].includes(response.statusCode) &&
        response.headers.location
      ) {
        response.resume();
        download(new URL(response.headers.location, url).toString(), destination)
          .then(resolve)
          .catch(reject);
        return;
      }
      if (response.statusCode !== 200) {
        response.resume();
        reject(new Error(`${url}: HTTP ${response.statusCode}`));
        return;
      }
      const writer = createWriteStream(destination);
      response.pipe(writer);
      writer.on("finish", resolve);
      writer.on("error", reject);
    });
    request.on("error", reject);
  });
}

async function sha256File(path) {
  const hash = createHash("sha256");
  const bytes = await readFile(path);
  hash.update(bytes);
  return hash.digest("hex");
}

async function extractExecutableFromTarball(archivePath, scratchDir, tool, outputPath) {
  const extractDir = join(scratchDir, `${tool}-extract`);
  mkdirSync(extractDir, { recursive: true });
  const result = spawnSync("tar", ["-xzf", archivePath, "-C", extractDir], {
    encoding: "utf8",
  });
  if (result.status !== 0) {
    fail([`Could not extract ${archivePath}: ${result.stderr || result.stdout}`]);
  }

  const executablePath = findExtractedExecutable(extractDir, tool);
  if (!executablePath) {
    fail([`Could not find ${tool} executable inside ${archivePath}.`]);
  }

  mkdirSync(dirname(outputPath), { recursive: true });
  await copyFile(executablePath, outputPath);
}

function findExtractedExecutable(root, tool) {
  const names = new Set([tool, `${tool}.exe`]);
  const queue = [root];
  while (queue.length) {
    const directory = queue.shift();
    for (const entry of readdirSync(directory)) {
      const path = join(directory, entry);
      const stat = statSync(path);
      if (stat.isDirectory()) {
        queue.push(path);
      } else if (names.has(entry) && stat.size > 0) {
        return path;
      }
    }
  }
  return null;
}

function executableName(tool, target) {
  return target.includes("windows") ? `${tool}.exe` : tool;
}

function fail(messages) {
  console.error(messages.map((message) => `failure: ${message}`).join("\n"));
  process.exit(1);
}
