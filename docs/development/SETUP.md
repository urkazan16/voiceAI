# Development environment

Pinned:

- Node 20.19.x (`.nvmrc`)
- Rust 1.88.0 (`rust-toolchain.toml`)
- Tauri 2.2.5 / CLI 2.2.7
- npm lockfile `package-lock.json`
- Cargo lockfile `src-tauri/Cargo.lock`

```bash
npm install
npm run check
npm test
npm run tauri dev
```

Clean-machine rule: if a dependency is required to build, it is listed in the README. No local binaries, no `.env` secrets, no absolute paths.
