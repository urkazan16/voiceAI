# Licensing policy

Application license: MIT.

Allowlist for production dependencies:

- MIT
- Apache-2.0
- BSD-2-Clause
- BSD-3-Clause
- ISC
- Public Domain / CC0-1.0 / Unlicense / 0BSD

Strong copyleft (GPL, AGPL) fails CI until a recorded legal exception exists in `EXCEPTIONS.md`.

Reviewed data / cryptographic licenses used by the Rust ecosystem:

- Unicode-3.0 (`unicode-ident`)
- Zlib (`bytemuck` dual-license option)
- OpenSSL (`ring` conjunction with MIT/ISC)

Model weights are Apache-2.0 (Qwen) or MIT (whisper ggml distributions as documented by upstream). Weights are downloaded by the user and are not embedded in the DMG until redistribution terms are verified.
