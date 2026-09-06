# Contributing to LocalFlow

## Branching

Use `main` plus short-lived feature branches (`feature/audio`, `feature/whisper`, `feature/personalization`, `feature/injection`).

Pull requests must pass:

- tests
- formatter / lint (`npm run check`)
- license scan (`npm run license:check`)
- uniqueness report attached (`npm run uniqueness:check`)
- security scan (`npm audit` / `cargo audit` when available)
- build

## Commits

One logical change per commit. Prefer:

```text
feat(audio): add microphone device discovery
```

Do not squash the entire product into `Initial project`.

## Local loop

```bash
npm install
npm run check
npm test
npm run tauri dev
```

## Native runtimes

Only the reviewed MIT subset of whisper.cpp / llama.cpp may be linked. Do not copy examples, extra codecs, or unlicensed files into `third_party/`. See `docs/licensing/NATIVE.md`.
