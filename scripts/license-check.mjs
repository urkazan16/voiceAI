#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ALLOW = new Set([
  "MIT",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "0BSD",
  "CC0-1.0",
  "Unlicense",
  "Public Domain",
  "Unicode-3.0",
  "Unicode-DFS-2016",
  "Zlib",
  "OpenSSL",
  "NCSA",
  "BSL-1.0",
  "BlueOak-1.0.0",
  "MPL-2.0",
  "CDLA-Permissive-2.0",
]);

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function licenseString(raw) {
  if (!raw) return "";
  if (typeof raw === "string") return raw;
  if (Array.isArray(raw)) return raw.map((item) => item.type ?? item).join(" OR ");
  if (typeof raw === "object" && raw.type) return String(raw.type);
  return String(raw);
}

function tokens(license) {
  return licenseString(license)
    .replace(/WITH LLVM-exception/gi, "")
    .replace(/[()]/g, " ")
    .replaceAll("/", " OR ")
    .split(/\s+(OR|AND)\s+/i)
    .map((s) => s.trim())
    .filter((s) => s && s !== "OR" && s !== "AND");
}

function allowed(license) {
  const text = licenseString(license);
  const parts = tokens(text);
  if (parts.length === 0) return false;
  if (/\bAND\b/i.test(text.replaceAll("/", " OR "))) {
    return parts.every((p) => ALLOW.has(p));
  }
  return parts.some((p) => ALLOW.has(p));
}

function failCopyleft(license, name) {
  const text = licenseString(license);
  if (/\bAGPL\b/i.test(text) || (/\bGPL\b/i.test(text) && !/\bMIT OR GPL/i.test(text))) {
    console.error(`COPYLEFT FAIL ${name}: ${text}`);
    return true;
  }
  return false;
}

const pkg = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
const declared = { ...pkg.dependencies, ...pkg.devDependencies };
for (const name of Object.keys(declared)) {
  const licensePath = path.join(root, "node_modules", name, "package.json");
  try {
    const meta = JSON.parse(readFileSync(licensePath, "utf8"));
    const license = licenseString(meta.license ?? meta.licenses);
    if (failCopyleft(license, name)) process.exit(1);
    if (!allowed(license)) {
      console.error(`LICENSE FAIL npm ${name}: ${license}`);
      process.exit(1);
    }
  } catch (error) {
    console.error(`LICENSE FAIL missing package metadata for ${name}: ${error}`);
    process.exit(1);
  }
}

const cargoBin = process.env.CARGO ?? path.join(process.env.HOME ?? "", ".cargo", "bin", "cargo");
const cargo = spawnSync(
  cargoBin,
  ["metadata", "--format-version", "1", "--manifest-path", "src-tauri/Cargo.toml"],
  {
    encoding: "utf8",
    cwd: root,
    env: {
      ...process.env,
      PATH: `${path.join(process.env.HOME ?? "", ".cargo", "bin")}${path.delimiter}${process.env.PATH ?? ""}`,
    },
    maxBuffer: 64 * 1024 * 1024,
  },
);
if (cargo.status !== 0) {
  console.error(cargo.stderr || cargo.error || "cargo metadata failed");
  process.exit(cargo.status ?? 1);
}
const metadata = JSON.parse(cargo.stdout);
for (const item of metadata.packages) {
  if (!item.source) continue;
  const license = item.license ?? "";
  const licenseFile = item.license_file ?? "";
  if (!license && !licenseFile) {
    console.error(`LICENSE FAIL cargo ${item.name}: missing license and license-file`);
    process.exit(1);
  }
  if (!license) continue;
  if (failCopyleft(license, item.name)) process.exit(1);
  if (!allowed(license)) {
    console.error(`LICENSE FAIL cargo ${item.name}: ${license}`);
    process.exit(1);
  }
}

console.log("License allowlist check passed.");
