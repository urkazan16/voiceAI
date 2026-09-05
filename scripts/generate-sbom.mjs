#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outDir = process.argv[2]
  ? path.resolve(process.argv[2])
  : path.join(root, "release-artifacts");
mkdirSync(outDir, { recursive: true });

const npm = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
const cargoEnv = {
  ...process.env,
  PATH: `${path.join(process.env.HOME ?? "", ".cargo", "bin")}${path.delimiter}${process.env.PATH ?? ""}`,
};
const cargo = spawnSync(
  "cargo",
  ["metadata", "--format-version", "1", "--manifest-path", "src-tauri/Cargo.toml"],
  {
    encoding: "utf8",
    cwd: root,
    env: cargoEnv,
    maxBuffer: 64 * 1024 * 1024,
  },
);
if (cargo.status !== 0) {
  process.exit(cargo.status ?? 1);
}
const metadata = JSON.parse(cargo.stdout);

const components = [
  {
    name: npm.name,
    version: npm.version,
    license: "MIT",
    source: "https://github.com/localflow/localflow",
    checksum: "see SHA256SUMS for packaged binaries",
  },
];

for (const [name, version] of Object.entries({ ...npm.dependencies, ...npm.devDependencies })) {
  components.push({
    name,
    version,
    license: "see npm license scan",
    source: `https://www.npmjs.com/package/${name}`,
    checksum: "lockfile: package-lock.json",
  });
}

for (const pkg of metadata.packages ?? []) {
  if (pkg.source) {
    components.push({
      name: pkg.name,
      version: pkg.version,
      license: pkg.license ?? "UNKNOWN",
      source: pkg.source,
      checksum: "lockfile: src-tauri/Cargo.lock",
    });
  }
}

const sbom = {
  bomFormat: "CycloneDX",
  specVersion: "1.5",
  version: 1,
  metadata: {
    timestamp: new Date().toISOString(),
    component: { name: "LocalFlow", version: npm.version, type: "application" },
  },
  components: components.map((c) => ({
    type: "library",
    name: c.name,
    version: c.version,
    licenses: [{ license: { id: c.license } }],
    source: c.source,
    hashes: [{ alg: "SHA-256", content: c.checksum }],
  })),
};

writeFileSync(path.join(outDir, "sbom.cdx.json"), JSON.stringify(sbom, null, 2));
console.log(`SBOM written to ${path.join(outDir, "sbom.cdx.json")}`);
