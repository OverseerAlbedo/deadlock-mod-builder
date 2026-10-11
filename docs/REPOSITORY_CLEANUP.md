# Repository cleanup record

The clean GitHub source starts from the accepted 0.13.0-alpha.46 development snapshot. Model preparation, cloth authoring, hair/tail protections, Daki streamers, Graves fitting, UI components and checked-in regression fixtures were copied unchanged.

Removed from the GitHub source distribution:

- Retired Python desktop application and its obsolete Qt/Tk/PyInstaller launch/build files.
- Old Python wheel and unused archived compiler wrapper ZIP.
- Migration checkpoint, dozens of one-off historical release scripts, old generated source manifests and standalone build-result text files.
- Historical generated diagnostics, workspace captures, and obsolete documentation tied to retired paths.
- Installed dependencies, frontend outputs and Rust build caches.

These historical files remain recoverable from the preceding development-source archive; deletion from this public-source layout does not remove active runtime imports. No current Rust/React build or default regression test invokes the retired desktop backend. The active Blender integration probes are retained with only the Python reference helpers in their import dependency closure. No retired desktop GUI or packaging is included.

Kept because they are active: embedded Blender Python, all Rust crates/tests, React components/tests, catalogs and tool recipes, hash-pinned native readers, reader rebuild source/lock/licenses, and qualified physics references.

New release support is limited to CI, preflight, packaging, startup checks, notices and documentation. The app version is unchanged. The original application identifier is retained so existing user settings continue to resolve to the same location.
