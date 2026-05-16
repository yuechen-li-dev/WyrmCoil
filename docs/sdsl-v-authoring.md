# SDSL-V Authoring Guide (Current Implementation Status)

This guide documents what SDSL-V can do **today** in WyrmCoil, how to author source that works with the current compiler/test runner, and what remains intentionally out of scope.

Use this alongside `docs/sdsl-v.md`:
- `docs/sdsl-v.md` is the language/design reference and milestone history.
- this file is the practical “what works right now” checkpoint.

## 1) Current pipeline

Current implemented compiler path for `.sdslv`:

```text
.sdslv source
→ LexSource
→ ParseSource
→ ValidateSource / ValidateModule
→ CompileSourceToHlsl / EmitHlsl
→ HLSL text
```

Current implemented test path for `.sdslvtest`:

```text
.sdslvtest source
→ ParseTestSource
→ ValidateTestSource / ValidateTestModule
→ RunTestSource / RunTests
→ CPU-side pass/fail + diagnostics
```

Not implemented yet:
- no renderer shader integration pipeline

## 2) Compiler API quick reference

Primary public APIs currently exposed from `Engine::shader::sdslv`:

- `LexSource`
- `ParseSource`
- `ValidateSource`
- `ValidateModule`
- `EmitHlsl`
- `CompileSourceToHlsl`
- `ParseTestSource`
- `ValidateTestSource`
- `ValidateTestModule`
- `RunTestSource`
- `RunTests`

## 3) Minimal shader that works today

The following is a compact example that currently parses, validates, and emits HLSL:

```sdslv
namespace WyrmCoil.Examples;

type ClipPosition4 = float4 @space(clip.position);

stream VertexOut {
    Position: ClipPosition4;
    Color: float4;
}

shader FlatColor {
    stage vertex fn VS(pos: float3, color: float4) -> VertexOut {
        let output: VertexOut;
        output.Position = float4(pos, 1.0);
        output.Color = color;
        return output;
    }

    stage pixel fn PS(input: VertexOut) -> float4 {
        return input.Color;
    }
}
```

Notes:
- uses only currently supported function-body subset (`let`, assignment, `return`, constructor calls, field access).
- stage names currently supported: `vertex`, `pixel`, `compute`.

## 4) Streams

Streams are explicit stage-boundary payload structs in SDSL-V source.

Current lowering behavior:
- stream declarations lower to HLSL `struct`s.
- stream field lowering preserves declaration order.
- deterministic semantic assignment currently uses:
  - `SV_Position` for clip-position-like position fields (`ClipPosition4` / `@space(clip.position)` / field named `Position` in supported shape)
  - `TEXCOORDn` for non-position fields, assigned in declaration order (`TEXCOORD0`, `TEXCOORD1`, ...).

Current limits:
- semantic assignment is intentionally narrow and deterministic, not fully programmable yet.
- flow declarations do not lower, so flow data does not participate in stream emission.

## 5) Coordinate-space aliases

Semantic aliases are declared with `@space(...)`, for example:

```sdslv
type WorldPosition3 = float3 @space(world.position);
```

Current behavior:
- semantic aliases are **nominally distinct** from other semantic aliases.
- plain aliases (no `@space`) remain transparent aliases.
- known semantic mismatches (world/view/clip/tangent-style mixups) produce diagnostics.
- bounded compatibility checks run for local initializers, assignments, returns, and call arguments.
- matching underlying constructor results are accepted where current bounded typing supports them.

Valid shape example:

```sdslv
type WorldPosition3 = float3 @space(world.position);

shader S {
    fn UseWorld(p: WorldPosition3) -> WorldPosition3 {
        return p;
    }
}
```

Invalid shape example (diagnosed):

```sdslv
type WorldPosition3 = float3 @space(world.position);
type ClipPosition4 = float4 @space(clip.position);

shader S {
    fn Bad(c: ClipPosition4) -> WorldPosition3 {
        return c;
    }
}
```

## 6) Function body subset

