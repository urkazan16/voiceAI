# Contributing to LocalFlow

## Branching

Use `main` plus short-lived feature branches (`feature/audio`, `feature/whisper`, `feature/personalization`, `feature/injection`).

Pull requests must pass:

- tests
- `npm run check:gate` — same as CI jobs **quality**, **license**, and **security**
- formatter / lint / Clippy (`npm run check`, includes the gate)
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
npm run check:gate
npm run check
npm test
npm run tauri dev
```

`npm install` installs a `pre-commit` hook that runs `npm run check:gate` so a commit cannot land if quality, license, or security would fail.

## Native runtimes

Only the reviewed MIT subset of whisper.cpp / llama.cpp may be linked. Do not copy examples, extra codecs, or unlicensed files into `third_party/`. See `docs/licensing/NATIVE.md`.
