# Symphonia MPL-2.0 project review

Decision date: 2026-10-09  
Component: Symphonia 0.6.1  
Decision: **approved for the optional `audio-symphonia-opus` build**

This is the project's distribution-compliance decision, not outside legal
advice. It replaces the earlier pending entry for Symphonia in
`EXCEPTIONS.md`.

## Reviewed scope

The resolved feature graph contains these unmodified MPL-2.0 crates, all at
version 0.6.1:

- `symphonia`
- `symphonia-common`
- `symphonia-core`
- `symphonia-format-ogg`
- `symphonia-metadata`

LocalFlow does not patch those crates. The separately modified
`symphonia-adapter-libopus` is distributed under its upstream Apache-2.0
option, so it is outside the MPL-covered file set.

## Decision basis

MPL-2.0 is file-level copyleft. It permits the MPL files to be compiled and
statically linked into a larger work without relicensing LocalFlow's own files.
For executable distribution, recipients must be told how to obtain the MPL
Source Code Form, and additional distribution terms must not restrict their
MPL rights.

The project therefore approves this dependency subject to the controls below:

1. Keep the exact MPL license text in `licenses/symphonia-mpl-2.0.txt`.
2. Bundle `NOTICE`, the license text and `licenses/symphonia-source.json` in
   every application package.
3. Keep the direct crates.io source URLs and Cargo.lock SHA-256 values in that
   source manifest. These `.crate` archives are the exact preferred form for
   modification used by the build.
4. If any MPL-covered crate is patched or vendored, preserve MPL notices,
   publish the modified MPL files in Source Code Form, and repeat this review.
5. Do not add terms that prevent recipients from exercising MPL-2.0 rights in
   the covered files.
6. Re-run `npm run license:check` and package inspection for every release.

## Verification performed

The five cached `.crate` archives were hashed locally and matched Cargo.lock.
The feature-aware dependency graph contains no other Symphonia crate. The
release wrapper verifies that the installed application contains the notice,
MPL text and source manifest, and `SHA256SUMS` covers those files.

With these controls, Symphonia 0.6.1 is approved for experimental and
production distribution as an optional component. Enabling the audio backend
by default remains a separate product and engineering decision.