Current supported statements:
- `let` declarations (typed, initializer optional)
- assignment to assignable expressions
- `return <expr>;`
- empty statement `;`
- expression statements are accepted in test-function contexts (assert calls), not as general shader-body statements

Current supported expressions:
- identifiers
- scalar literals (`i32`, float-style literals), booleans
- field/member access (including swizzle-style member reads)
- function calls / constructor calls
- arithmetic (`+`, `-`, `*`, `/`) with precedence and parentheses
- unary minus
- comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`) in supported validation/execution contexts

Not currently supported in shader function bodies:
- `if`
- loops (`for`, `while`)
- `match`
- nested block-control syntax beyond the bounded parser subset
- `discard`
- array indexing

## 7) Interfaces and generics

Current implemented interface/generic model:
- `interface` declarations define method signatures.
- shaders explicitly opt in with `implements`.
- implementing methods must be explicitly marked `override`.
- generic shaders support `where` constraints against interfaces.
- explicit compile declarations instantiate concrete variants:

```sdslv
compile ForwardPass<FlatMaterial> as ForwardFlatMaterial;
```

Emission behavior:
- generic templates are not emitted directly as runnable entry points.
- `compile` declarations drive monomorphized concrete stage emission.
- constrained interface-style calls in that pattern are rewritten to concrete helper calls during emission.

Current limitations:
- no default interface methods
- no `base.Method()`
- no implicit interface satisfaction
- no nested generics
- no generic free functions

## 8) `.sdslvtest`

`.sdslvtest` is the SDSL-V test-file extension.

Current syntax/validation:
- `[Fact]` attribute is supported.
- `[Theory]` / `[Case]` are not supported.
- assertions currently supported:
  - `Assert.True(condition, "message")`
  - `Assert.Equals(actual, expected, "message")`
  - `Assert.Near(actual, expected, tolerance, "message")`
- custom string message is mandatory for supported assert calls.
- non-assert expression statements are rejected by validation.

Small runnable test example:

```sdslv
namespace WyrmCoil.Tests;

