#!/usr/bin/env node
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";

const dir = path.resolve(process.argv[2] ?? "release-artifacts");
const lines = [];
function addFile(full, displayedName) {
  const hash = createHash("sha256").update(readFileSync(full)).digest("hex");
  lines.push(`${hash}  ${displayedName}`);
}

for (const name of readdirSync(dir)) {
  const full = path.join(dir, name);
  if (!statSync(full).isFile()) continue;
  if (name === "SHA256SUMS" || name.startsWith(".")) continue;
  addFile(full, name);
}

const licenseDir = path.join(dir, "THIRD_PARTY_LICENSES");
for (const name of readdirSync(licenseDir)) {
  const full = path.join(licenseDir, name);
  if (statSync(full).isFile() && !name.startsWith(".")) {
    addFile(full, `THIRD_PARTY_LICENSES/${name}`);
  }
}
writeFileSync(path.join(dir, "SHA256SUMS"), `${lines.sort().join("\n")}\n`);
console.log("SHA256SUMS written");
