#!/usr/bin/env node
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";

const dir = path.resolve(process.argv[2] ?? "release-artifacts");
const lines = [];
for (const name of readdirSync(dir)) {
  const full = path.join(dir, name);
  if (!statSync(full).isFile()) continue;
  if (name === "SHA256SUMS") continue;
  const hash = createHash("sha256").update(readFileSync(full)).digest("hex");
  lines.push(`${hash}  ${name}`);
}
writeFileSync(path.join(dir, "SHA256SUMS"), `${lines.sort().join("\n")}\n`);
console.log("SHA256SUMS written");
