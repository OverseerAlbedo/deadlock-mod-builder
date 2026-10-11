# Exact Source 2 resource values

This crate supplies bounded KV3v1/v4/v5 decoding (uncompressed, LZ4, and Zstd),
exact typed ordered values with annotations, a deterministic validated KV3v1
encoder, lossless text KV3 span edits, resource-envelope rebuilds by name or
ordinal, and exact independent text-to-binary value verification.

Limits are explicit: 128 MiB decoded block, one million items per container,
eight million total values, and 256 levels of nesting. Headers, lane consumption,
string indices, blob lengths, dependent LZ4 history, Zstd frame completion,
footer, finite numeric values, annotations, duplicate keys, and outer resource
bounds are checked. Unsupported layouts fail closed. Rebuilds preserve every
untouched payload and block order; repeated MDAT/AGRP blocks can be selected by
ordinal without editing siblings.

`Value::canonical_eq` preserves object order, annotations, signed floating zero,
and every unknown field. Float32 values may expand exactly to float64; integer
storage widths share the integer domain. `value::canonical_sha256` uses that same
value domain with explicit type/length framing. No epsilon is applied to physics.

The text parser retains UTF-8 byte spans, comments, whitespace and unknown typed
scalars. Edits must be nonoverlapping and on character boundaries, and the result
is reparsed before return. Untouched authored content stays byte-identical.

Independent-reader verification preserves raw stdout independently of timing
and UI logs. Float32 casts are permitted only when the serialized field really
is Float32; Float64 requires exact equality. Blob literals cannot be substituted
with similarly named authored objects. Text does not replace the primary typed
annotation/order proof.

Read-only cloth structure, control graph, scalar/SIMD rod and supported capsule
feasibility diagnostics live here. They are prerequisites, not transfer or build
authorization; full resource composition is in `deadlock-cloth`.

Tests include six synthetic compression/type layouts, malformed/truncated data,
lossless Unicode edits, all 46 supported native block ordinals from both R12
references compared with the Python canonical encoder, 36 diagnostic cases, and
actual bundled .NET decoding of Rust-encoded PHYS/DATA. Development fixture
exporters use Python only to record parity evidence; production does not.
