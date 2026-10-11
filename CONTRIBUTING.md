# Contributing

Start with [development setup](docs/DEVELOPMENT.md). Run the frontend and Rust regression suites before submitting a change. Include a short explanation of the problem, changed behavior, test results, and remaining limitations.

Keep model/cloth changes separate from visual-only changes. Preserve skeleton identities, bind transforms, normalized weights, native resource integrity, and the accepted character workflows. Never make a failing validation pass by removing the guard without understanding the failure.

Use small commit summaries such as `fix: preserve garment seam constraints`, `ui: clarify model preparation actions`, or `docs: improve setup instructions`.

Do not commit build outputs, credentials, game dumps, local CSDK installations, or personal workspaces. Synthetic and deliberately curated regression fixtures are an exception to broad asset exclusions; document their purpose and origin. Do not add external assets without appropriate rights.

## Releases are explicit

Ordinary commits do not change the public version, create tags, publish GitHub releases, or schedule releases. A successful CI run uploads temporary build artifacts only. A release requires a separate explicit request for that release. Follow [the release checklist](docs/RELEASING.md).

The active Blender Python bridge is required. Do not remove it as part of cleanup. The retired Python desktop GUI is absent. Optional Blender integration probes retain a small independent Python reference library for fixture/export comparisons; normal Rust CI uses checked-in fixtures.
