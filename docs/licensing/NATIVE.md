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
Symphonia source identity and checksums are retained in `Cargo.lock` and the
generated SBOM; a release must preserve a practical way for recipients to
obtain the exact corresponding MPL-covered source.

This feature is not approved for a production installer until the separate
Symphonia MPL-2.0 review, target installer smoke tests, and final packaged
artifact inspection are complete.
