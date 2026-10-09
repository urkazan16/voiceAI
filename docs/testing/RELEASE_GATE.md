# Release gate

Before tagging a release:

1. Clean checkout
2. `npm ci`
3. `npm run check`
4. `npm test` / `npm run test:all`
5. `npm run test:ai` when models are present
6. Performance and reliability benches (`docs/evaluation/BENCHMARK.md`)
7. `npm run measure:block0` → `docs/evaluation/BLOCK0.md`
8. Offline run after models are installed
9. Confirm no external network during record → insert
10. `npm run license:check`
11. `npm run build:release` (SBOM + SHA256SUMS + `.app` / `.dmg`)
12. Fill `docs/evaluation/MVP_SCORECARD.md` from measured results

For an experimental Ogg/Opus candidate, manually dispatch CI with
`opus_package_smoke=true` and retain the four green target logs. This path
builds signed/notarized macOS packages plus Windows NSIS and Linux packages,
runs `opus-decode-smoke` from the packaged application, and does not publish or
upload the experimental installers. It also records default-versus-feature
installer size, packaged decode time and peak RSS. Review those generated
`OPUS_PACKAGE_DELTA` reports together with
`docs/licensing/SYMPHONIA_MPL_REVIEW.md` and
`docs/development/STT_CANCELLATION_CONTRACT.md`.

Minimum pass: 85/100 with Accuracy ≥ 20, Speed ≥ 15, Reliability ≥ 17. Critical gates (offline, local LLM, buildable, LICENSE, checksums, no code execution) cannot be traded for UI polish.