[Fact]
fn BasicArithmeticAndAsserts() {
    let value: f32 = 1.0 + 1.0;
    Assert.True(value > 0.0, "value should be positive");
    Assert.Equals(value, 2.0, "value should equal two");
    Assert.Near(value, 2.001, 0.01, "value should be near two");
}
```

Harness note:
- compiler development harness remains Rust `cargo test`; there is no reflection-based file discovery runtime for `.sdslvtest` yet.

## 9) Test runner execution subset

`RunTestSource`/`RunTests` currently execute a bounded CPU-side subset.

Can execute today:
- local scalar declarations
- assignment
- arithmetic and unary minus
- comparisons
- selected built-ins: `abs`, `min`, `max`, `clamp`, `saturate`
- assertion calls (`Assert.True`, `Assert.Equals`, `Assert.Near`)

Cannot execute today:
- GPU execution
- DXC/SPIR-V paths
- automatic filesystem discovery of `.sdslvtest`
- full shader-function execution pipeline
- broad vector/matrix semantics beyond current bounded evaluator surface

## 10) Shader flows (current status)

Flows are currently authoring-time structure only (Octomata-inspired), with parser/validation support but **no lowering/execution**.

Current syntax supported:
- `flow Name(params) -> ReturnType { ... }`
- optional `board { Field: Type; ... }` block
- `state Name { ... }`
- guard `when` with `case` arms and required `else`
- `goto State;`
- `return Expr;`
- `board.Field` reads in guard/return expressions

Current validation includes:
- at least one state per flow
- duplicate state names rejected
- `goto` targets must resolve
- `when` requires at least one `case` and an `else`
- board shape checks (single board block, non-empty, unique fields, supported types)
- unknown board-field reads are diagnosed
- board must be declared before states

Current non-goals (still true):
- bounded acyclic value-flow lowering to HLSL helper functions (M13 subset)
- no flow execution/runtime state machine
- no utility `when`
- no `suspend`, `remember`, `resume`

## 11) Feature status table

| Feature | Status |
|---|---|
| Lexer/parser | Implemented |
| Declaration validation | Implemented |
| HLSL stream/stage emission | Implemented (bounded deterministic lowering) |
| Function body subset | Implemented (bounded subset) |
| Interfaces / generic `compile` | Implemented (monomorphization pattern, bounded rewrite) |
| Coordinate-space checking | Implemented (bounded body validation) |
| `.sdslvtest` `[Fact]` syntax | Implemented |
| `.sdslvtest` execution | Implemented (scalar subset runner) |
| Shader flows | Parse/validate only |
| Flow boards + board reads/writes | Parse/validate only (no lowering/execution) |
| Flow lowering | Implemented for acyclic value-returning subset (M13) |
| DXC/SPIR-V boundary | Implemented (optional DXC probe, non-mandatory) |
| Renderer shader integration | Not implemented |

## 12) Non-goals / future work

Known future work items:
- renderer pipeline integration for compiled shader artifacts
- fuller expression/type checking
- shader-body control flow (`if`/loops) expansion as scoped milestones
- flow lowering
- utility `when`
- `[Theory]` / `[Case]`
- richer `.sdslvtest` runner surface
- shader function execution from test runner
- properties
- partial shaders
- richer enum payload/match model

No timeline commitments are implied by this list.


### M12 flow update

Flows now support board writes inside flow states:
- `board.Field = expr;`

Current flow validation in M12 includes:
- board field existence checks for reads and writes
- board write type checks
- bool guard checks for known `when case` condition types
- flow return type checks (direct state return and `when` return actions)

Still out of scope in M12:
- flow lowering/execution
- definite assignment analysis
- utility/suspend/remember/resume

## 13) M14 shader artifact API (current)

SDSL-V now exposes a structured shader artifact contract on top of parse/validate/emit:

- `CompileSourceToShaderArtifact(source_name, source)`
- `BuildShaderArtifact(source_name, module)`

Artifact shape:
- `SourceName: String`
- `Hlsl: String`
- `EntryPoints: Vec<SdslvEntryPoint>`

Entry point metadata contains:
- `Name` (generated HLSL entry name, e.g. `FlatColor_VS`)
- `Stage` (`Vertex` / `Pixel` / `Compute`)
- `ShaderName`
- `MethodName`
- `TargetProfile` (`vs_6_0` / `ps_6_0` / `cs_6_0`)

Behavior notes:
- entry points are collected from `stage` methods only
- helper methods and flow helpers are not entry points
- compile aliases are reflected as entry points (e.g. `ForwardFlatMaterial_PS`)
- generic templates are not directly listed as entry points
- failures in parse/validate/emit return diagnostics and do not return partial artifacts

Still out of scope in M14:
- no DXC invocation
- no SPIR-V generation
- no renderer integration
- no shader reflection


## 14) M15 DXC boundary (optional backend probe)

M15 adds an optional DXC compile boundary on top of M14 artifacts.

Current boundary path:

```text
SdslvShaderArtifact + SdslvEntryPoint
→ DxcCompileRequest
→ optional external DXC invocation
→ DxcCompileResult
```

APIs now include:
- `DxcOptions` (default: `DxcPath = "dxc"`, `OutputSpirv = true`)
- `DxcCompileRequest::FromArtifactEntry(...)`
- `FindDxc(...)`
- `BuildDxcCommand(...)`
- `CompileHlslWithDxc(...)`
- `CompileArtifactEntryWithDxc(...)`

Behavior notes:
- DXC is optional; unavailable tools return structured `DxcError::ToolUnavailable`.
- normal `cargo test` does not require DXC.
- command-shape and artifact-request mapping are covered by non-DXC tests.
- optional real-DXC test is ignored and gated with `WYRMCOIL_RUN_DXC_TESTS=1`.

Still out of scope in M15:
- renderer / `wgpu` pipeline wiring of shader binaries
- reflection/resource binding extraction
- asset pipeline integration
