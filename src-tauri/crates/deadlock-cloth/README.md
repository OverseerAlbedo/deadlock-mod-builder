# Cloth resource boundary

`deadlock-cloth` operates on immutable compiled-resource bytes, with exact ordered
KV3 values from `deadlock-resource` and identity/bind checks from
`deadlock-skeleton`. It has no Python, filesystem, process, UI, or game dependency.
The original R12 reference and user-accepted balanced reference are never edited.

Public entry points:

- `audit_resource(raw)` returns counts, complete canonical solver fingerprint,
  source skeleton, named motion flags, and proved control bindings. No-PHYS models
  are valid zero-cloth inputs. External VPHYS references and multiple PHYS blocks
  are refused.
- `validate_motion(original, candidate, source)` compares original motion,
  including ordinary child extensions and full compiled binding proof. Its typed
  report can have `passed = false`; the caller must reject that state.
- `restore(original, candidate, source)` restores allowlisted compiler name
  aliases, then only DATA motion bits and byte-exact original PHYS. Other block
  payloads, remapping, mesh weights, and animation samples remain exact. A
  different authored solver is never automatically replaced.
- `qualify_tested(reference)` freshly checks FE structure/tree/index capacity,
  complete scalar/SIMD rods, supported rest-constraint feasibility, mesh and
  animation bindings, inverse/rest transforms, and self-contained solver values.
- `inspect_tested(original, reference, source)` binds qualification to immutable
  original/reference hashes and the selected source's custom skeleton authority.
- `apply_tested(original, reference, candidate, expected, source)` transfers the
  complete qualified FE object and corresponding named motion bits. Original
  rigid-body fields remain authoritative; no partial merge, solver-index map,
  leaf-to-node guess, coefficient conversion, or numeric tolerance is used.
- `validate_tested(...)` recomputes qualification and all exact candidate proofs;
  prior success receipts cannot substitute for current resources.

The canonical solver fingerprint algorithm is explicitly named
`canonical-kv3-v1-sha256`. It matches the exact canonical value equality domain:
ordered object fields, annotations, float bits (including signed zero), blobs,
and authored strings remain distinct; integer storage widths and exact float32
expansion do not change the semantic value. It is not the old CLI JSON hash and
must not be compared with a Python-era cached fingerprint.

A successful typed transform is not publication authorization. The pipeline must
independently read PHYS and DATA through the pinned full-precision native reader,
verify every serialized value, preserve the compiler output, recheck hashes and
source authority, then atomically commit within its process barrier. Packaging
and installation must repeat current-game and frozen-resource gates. The
`independent_reader_validation_required` receipt field states this explicitly.
No receipt claims in-game simulation was tested.

## Differential verification

Development-only Python scripts capture the established implementation's oracle
fixtures; they are never shipped as runtime dependencies. Five restoration
fixtures (normal, reordered, ordinary extension, already-preserved, no-cloth) and
complete tested FE import produce byte-identical resource output in Rust. Fifteen
rest/scale/inverse-bind/animation/tree/rod/fit/CTRL/capacity/opaque-value/body/flag
corruption fixtures remain rejected. Additional cases cover no PHYS, external
VPHYS, duplicate PHYS, authored solver conflicts, and changed authority hashes.

The accepted native 8,381-node / 380-static / 95-bone balanced reference qualifies
without modification. `deadlock-pipeline/tests/motion.rs` composes native transfer
with the real bundled .NET reader and checks the complete Python oracle output:

- original SHA-256: `82e7212cfd93138c663ece73c6d5145f5146bca19f27942c7d769514354ebe7e`
- accepted reference: `27dd44ea4db29f671cff53d0dd8ad171792fe5f12f45b05aca5db65e0a6b5b0c`
- stripped candidate recipe: `29c5b78991ab6acb707d767ea2c718e197b66176cb392bec3682377ba52bf5f1`
- restored output: `9bd997cc0014cbddd4a9f4cd4568181feae9f45917b9300576dcbb53e1031518`

Those are resource-boundary tests. They do not execute Valve's compiler, launch
Deadlock, or certify a newly built Windows installer.

Ordinary source-authorized extra bones now qualify for originals without cloth as well. This explicitly corrects the legacy `cloth_nodes > 0` false-rejection predicate. No-cloth extensions require a SourceBinding and the same complete compiled mesh/remap/inverse/animation and original skeleton proofs; absence of PHYS is represented honestly and motion restoration remains unnecessary. A missing source contract or unauthorized extra bone still fails.
