# Canonical SDSL-V in WyrmCoil

WyrmCoil’s Rust compiler has a canonical frontend based on Oct revision
`fa5b4993ca61f79b2ce18d09e9e3bbce88b9b9a0`. It uses the existing Rust lexer,
parser, AST, diagnostics, and shader artifact APIs. Oct supplies the language
specification and qualification fixtures. WyrmCoil compiles without Go or an
Oct executable.

Use `LexCanonicalSource` for canonical tokens, `ParseCanonicalSource` for the
AST, and `CompileCanonicalSourceToShaderArtifact` to validate and emit HLSL.
Existing entry points continue to accept WyrmCoil’s original dialect, including
interface-based specialization, older flow syntax, and fallible syntax. Existing
shaders and material-generated shaders continue to use those entry points.

## Qualification

| Boundary | Checked against the pinned Oct checkout |
| --- | --- |
| Frontend | Parsed all 15 valid sources listed in the conformance manifest, including its five graphics cases. Also parsed four guarded-memory and flow-board shaders. |
| Rejection | Matched the diagnostic code and primary start line and column of 14 selected conformance fixtures. The checks cover deprecated syntax, initialized and immutable bindings, template headers, hexadecimal overflow, and graphics interface errors. |
| Native compilation | WyrmCoil emitted HLSL for Oct’s MinimalVertex, MinimalPixel, and MultipleRenderTargets fixtures. DXC compiled all three and spirv-val accepted the results. The tests also inspect execution model, entry names, locations, and the position builtin. |

The WyrmCoil default suite passed 397 tests, including the two new tests that
run without Oct. All six conformance tests and all 18 Zed adapter tests passed.
The naming check reported the same eight baseline findings.

The optional legacy DXC probe for FlatColor_VS still fails because its emitted
helper refers to an undeclared material field named Color. The probe failed
with the same DXC error on the pre-port commit 8aa10eb. Legacy material
lowering remains a separate gap.

The native emitter supports the qualified resource-free graphics subset.
The frontend reads texture and sampler declarations, imports, concept/config
templates, payload enums, and structured control, including the full
CanonicalGraphicsProgram source. Native compilation of that fixture still
requires module and function resolution, specialization, and resource and
material lowering. Full Oct grammar and semantic parity remain open, including
tensor statements and reductions, compile-time match and utility, and the modern
GPU test and benchmark profiles. The current qualification compares primary
diagnostic starts and selected SPIR-V structure. Runtime and artifact byte
parity are outside those measurements.

Run from the WyrmCoil root. The optional qualification tests use the original
fixtures in a local checkout. DXC, spirv-val, and spirv-dis must be on PATH
for the backend test.

~~~powershell
cargo test --locked
$env:WYRMCOIL_OCT_ROOT = (Resolve-Path ../oct).Path
cargo test --locked --test sdslv_oct_conformance -- --include-ignored
~~~

## VD-MIR in Rust

Port Oct’s VD-MIR semantics and use Rust enums for its representation.
Oct already has the right boundary. Its compiler resolves and validates source,
materializes concrete programs, and lowers typed expressions, structured
blocks, interfaces, resources, and capabilities before HLSL emission. Its
graphics entries and paired program metadata are useful contracts for
WyrmCoil. The Go interfaces and record layout are implementation details.

Use owned structs for functions and ordered module tables. Use enums for
mutually exclusive choices, such as vertex, pixel, or compute entries and
location, target, builtin, or binding roles. Use `Option` for optional
metadata and preserve source spans through lowering. This keeps cases such as a
resource field carrying a stage location out of the validated representation.
Keep control structured, including explicit flow terminators and guarded
loads and stores. Use deterministic source or binding order in dumps and
manifests.

The current graphics role enum shares one interpretation of stream declarations
between validation and HLSL emission. It is a small piece of the proposed
boundary. The next lowering should produce a self-contained typed graphics
program, including entry interfaces and resolved resource bindings, using
the three compiled fixtures as baselines and CanonicalGraphicsProgram as the
first specialization and material target. Compare Oct and Rust on normalized
interface and resource facts. Keep HLSL spellings and DXC policy in the backend,
and migrate the existing emitter through that boundary one supported slice
at a time.
