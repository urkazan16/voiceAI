#!/usr/bin/env python3
"""Decode APPLE_CERTIFICATE into a PKCS#12 file. Never print the secret."""

from __future__ import annotations

import base64
import binascii
import os
import pathlib
import re
import sys
from urllib.parse import unquote

B64_RE = re.compile(r"^[A-Za-z0-9+/]*={0,2}$")
ZWSP_RE = re.compile(r"[\u200b\u200c\u200d\u2060\ufeff]")
PREFIX_RE = re.compile(
    r"^(?:APPLE_CERTIFICATE=|data:application/[^;]+;base64,|base64,)",
    re.IGNORECASE,
)


def _normalize(raw: str) -> str:
    text = raw.replace("\ufeff", "")
    text = ZWSP_RE.sub("", text)
    text = text.strip().strip('"').strip("'")
    text = PREFIX_RE.sub("", text).strip()
    text = (
        text.replace("\\r\\n", "")
        .replace("\\n", "")
        .replace("\\r", "")
        .replace("\\t", "")
    )
    if "%" in text:
        text = unquote(text)
    pem = re.search(
        r"-----BEGIN ([^-]+)-----([A-Za-z0-9+/=\s]+)-----END \1-----",
        text,
        re.IGNORECASE,
    )
    if pem:
        kind = pem.group(1).strip().upper()
        if "PKCS" not in kind and "PRIVATE" not in kind:
            raise ValueError(
                "APPLE_CERTIFICATE looks like a PEM .cer/.pem, not a PKCS#12 .p12. "
                "In Keychain Access export Developer ID Application as .p12 "
                "(Personal Information Exchange), with the private key."
            )
        text = pem.group(2)
    return re.sub(r"\s+", "", text)


def _invalid_codepoints(text: str) -> list[str]:
    seen: list[str] = []
    for ch in text:
        if ch in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=-_":
            continue
        label = f"U+{ord(ch):04X}"
        if label not in seen:
            seen.append(label)
        if len(seen) >= 12:
            break
    return seen


def diagnose(raw: str) -> str:
    compact = re.sub(r"\s+", "", raw)
    hints: list[str] = []
    if len(raw) < 200:
        hints.append(
            f"value is only {len(raw)} characters; a Developer ID .p12 is thousands of base64 characters"
        )
    upper = raw.upper()
    if "DEVELOPER ID" in upper or (
        "APPLICATION:" in upper and "UZ676" in upper.replace(" ", "")
    ):
        hints.append(
            "this looks like APPLE_SIGNING_IDENTITY, not the base64 .p12 — paste openssl base64 -A output"
        )
    if "@" in raw and "MII" not in raw:
        hints.append("this looks like an email (APPLE_ID), not the certificate")
    if "BEGIN" in upper and "CERTIFICATE" in upper:
        hints.append("this looks like a PEM certificate (.cer), not a .p12 with the private key")
    if re.search(r"(?i)(/Users/|[A-Za-z]:\\|\.p12\b|\.cer\b)", raw) and "MII" not in raw:
        hints.append(
            "this looks like a file path; encode the file: openssl base64 -A -in file.p12 | pbcopy"
        )
    if "\ufffd" in raw:
        hints.append(
            "contains U+FFFD replacement characters — the secret was stored as a binary .p12; encode it first"
        )
    invalid = _invalid_codepoints(compact)
    if invalid:
        hints.append(f"non-base64 code points: {', '.join(invalid)}")
    if not hints:
        hints.append("paste one line of openssl base64 -A output, no quotes")
    return "; ".join(hints)


def decode_p12(raw: str) -> bytes:
    if not raw or not raw.strip():
        raise ValueError("APPLE_CERTIFICATE is empty")
    cleaned = _normalize(raw)
    if re.fullmatch(r"[0-9A-Fa-f]+", cleaned) and len(cleaned) >= 128 and len(cleaned) % 2 == 0:
        data = binascii.unhexlify(cleaned)
        _require_p12(data, raw)
        return data
    body = cleaned.replace("-", "+").replace("_", "/")
    body = body.rstrip("=")
    body += "=" * ((-len(body)) % 4)
    try:
        if B64_RE.match(body):
            data = base64.b64decode(body, validate=True)
        else:
            filtered = re.sub(r"[^A-Za-z0-9+/=]", "", body)
            filtered = filtered.rstrip("=")
            filtered += "=" * ((-len(filtered)) % 4)
            if len(filtered) < 80:
                raise ValueError("not enough base64 after cleanup")
            data = base64.b64decode(filtered, validate=True)
    except Exception as exc:
        raise ValueError(diagnose(raw)) from exc
    _require_p12(data, raw)
    return data


def _require_p12(data: bytes, raw: str) -> None:
    if len(data) < 64:
        raise ValueError(
            f"decoded payload is too small ({len(data)} bytes) to be a PKCS#12. {diagnose(raw)}"
        )
    if data[0] != 0x30:
        raise ValueError(
            "decoded bytes are not a PKCS#12 (expected ASN.1 SEQUENCE). "
            "Export Developer ID Application as .p12, then: openssl base64 -A -in file.p12 | pbcopy"
        )


def main() -> int:
    raw = os.environ.get("APPLE_CERTIFICATE", "")
    dest = os.environ.get("CERT_PATH")
    if not dest:
        print("CERT_PATH is not set", file=sys.stderr)
        return 1
    try:
        data = decode_p12(raw)
    except ValueError as exc:
        print("APPLE_CERTIFICATE is not a base64 Developer ID .p12.", file=sys.stderr)
        print(str(exc), file=sys.stderr)
        print("On the Mac that has the cert:", file=sys.stderr)
        print("  openssl base64 -A -in your.p12 | pbcopy", file=sys.stderr)
        print(
            "Paste that one line into environment APPLE_CERTIFICATE "
            "(Settings → Environments), not the identity, path, .cer, or binary file.",
            file=sys.stderr,
        )
        return 1
    pathlib.Path(dest).write_bytes(data)
    print(f"decoded PKCS#12 ({len(data)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
