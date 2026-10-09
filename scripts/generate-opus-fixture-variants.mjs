import { Buffer } from "node:buffer";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const fixtureDir = join(
  dirname(fileURLToPath(import.meta.url)),
  "..",
  "tests",
  "fixtures",
  "ogg_opus",
);
const source = readFileSync(join(fixtureDir, "mono-997hz-1s.opus"));

function oggCrc(bytes) {
  let crc = 0;
  for (const byte of bytes) {
    crc = (crc ^ (byte << 24)) >>> 0;
    for (let bit = 0; bit < 8; bit += 1) {
      crc =
        crc & 0x80000000
          ? ((crc << 1) ^ 0x04c11db7) >>> 0
          : (crc << 1) >>> 0;
    }
  }
  return crc;
}

function pageRanges(bytes) {
  const ranges = [];
  let offset = 0;
  while (offset < bytes.length) {
    if (bytes.subarray(offset, offset + 4).toString() !== "OggS") {
      throw new Error(`invalid Ogg capture pattern at ${offset}`);
    }
    const segments = bytes[offset + 26];
    const headerEnd = offset + 27 + segments;
    let bodyLength = 0;
    for (let index = offset + 27; index < headerEnd; index += 1) {
      bodyLength += bytes[index];
    }
    ranges.push([offset, headerEnd + bodyLength]);
    offset = headerEnd + bodyLength;
  }
  return ranges;
}

function updatePageCrc(bytes, [start, end]) {
  bytes.fill(0, start + 22, start + 26);
  bytes.writeUInt32LE(oggCrc(bytes.subarray(start, end)), start + 22);
}

function writeHeaderVariant(name, edit) {
  const bytes = Buffer.from(source);
  const firstPage = pageRanges(bytes)[0];
  const bodyStart = firstPage[0] + 27 + bytes[firstPage[0] + 26];
  if (bytes.subarray(bodyStart, bodyStart + 8).toString() !== "OpusHead") {
    throw new Error("source fixture has no OpusHead packet");
  }
  edit(bytes, bodyStart);
  updatePageCrc(bytes, firstPage);
  writeFileSync(join(fixtureDir, name), bytes);
}

writeHeaderVariant("mono-997hz-1s-gain-plus-6db.opus", (bytes, offset) => {
  bytes.writeInt16LE(6 * 256, offset + 16);
});
writeHeaderVariant("mono-997hz-1s-gain-minus-6db.opus", (bytes, offset) => {
  bytes.writeInt16LE(-6 * 256, offset + 16);
});
writeHeaderVariant("mono-997hz-1s-input-rate-44100.opus", (bytes, offset) => {
  bytes.writeUInt32LE(44_100, offset + 12);
});

{
  const bytes = Buffer.from(source);
  const ranges = pageRanges(bytes);
  for (const range of ranges.slice(2)) {
    const granule = bytes.readBigUInt64LE(range[0] + 6);
    bytes.writeBigUInt64LE(granule + 48_000n, range[0] + 6);
    updatePageCrc(bytes, range);
  }
  writeFileSync(join(fixtureDir, "mono-997hz-1s-initial-granule.opus"), bytes);
}
