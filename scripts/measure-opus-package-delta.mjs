#!/usr/bin/env node
import {
  appendFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const [action, ...args] = process.argv.slice(2);

function releaseArch() {
  const target = process.env.CARGO_BUILD_TARGET?.trim() || "";
  if (target.startsWith("x86_64") || target.startsWith("i686")) return "x64";
  if (target.startsWith("aarch64") || target.includes("arm64")) return "arm64";
  return os.arch();
}

function releaseDir() {
  const target = process.env.CARGO_BUILD_TARGET?.trim();
  return path.join(root, "src-tauri/target", ...(target ? [target] : []), "release");
}

function binaryPath() {
  const dir = releaseDir();
  if (process.platform === "darwin") {
    return path.join(dir, "bundle/macos/LocalFlow.app/Contents/MacOS/localflow");
  }
  return path.join(dir, process.platform === "win32" ? "localflow.exe" : "localflow");
}

function expectedInstallers() {
  const arch = releaseArch();
  if (process.platform === "darwin") return [`LocalFlow-macos-${arch}.dmg`];
  if (process.platform === "win32") return [`LocalFlow-windows-${arch}.exe`];
  const appImageArch = arch === "arm64" ? "aarch64" : "x86_64";
  return [`LocalFlow-linux-${arch}.deb`, `LocalFlow-${appImageArch}.AppImage`];
}

function snapshot(variant, output) {
  const artifacts = path.join(root, "release-artifacts");
  const executable = binaryPath();
  if (!existsSync(executable)) throw new Error(`release executable is missing: ${executable}`);
  const installers = {};
  for (const name of expectedInstallers()) {
    const file = path.join(artifacts, name);
    if (!existsSync(file)) throw new Error(`installer is missing: ${file}`);
    installers[name] = statSync(file).size;
  }
  const metricsFile = path.join(artifacts, "opus-runtime-metrics.json");
  const result = {
    variant,
    platform: process.platform,
    arch: releaseArch(),
    binaryBytes: statSync(executable).size,
    installers,
    runtime: existsSync(metricsFile) ? JSON.parse(readFileSync(metricsFile, "utf8")) : null,
  };
  mkdirSync(path.dirname(output), { recursive: true });
  writeFileSync(output, `${JSON.stringify(result, null, 2)}\n`);
  console.log(`Saved ${variant} release snapshot to ${output}`);
}

function signed(value) {
  return value > 0 ? `+${value}` : String(value);
}

function compare(defaultFile, candidateFile, outputBase) {
  const baseline = JSON.parse(readFileSync(defaultFile, "utf8"));
  const candidate = JSON.parse(readFileSync(candidateFile, "utf8"));
  if (baseline.platform !== candidate.platform || baseline.arch !== candidate.arch) {
    throw new Error("release snapshots are from different targets");
  }
  const rows = [];
  const add = (name, before, after) => {
    const delta = after - before;
    rows.push({
      name,
      defaultBytes: before,
      opusBytes: after,
      deltaBytes: delta,
      deltaPercent: (delta / before) * 100,
    });
  };
  add("executable", baseline.binaryBytes, candidate.binaryBytes);
  for (const [name, before] of Object.entries(baseline.installers)) {
    if (!(name in candidate.installers)) throw new Error(`candidate is missing installer ${name}`);
    add(name, before, candidate.installers[name]);
  }
  const report = {
    platform: baseline.platform,
    arch: baseline.arch,
    rows,
    opusRuntime: candidate.runtime,
  };
  const markdown = [
    `# Ogg/Opus package delta — ${report.platform}/${report.arch}`,
    "",
    "| Artifact | Default bytes | Opus bytes | Delta bytes | Delta |",
    "| --- | ---: | ---: | ---: | ---: |",
    ...rows.map(
      (row) =>
        `| ${row.name} | ${row.defaultBytes} | ${row.opusBytes} | ${signed(row.deltaBytes)} | ${signed(row.deltaPercent.toFixed(3))}% |`,
    ),
    "",
    candidate.runtime
      ? `Packaged decode: ${candidate.runtime.decode_ms.toFixed(3)} ms; peak RSS: ${candidate.runtime.peak_rss_bytes} bytes.`
      : "Packaged decode metrics unavailable.",
    "",
  ].join("\n");
  writeFileSync(`${outputBase}.json`, `${JSON.stringify(report, null, 2)}\n`);
  writeFileSync(`${outputBase}.md`, markdown);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, markdown);
  console.log(markdown);
}

if (action === "snapshot" && args.length === 2) {
  snapshot(args[0], path.resolve(args[1]));
} else if (action === "compare" && args.length === 3) {
  compare(path.resolve(args[0]), path.resolve(args[1]), path.resolve(args[2]));
} else {
  console.error(
    "usage: measure-opus-package-delta.mjs snapshot VARIANT OUTPUT | compare DEFAULT_JSON OPUS_JSON OUTPUT_BASE",
  );
  process.exit(2);
}
