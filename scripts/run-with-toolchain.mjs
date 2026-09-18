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
// Passing an argument array through a shell makes paths containing spaces split
// again (for example "Application Support/.../audio.wav"). Invoke executables
// directly. Tauri's npm entry point is JavaScript, which also avoids the `.cmd`
// launcher requirement on Windows.
const executable = cmd === "tauri" ? process.execPath : cmd;
const commandArgs =
  cmd === "tauri"
    ? [path.join(process.cwd(), "node_modules", "@tauri-apps", "cli", "tauri.js"), ...rest]
    : rest;
const result = spawnSync(executable, commandArgs, {
  stdio: "inherit",
  env: process.env,
  shell: false,
});
process.exit(result.status ?? 1);
