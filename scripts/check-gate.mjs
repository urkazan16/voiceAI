#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const npm = process.platform === "win32" ? "npm.cmd" : "npm";

const steps = [
  ["quality", npm, ["run", "check:js"]],
  ["license", npm, ["run", "license:check"]],
  ["license", npm, ["run", "uniqueness:check"]],
  ["security", npm, ["run", "check:security"]],
];

for (const [job, cmd, args] of steps) {
  console.log(`\n==> ${job}: ${cmd} ${args.join(" ")}`);
  const result = spawnSync(cmd, args, { stdio: "inherit", cwd: root, env: process.env });
  if (result.status !== 0) {
    console.error(`\ncheck:gate failed on CI job "${job}".`);
    process.exit(result.status ?? 1);
  }
}

const cargoEnv = {
  ...process.env,
  PATH: `${path.join(process.env.HOME ?? "", ".cargo", "bin")}${path.delimiter}${process.env.PATH ?? ""}`,
};
const cargo = process.env.CARGO ?? "cargo";
const auditAvailable = spawnSync(cargo, ["audit", "-V"], {
  encoding: "utf8",
  cwd: root,
  env: cargoEnv,
});
if (auditAvailable.status !== 0) {
  console.warn("cargo-audit is not installed; CI security still runs it.");
} else {
  const cargoAudit = spawnSync(cargo, ["audit", "--file", "src-tauri/Cargo.lock"], {
    stdio: "inherit",
    cwd: root,
    env: cargoEnv,
  });
  if (cargoAudit.status !== 0) {
    console.error('\ncheck:gate failed on CI job "security" (cargo audit).');
    process.exit(cargoAudit.status ?? 1);
  }
}

console.log("\ncheck:gate passed (quality, license, security).");
