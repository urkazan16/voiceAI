#!/usr/bin/env node
import { chmodSync, copyFileSync, existsSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const gitDir = path.join(root, ".git");
if (!existsSync(gitDir)) {
  process.exit(0);
}
const hooksDir = path.join(gitDir, "hooks");
mkdirSync(hooksDir, { recursive: true });
const dest = path.join(hooksDir, "pre-commit");
copyFileSync(path.join(root, ".githooks", "pre-commit"), dest);
chmodSync(dest, 0o755);
