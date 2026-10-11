# Blender test reference helpers

Only the Python reference modules and fixture helpers imported by `scripts/test_blender*.py` are retained here. Their static import dependency closure was copied from the accepted development source. The retired Qt/Tk user interface, desktop entry points, Python packaging and old release artifacts are absent.

These modules provide independent fixture generation/export comparisons for optional live-Blender integration probes. They are not imported by the Rust desktop app or by the normal Windows CI build. Current production Blender scripts remain under `src-tauri/crates/deadlock-preparation/resources/blender/` and are embedded into Rust.

Do not discard these test helpers solely because they are Python. Removing them would break the retained Blender regressions. Run those probes through a supported Blender build and read each script's input/output requirements first.
