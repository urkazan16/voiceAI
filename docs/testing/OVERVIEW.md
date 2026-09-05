# Testing

| Command            | Coverage                                       |
| ------------------ | ---------------------------------------------- |
| `npm test`         | Frontend unit, Rust lib, integration           |
| `npm run test:all` | Adds UI, pipeline, dictionary, personalization |
| `npm run test:ai`  | AI catalog / future inference bench            |

Corpus lives in `tests/corpus/` with `expected.json` per domain. Audio fixtures are tiny and synthetic; large recordings are not stored in git.

Reliability and network-privacy tests that need a full `.app` are described in `docs/testing/RELEASE_GATE.md`.
