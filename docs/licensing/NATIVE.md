# Native dependency subset

whisper.cpp and llama.cpp are MIT at the project level. Repositories also contain examples and optional files that may differ.

LocalFlow may distribute only:

- Core C/C++ inference sources required to transcribe or generate
- Matching MIT/Apache headers

The main STT path uses `whisper-rs` 0.13.2, which vendors a MIT whisper.cpp subset at build time (cmake). Record the resolved `whisper-rs-sys` crate version from `Cargo.lock` in release notes.

Not distributed:

- Unreviewed examples
- Extra codec backends
- Bundled model files

Use `scripts/build-native-runtime.sh` to compile the leftover FFI stub. Linking a separately fetched llama.cpp subset still requires a pinned commit SHA in this file before release.
