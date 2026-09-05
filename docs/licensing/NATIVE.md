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
