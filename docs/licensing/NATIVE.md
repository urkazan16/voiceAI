# Native inference

whisper.cpp is pulled in as the MIT subset vendored by `whisper-rs` 0.13.2 (`whisper-rs-sys` in `Cargo.lock`). There is no `runtime.c` stub in the app binary.

LocalFlow may distribute only:

- Core C/C++ inference sources required to transcribe (via whisper-rs-sys)
- Matching MIT/Apache headers

Not distributed:

- Unreviewed examples
- Extra codec backends
- Bundled model files
- llama.cpp (professional/code modes use on-device formatting)

## Experimental Ogg/Opus backend

The optional, non-default `audio-symphonia-opus` feature uses the LocalFlow-pinned
Apache-2.0 fork of `symphonia-adapter-libopus` 0.3.0 and `opusic-sys` 0.7.5.
The latter builds and statically links its vendored libopus 1.6.1. The end user
does not install libopus or CMake. The C toolchain and CMake are build-time tools.

The vendored native source artifact is the `opusic-sys 0.7.5` crates.io archive,
SHA-256 `c9d1ecdf206421bc74343ab3bb2f30ad2abbfee41fa341f7181fecbaf957769a`.
Its embedded `opus/package_version` identifies libopus 1.6.1. Generate an
experimental SBOM with `npm run sbom:opus`.

The packaged license texts are `licenses/symphonia-mpl-2.0.txt`,
`licenses/symphonia-adapter-apache-2.0.txt`, and `licenses/libopus.txt`.
Tauri copies `NOTICE` and the complete `licenses/` directory into stable
application resource paths, while CI retains the same files with the SBOM and
changelog beside each target's installer artifact.
The project review in `SYMPHONIA_MPL_REVIEW.md` approves Symphonia 0.6.1 for
optional distribution. `licenses/symphonia-source.json` gives recipients
direct crates.io URLs and Cargo.lock SHA-256 values for the exact five
MPL-covered source archives; it is bundled with the license texts and covered
by `SHA256SUMS`.

Build an experimental installer and its feature-aware SBOM with
`LOCALFLOW_AUDIO_SYMPHONIA_OPUS=1 npm run build:release`. Omitting the variable
keeps the normal release feature off.
The experimental wrapper also runs `opus-decode-smoke` from the packaged
application and requires exactly 16,000 mono samples at 16 kHz from the pinned
one-second fixture before it accepts the package.

The MPL-2.0 review and local macOS packaged-artifact inspection are complete.
Production enablement still requires retained green package-smoke and
measurement results for every declared release target.
