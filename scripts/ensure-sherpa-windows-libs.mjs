#!/usr/bin/env node
import { existsSync, lstatSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SHERPA_VERSION = "1.13.8";
const ARCHIVE_STEM = `sherpa-onnx-v${SHERPA_VERSION}-win-x64-shared-MT-Release-lib`;
const ARCHIVE_NAME = `${ARCHIVE_STEM}.tar.bz2`;
const DOWNLOAD_URL = `https://github.com/k2-fsa/sherpa-onnx/releases/download/v${SHERPA_VERSION}/${ARCHIVE_NAME}`;
const IMPORT_LIB = "sherpa-onnx-c-api.lib";

export function ensureSherpaWindowsImportLibs() {
  if (process.platform !== "win32") {
    return;
  }
  const cacheRoot = path.join(process.cwd(), "src-tauri", "target", "sherpa-onnx-prebuilt");
  if (findNamedFile(cacheRoot, IMPORT_LIB)) {
    return;
  }
  mkdirSync(cacheRoot, { recursive: true });
  const archivePath = path.join(cacheRoot, ARCHIVE_NAME);
  const extractedDir = path.join(cacheRoot, ARCHIVE_STEM);
  rmSync(extractedDir, { recursive: true, force: true });
  const curl = spawnSync("curl", ["-fsSL", "--retry", "3", "-o", archivePath, DOWNLOAD_URL], {
    stdio: "inherit",
    env: process.env,
    shell: false,
  });
  if (curl.status !== 0) {
    console.warn(`ensure-sherpa-windows-libs: could not download ${DOWNLOAD_URL}`);
    return;
  }
  // Windows tar.exe often cannot read .bz2; Python's tarfile can.
  const python = spawnSync(
    "python",
    [
      "-c",
      "import tarfile,sys; tarfile.open(sys.argv[1],'r:bz2').extractall(sys.argv[2])",
      archivePath,
      cacheRoot,
    ],
    { stdio: "inherit", env: process.env, shell: false },
  );
  if (python.status !== 0 || !findNamedFile(cacheRoot, IMPORT_LIB)) {
    rmSync(extractedDir, { recursive: true, force: true });
    console.warn(
      "ensure-sherpa-windows-libs: extract did not produce sherpa-onnx-c-api.lib; localflow build.rs will retry",
    );
  }
}

function findNamedFile(dir, name, depth = 0) {
  if (!existsSync(dir) || depth > 10) {
    return false;
  }
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    return false;
  }
  const want = name.toLowerCase();
  for (const entry of entries) {
    const full = path.join(dir, entry);
    let st;
    try {
      st = lstatSync(full);
    } catch {
      continue;
    }
    if (st.isDirectory()) {
      if (findNamedFile(full, name, depth + 1)) {
        return true;
      }
    } else if (entry.toLowerCase() === want) {
      return true;
    }
  }
  return false;
}

const thisFile = fileURLToPath(import.meta.url);
if (process.argv[1] && path.resolve(process.argv[1]) === thisFile) {
  ensureSherpaWindowsImportLibs();
}
