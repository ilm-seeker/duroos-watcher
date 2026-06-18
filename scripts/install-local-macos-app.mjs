import { spawnSync } from "node:child_process";
import { existsSync, renameSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceApp = resolve(repoRoot, "src-tauri/target/release/bundle/macos/Duroos Watcher.app");
const targetApp = resolve(process.env.DUROOS_WATCHER_INSTALL_PATH ?? "/Applications/Duroos Watcher.app");
const tempApp = `${targetApp}.installing-${Date.now()}`;

if (process.platform !== "darwin") {
  fail("Local app installation is only supported on macOS.");
}

if (!existsSync(sourceApp)) {
  fail(`Build artifact is missing: ${sourceApp}\nRun npm run tauri:build:app first.`);
}

const runningApp = spawnSync("pgrep", ["-x", "Duroos Watcher"], { encoding: "utf8" });
if (runningApp.status === 0) {
  fail("Duroos Watcher is running. Quit it before replacing the app in /Applications.");
}

if (runningApp.error) {
  fail(`Could not check whether Duroos Watcher is running: ${runningApp.error.message}`);
}

rmSync(tempApp, { force: true, recursive: true });

const copyResult = spawnSync("ditto", [sourceApp, tempApp], { encoding: "utf8" });
if (copyResult.status !== 0) {
  rmSync(tempApp, { force: true, recursive: true });
  fail(`Could not copy app bundle with ditto:\n${copyResult.stderr || copyResult.stdout}`);
}

const signResult = spawnSync("codesign", ["--force", "--deep", "--sign", "-", tempApp], {
  encoding: "utf8",
});
if (signResult.status !== 0) {
  rmSync(tempApp, { force: true, recursive: true });
  fail(`Could not ad-hoc sign app bundle:\n${signResult.stderr || signResult.stdout}`);
}

try {
  rmSync(targetApp, { force: true, recursive: true });
  renameSync(tempApp, targetApp);
} catch (error) {
  rmSync(tempApp, { force: true, recursive: true });
  fail(`Could not replace ${targetApp}: ${error.message}`);
}

console.log(`Installed and ad-hoc signed ${targetApp}`);

function fail(message) {
  console.error(message);
  process.exit(1);
}
