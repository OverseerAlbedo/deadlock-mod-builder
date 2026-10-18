# Full-precision cloth audit reader

This small console program uses the MIT ValveResourceFormat parser independently
of the Builder's Rust native-resource decoder. The normal viewer's `F6` text formatter
cannot be an exact numerical transport. This program emits the parsed values as
KV3 text with invariant `G9` (Float32) and `G17` (Float64) numeric formatting
and never modifies its input. Explicit precision preserves the exact native
value at power-of-two boundaries, including TangTang's `-2**-25` quaternion
component; the previous `R` format emitted an adjacent Float64 for that value.
The receiving verifier still applies no numeric tolerance.
Annotations remain covered by the Builder's primary typed comparison; this
transport is the additional independent value check, not an annotation editor.

It accepts only `-i <compiled-model> -b PHYS|DATA`, or `--version`. Failure produces
a nonzero exit code. Resource/block sizes, table bounds, duplicate requested
blocks, overlapping ranges, nesting, duplicate fields and finite numbers are
checked. The parent enforces timeout, cancellation and complete output capture.

## Rebuilding

Use the SDK selected by `global.json` and the pinned package lock. From this
directory run:

```
dotnet publish ClothReader.csproj -c Release -r linux-x64 -p:RestoreLockedMode=true -o ../../artifacts/reader/linux-x64
dotnet publish ClothReader.csproj -c Release -r win-x64 -p:RestoreLockedMode=true -o ../../artifacts/reader/win-x64
```

The runtime is self-contained. Single-file compression is enabled and trimming
is disabled. Each output is one executable. A rebuild is an intentional reader
qualification change, not part of ordinary Windows CI. Review the new output
before replacing the qualified files under `resources/native/cloth-reader/`.
Update BOTH their SHA-256 entries in its `manifest.json` and the corresponding
production pins in `src-tauri/crates/deadlock-tools/src/cloth_reader.rs`.
Update `licenses/cloth-reader/` if dependencies change. From the repository root,
run `node scripts/check-repository.mjs` and
`cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked` on the
supported test hosts, including Windows. Do not loosen trust checks to accept
an unqualified binary. Ordinary source builds use the supplied qualified reader.

Reader protocol/version: `round-trip-v1` / `1.0.2`. Parser package:
`ValveResourceFormat 20.1.7467`, commit
`9e2e6c7f67e3c4e5a5f443a60c3a5d841ba8f304`. Runtime: `.NET 10.0.12`.
Sources and notices: `licenses/cloth-reader/DEPENDENCIES.json` and adjacent files.
Windows cross-publication is not evidence of Windows execution or gameplay.
