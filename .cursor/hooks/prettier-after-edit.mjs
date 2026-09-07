#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";

const allowed = new Set([
  ".md",
  ".json",
  ".js",
  ".mjs",
  ".cjs",
  ".ts",
  ".tsx",
  ".yml",
  ".yaml",
  ".css",
  ".html",
]);

let raw = "";
process.stdin.setEncoding("utf8");
for await (const chunk of process.stdin) {
  raw += chunk;
}

let file;
try {
  const event = JSON.parse(raw || "{}");
  file = event.file_path || event.filePath || event.path;
} catch {
  process.stdout.write("{}\n");
  process.exit(0);
}

if (file && existsSync(file) && allowed.has(path.extname(file))) {
  spawnSync("npx", ["prettier", "--write", "--", file], {
    stdio: "ignore",
    shell: process.platform === "win32",
  });
}

process.stdout.write("{}\n");
