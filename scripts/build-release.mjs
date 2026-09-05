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
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function run(cmd, args, opts = {}) {
  const result = spawnSync(cmd, args, { stdio: "inherit", cwd: root, ...opts });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

process.chdir(root);

run("npx", ["tsc", "--noEmit"]);
run("npx", ["eslint", "."]);
run("npx", ["vite", "build"]);
run("cmake", ["-S", "src-tauri/native", "-B", "src-tauri/native/build"]);
run("cmake", ["--build", "src-tauri/native/build"]);

const catalog = path.join(root, "src-tauri/resources/model-catalog.json");
if (!existsSync(catalog)) {
  console.error("missing model catalog");
  process.exit(1);
}

run("cargo", ["build", "--manifest-path", "src-tauri/Cargo.toml", "--locked", "--release"]);
run("npx", ["tauri", "build", "--bundles", "app,dmg"]);

const version = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8")).version;
const artifacts = path.join(root, "release-artifacts");
mkdirSync(artifacts, { recursive: true });

const bundleDir = path.join(root, "src-tauri/target/release/bundle");
function collect(kind) {
  const dir = path.join(bundleDir, kind);
  if (!existsSync(dir)) return;
  for (const name of readdirSync(dir)) {
    copyFileSync(path.join(dir, name), path.join(artifacts, name));
  }
}
collect("dmg");
collect("macos");

run("node", ["scripts/generate-sbom.mjs", artifacts]);
run("node", ["scripts/write-sha256sums.mjs", artifacts]);
cpSync(path.join(root, "licenses"), path.join(artifacts, "THIRD_PARTY_LICENSES"), {
  recursive: true,
});
copyFileSync(path.join(root, "NOTICE"), path.join(artifacts, "NOTICE"));

writeFileSync(path.join(artifacts, "CHANGELOG.md"), readFileSync(path.join(root, "CHANGELOG.md")));

console.log(`Release artifacts for LocalFlow ${version} written to ${artifacts}`);
