#!/usr/bin/env bash
# Decode APPLE_CERTIFICATE (base64 PKCS#12), import into a temporary keychain,
# and point codesign at it. Tauri CLI 2.2.x uses macOS `base64 --decode`, which
# rejects wrapped/quoted GitHub secrets.
set -euo pipefail

if [[ -z "${APPLE_CERTIFICATE:-}" || -z "${APPLE_CERTIFICATE_PASSWORD:-}" ]]; then
  echo "APPLE_CERTIFICATE and APPLE_CERTIFICATE_PASSWORD must be set." >&2
  exit 1
fi

CERT_PATH="${RUNNER_TEMP:-/tmp}/certificate.p12"
KEYCHAIN="${RUNNER_TEMP:-/tmp}/app-signing.keychain-db"
export CERT_PATH

python3 - <<'PY'
import base64, os, pathlib, re, sys

raw = os.environ["APPLE_CERTIFICATE"].replace("\ufeff", "").strip().strip('"').strip("'")
cleaned = re.sub(r"\s+", "", raw).replace("-", "+").replace("_", "/")
cleaned += "=" * ((-len(cleaned)) % 4)
try:
    data = base64.b64decode(cleaned, validate=True)
except Exception:
    print(
        "APPLE_CERTIFICATE is not valid base64 of a .p12.\n"
        "On the Mac that has the Developer ID cert:\n"
        "  openssl base64 -A -in your.p12 | pbcopy\n"
        "Paste one line into the environment secret (no quotes, no line breaks).",
        file=sys.stderr,
    )
    sys.exit(1)
if len(data) < 64:
    print(f"decoded payload is too small ({len(data)} bytes) to be a PKCS#12.", file=sys.stderr)
    sys.exit(1)
pathlib.Path(os.environ["CERT_PATH"]).write_bytes(data)
print(f"decoded PKCS#12 ({len(data)} bytes)")
PY

if ! openssl pkcs12 -in "$CERT_PATH" -passin env:APPLE_CERTIFICATE_PASSWORD -nokeys >/dev/null; then
  echo "Decoded bytes are not a PKCS#12, or APPLE_CERTIFICATE_PASSWORD is wrong." >&2
  echo "Export Developer ID Application as .p12 from Keychain Access, then:" >&2
  echo "  openssl base64 -A -in file.p12 | pbcopy" >&2
  rm -f "$CERT_PATH"
  exit 1
fi

KEYCHAIN_PASSWORD="$(openssl rand -base64 32)"
security delete-keychain "$KEYCHAIN" >/dev/null 2>&1 || true
security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security set-keychain-settings -lut 21600 "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security import "$CERT_PATH" \
  -k "$KEYCHAIN" \
  -P "$APPLE_CERTIFICATE_PASSWORD" \
  -A \
  -f pkcs12 \
  -T /usr/bin/codesign \
  -T /usr/bin/security \
  -T /usr/bin/productbuild
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security list-keychains -d user -s "$KEYCHAIN" login.keychain
security default-keychain -s "$KEYCHAIN"
security find-identity -v -p codesigning "$KEYCHAIN"
rm -f "$CERT_PATH"

{
  echo "KEYCHAIN_PATH=$KEYCHAIN"
  echo "KEYCHAIN_PASSWORD=$KEYCHAIN_PASSWORD"
} >> "$GITHUB_ENV"
echo "Imported Developer ID certificate into a temporary keychain."
