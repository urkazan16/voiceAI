#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
  copyFileSync,
  cpSync,
  lstatSync,
  rmSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const host = process.platform;
const cargoTarget = process.env.CARGO_BUILD_TARGET?.trim() || "";

function releaseArch() {
  if (cargoTarget.startsWith("x86_64") || cargoTarget.startsWith("i686")) {
    return "x64";
  }
  if (cargoTarget.startsWith("aarch64") || cargoTarget.includes("arm64")) {
    return "arm64";
  }
  return os.arch();
}

function rustReleaseDir() {
  if (cargoTarget) {
    return path.join(root, "src-tauri/target", cargoTarget, "release");
  }
  return path.join(root, "src-tauri/target/release");
}

function run(cmd, args, opts = {}) {
  // Windows cannot spawn npm/npx shims (.cmd) without a shell; the old
  // spawnSync("npx") exited 1 with no output on package (windows-latest).
  const result = spawnSync(cmd, args, {
    stdio: "inherit",
    cwd: root,
    env: process.env,
    shell: process.platform === "win32",
    ...opts,
  });
  if (result.error) {
    console.error(`failed to start ${cmd}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    console.error(`${cmd} ${args.join(" ")} exited ${result.status ?? "null"}`);
    process.exit(result.status ?? 1);
  }
}

process.chdir(root);

const ciMacos = host === "darwin" && process.env.CI === "true";
if (ciMacos) {
  requireAppleNotarizationEnv();
}

const catalog = path.join(root, "src-tauri/resources/model-catalog.json");
if (!existsSync(catalog)) {
  console.error("missing model catalog");
  process.exit(1);
}

const skipBuild = process.env.LOCALFLOW_SKIP_BUILD === "1";
if (!skipBuild) {
  run("npx", ["tsc", "--noEmit"]);
  run("npx", ["eslint", "."]);
  run("npx", ["vite", "build"]);

  // speech.m / lock.m are compiled from build.rs on macOS only. The CMake
  // target is an INTERFACE include and is unused on Windows/Linux.
  if (host === "darwin") {
    const cmakeArgs = ["-S", "src-tauri/native", "-B", "src-tauri/native/build"];
    if (cargoTarget === "x86_64-apple-darwin") {
      cmakeArgs.push("-DCMAKE_OSX_ARCHITECTURES=x86_64");
    } else if (cargoTarget === "aarch64-apple-darwin") {
      cmakeArgs.push("-DCMAKE_OSX_ARCHITECTURES=arm64");
    }
    run("cmake", cmakeArgs);
    run("cmake", ["--build", "src-tauri/native/build"]);
  }

  if (ciMacos) {
    // Tauri CLI 2.2.x shells out to macOS `base64 --decode`, which rejects
    // wrapped GitHub secrets. CI imports the .p12 into a keychain first.
    if (process.env.KEYCHAIN_PATH) {
      delete process.env.APPLE_CERTIFICATE;
      delete process.env.APPLE_CERTIFICATE_PASSWORD;
      unlockCiSigningKeychain();
    }
  }

  // One compile: tauri build already runs cargo --release. --locked keeps CI
  // on Cargo.lock; --target is required when packaging Intel from Apple silicon.
  const bundles = host === "darwin" ? "app,dmg" : host === "win32" ? "nsis" : "deb,appimage";
  const tauriArgs = ["tauri", "build", "--bundles", bundles];
  if (cargoTarget) {
    tauriArgs.push("--target", cargoTarget);
  }
  tauriArgs.push("--", "--locked");
  run("npx", tauriArgs);
}

const version = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8")).version;
const artifacts = path.join(root, "release-artifacts");
mkdirSync(artifacts, { recursive: true });

const bundleDir = path.join(rustReleaseDir(), "bundle");
const ARTIFACT_FILE = /\.(dmg|exe|msi|deb|rpm|AppImage)$/i;

function collect(kind) {
  const dir = path.join(bundleDir, kind);
  if (!existsSync(dir)) return;
  for (const name of readdirSync(dir)) {
    const from = path.join(dir, name);
    const to = path.join(artifacts, name);
    const st = lstatSync(from);
    const take = st.isDirectory() ? name.endsWith(".app") : ARTIFACT_FILE.test(name);
    if (!take) continue;
    if (existsSync(to)) {
      rmSync(to, { recursive: true, force: true });
    }
    if (st.isDirectory()) {
      // LocalFlow.app is a directory; copyFileSync cannot copy it.
      cpSync(from, to, { recursive: true });
    } else {
      copyFileSync(from, to);
    }
  }
}
for (const kind of ["dmg", "macos", "nsis", "msi", "deb", "rpm", "appimage"]) {
  collect(kind);
}

renameInstallers(artifacts, releaseArch());
if (ciMacos) {
  notarizeAndStapleDmgs(artifacts);
}

run("node", ["scripts/generate-sbom.mjs", artifacts]);
run("node", ["scripts/write-sha256sums.mjs", artifacts]);
cpSync(path.join(root, "licenses"), path.join(artifacts, "THIRD_PARTY_LICENSES"), {
  recursive: true,
});
copyFileSync(path.join(root, "NOTICE"), path.join(artifacts, "NOTICE"));

writeFileSync(path.join(artifacts, "CHANGELOG.md"), readFileSync(path.join(root, "CHANGELOG.md")));

console.log(
  `Release artifacts for LocalFlow ${version} (${host}/${releaseArch()}) written to ${artifacts}`,
);

/** Stable names so README / GitHub Releases latest URLs do not change with the version. */
function stableInstallerName(fileName, arch) {
  const cpu = arch === "arm64" || arch === "aarch64" ? "arm64" : "x64";
  const lower = fileName.toLowerCase();
  if (lower.endsWith(".dmg")) return `LocalFlow-macos-${cpu}.dmg`;
  if (lower.endsWith(".app")) return `LocalFlow-macos-${cpu}.app`;
  if (lower.endsWith(".exe")) return `LocalFlow-windows-${cpu}.exe`;
  if (lower.endsWith(".msi")) return `LocalFlow-windows-${cpu}.msi`;
  if (lower.endsWith(".deb")) return `LocalFlow-linux-${cpu}.deb`;
  if (lower.endsWith(".appimage")) return `LocalFlow-linux-${cpu}.AppImage`;
  return null;
}

/** GitHub-hosted macOS installers must be Developer ID signed and notarized. */
function requireAppleNotarizationEnv() {
  const required = ["APPLE_SIGNING_IDENTITY", "APPLE_ID", "APPLE_PASSWORD", "APPLE_TEAM_ID"];
  const missing = required.filter((key) => !process.env[key]?.trim());
  if (missing.length === 0) {
    return;
  }
  console.error("macOS CI package refuses to ship an unsigned .dmg (Gatekeeper blocks it).");
  console.error(`Missing GitHub Actions secrets: ${missing.join(", ")}`);
  console.error("See README.md Download section.");
  process.exit(1);
}

function unlockCiSigningKeychain() {
  const keychain = process.env.KEYCHAIN_PATH?.trim();
  const password = process.env.KEYCHAIN_PASSWORD;
  if (!keychain || !password) {
    return;
  }
  const result = spawnSync("security", ["unlock-keychain", "-p", password, keychain], {
    stdio: "inherit",
    cwd: root,
    env: process.env,
  });
  if (result.status !== 0) {
    console.error("security unlock-keychain failed for the CI signing keychain");
    process.exit(result.status ?? 1);
  }
}

function stapleWithRetry(dmg) {
  const pausesSec = [0, 15, 30];
  for (let i = 0; i < pausesSec.length; i += 1) {
    if (pausesSec[i] > 0) {
      spawnSync("sleep", [String(pausesSec[i])], { stdio: "inherit" });
    }
    const result = spawnSync("xcrun", ["stapler", "staple", dmg], {
      stdio: "inherit",
      cwd: root,
      env: process.env,
    });
    if (result.status === 0) {
      return;
    }
    console.error(`stapler staple attempt ${i + 1} failed for the .dmg`);
  }
  console.error("stapler staple failed for the .dmg after retries");
  process.exit(1);
}

function runWithoutArgLog(cmd, args, failureMessage) {
  const result = spawnSync(cmd, args, {
    stdio: "inherit",
    cwd: root,
    env: process.env,
  });
  if (result.error) {
    console.error(`failed to start ${cmd}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    console.error(failureMessage);
    process.exit(result.status ?? 1);
  }
}

/** Tauri notarizes the .app; the shipped .dmg is a new container and needs its own ticket. */
function notarizeAndStapleDmgs(dir) {
  const dmgs = readdirSync(dir).filter((name) => name.toLowerCase().endsWith(".dmg"));
  if (dmgs.length === 0) {
    console.error("macOS CI package produced no .dmg to notarize");
    process.exit(1);
  }
  const appleId = process.env.APPLE_ID?.trim();
  const password = process.env.APPLE_PASSWORD;
  const teamId = process.env.APPLE_TEAM_ID?.trim();
  if (!appleId || !password || !teamId) {
    console.error("missing Apple ID credentials to notarize the .dmg");
    process.exit(1);
  }
  for (const name of dmgs) {
    const dmg = path.join(dir, name);
    runWithoutArgLog(
      "xcrun",
      [
        "notarytool",
        "submit",
        dmg,
        "--apple-id",
        appleId,
        "--password",
        password,
        "--team-id",
        teamId,
        "--wait",
      ],
      "notarytool submit failed for the .dmg",
    );
    stapleWithRetry(dmg);
    run("xcrun", ["stapler", "validate", dmg]);
  }
}

function renameInstallers(dir, arch) {
  for (const name of readdirSync(dir)) {
    const next = stableInstallerName(name, arch);
    if (!next || next === name) continue;
    const from = path.join(dir, name);
    const to = path.join(dir, next);
    if (existsSync(to)) {
      rmSync(to, { recursive: true, force: true });
    }
    const st = lstatSync(from);
    if (st.isDirectory()) {
      cpSync(from, to, { recursive: true });
      rmSync(from, { recursive: true, force: true });
    } else {
      copyFileSync(from, to);
      rmSync(from);
    }
  }
}
