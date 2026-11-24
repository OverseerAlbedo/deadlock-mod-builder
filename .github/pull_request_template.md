## Summary

<!-- What changes and why. Link the issue if there is one. -->

## Checks

- [ ] `node scripts/check-repository.mjs`
- [ ] `npm test -- --maxWorkers=2`
- [ ] `npm run build`
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked`
- [ ] Moving-part rules changed? The Rust, Python and TypeScript detectors and `hanging-chains.json` were updated together.
- [ ] `CHANGELOG.md` updated for user-visible changes.

## In-game verification

<!-- What was checked in Deadlock, or what still needs an in-game check. -->
