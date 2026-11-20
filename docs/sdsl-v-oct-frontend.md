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

Both WyrmCoil dialects now use the same backend path.

~~~text
source -> AST -> validate and lower -> VdMirModule -> HLSL -> optional DXC
~~~

`LowerToVdMir` validates a parsed module and produces owned, source-ordered
items. The IR carries typed declarations and values, structured statements,
graphics interface roles, nominal spaces, and available source spans. Legacy
flow targets are state indices with explicit terminators. Lowering resolves
legacy specialization calls and orders named constructor arguments before
the backend sees them. Values whose type is left to HLSL by the existing
frontend carry `Unresolved`.

`EmitVdMirHlsl` consumes the IR, and shader artifacts derive their entry-point
metadata from the same lowered functions. The existing `EmitHlsl` and
shader artifact APIs lower internally. HLSL semantic spellings and DXC
profiles stay on the backend side of the boundary.

Compared with the pre-MIR revision `f10d4b9`, eight fixtures reproduced HLSL
and entry-point metadata byte for byte. The sampled cyclic flow reproduced its
diagnostic as well. The six Oct qualification tests, including the three DXC
and SPIR-V checks, passed through the MIR path. The default suite passed 401
tests with seven ignored, and all 18 Zed adapter tests passed. The naming
check reported the eight baseline findings. The opt-in legacy material
failure described above is still present.

The current lowering covers the existing emitter’s language subset.
Canonical resource bindings, materials, compute, guarded memory, and
template specialization still require lowering. The textured
CanonicalGraphicsProgram fixture remains the concrete target for those
graphics additions.
