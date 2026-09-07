#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";

const cargoBin = path.join(os.homedir(), ".cargo", "bin");
process.env.PATH = `${cargoBin}${path.delimiter}${process.env.PATH}`;

const args = process.argv.slice(2);
if (args.length === 0) {
  console.error("usage: run-with-toolchain.mjs <command> [args...]");
  process.exit(2);
}

const [cmd, ...rest] = args;
const result = spawnSync(cmd, rest, {
  stdio: "inherit",
  env: process.env,
  shell: true,
});
process.exit(result.status ?? 1);
