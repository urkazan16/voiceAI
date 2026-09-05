# Native dependency subset

whisper.cpp and llama.cpp are MIT at the project level. Repositories also contain examples and optional files that may differ.

LocalFlow may distribute only:

- Core C/C++ inference sources required to transcribe or generate
- Matching MIT/Apache headers

Not distributed:

- Unreviewed examples
- Extra codec backends
- Bundled model files

Use `scripts/build-native-runtime.sh` to compile the stub library shipped in-tree. Linking a fetched subset requires a pinned commit SHA recorded in this file before release.
