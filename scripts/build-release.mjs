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

function run(cmd, args, opts = {}) {
  const result = spawnSync(cmd, args, { stdio: "inherit", cwd: root, ...opts });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

process.chdir(root);

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
    run("cmake", ["-S", "src-tauri/native", "-B", "src-tauri/native/build"]);
    run("cmake", ["--build", "src-tauri/native/build"]);
  }

  run("cargo", ["build", "--manifest-path", "src-tauri/Cargo.toml", "--locked", "--release"]);

  const bundles = host === "darwin" ? "app,dmg" : host === "win32" ? "nsis" : "deb,appimage";
  run("npx", ["tauri", "build", "--bundles", bundles]);
}

const version = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8")).version;
const artifacts = path.join(root, "release-artifacts");
mkdirSync(artifacts, { recursive: true });

const bundleDir = path.join(root, "src-tauri/target/release/bundle");
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

renameInstallers(artifacts, os.arch());

run("node", ["scripts/generate-sbom.mjs", artifacts]);
run("node", ["scripts/write-sha256sums.mjs", artifacts]);
cpSync(path.join(root, "licenses"), path.join(artifacts, "THIRD_PARTY_LICENSES"), {
  recursive: true,
});
copyFileSync(path.join(root, "NOTICE"), path.join(artifacts, "NOTICE"));

writeFileSync(path.join(artifacts, "CHANGELOG.md"), readFileSync(path.join(root, "CHANGELOG.md")));

console.log(
  `Release artifacts for LocalFlow ${version} (${host}/${os.arch()}) written to ${artifacts}`,
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
