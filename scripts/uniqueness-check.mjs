#!/usr/bin/env node
import { readFileSync, existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const reportRel = "docs/evaluation/UNIQUENESS.md";
const reportPath = path.join(root, reportRel);
const readmePath = path.join(root, "README.md");

if (!existsSync(reportPath)) {
  console.error(`UNIQUENESS FAIL missing ${reportRel}`);
  process.exit(1);
}

const report = readFileSync(reportPath, "utf8");
const readme = readFileSync(readmePath, "utf8");

for (const needle of [
  "## Verdict",
  "first-party",
  reportRel,
  "This is an engineering uniqueness report",
]) {
  if (!report.includes(needle)) {
    console.error(`UNIQUENESS FAIL report missing ${JSON.stringify(needle)}`);
    process.exit(1);
  }
}

if (!readme.includes(reportRel)) {
  console.error("UNIQUENESS FAIL README does not attach docs/evaluation/UNIQUENESS.md");
  process.exit(1);
}

console.log("Uniqueness report is attached to the solution tree.");
