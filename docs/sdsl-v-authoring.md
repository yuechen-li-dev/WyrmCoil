# SDSL-V Authoring Guide (Current Implementation Status)

This guide documents what SDSL-V can do **today** in WyrmCoil, how to author source that works with the current compiler/test runner, and what remains intentionally out of scope.

Use this alongside `docs/sdsl-v.md`:
- `docs/sdsl-v.md` is the language/design reference and milestone history.
- M35 shader source policy selects between SDSL-V and WGSL source inputs with Dunewyrm utility scoring; this pass is policy-only and does not compile either path.
- this file is the practical “what works right now” checkpoint.

Language philosophy:
- SDSL-V is HLSL-targeting, not HLSL-compatible.
- SDSL-V is the preferred WyrmCoil shader authoring path.
- WGSL/raw HLSL can exist as backend/source escape paths, but they are not SDSL-V.

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

SDSL-V also supports `record` declarations for plain value aggregates:

- `record` = ordinary value aggregate (no stage semantics).
- `stream` = stage-boundary aggregate (semantic-bearing stage IO).

Current lowering behavior:
- stream declarations lower to HLSL `struct`s.
- stream field lowering preserves declaration order.
- deterministic semantic assignment currently uses:
  - `SV_Position` for clip-position-like position fields (`ClipPosition4` / `@space(clip.position)` / field named `Position` in supported shape)
  - `TEXCOORDn` for non-position fields, assigned in declaration order (`TEXCOORD0`, `TEXCOORD1`, ...).

Current limits:
- semantic assignment is intentionally narrow and deterministic, not fully programmable yet.
- flow declarations do not lower, so flow data does not participate in stream emission.

Record example:

```sdslv
record SurfaceData {
    WorldPos: WorldPosition3;
    Normal: WorldNormal3;
    BaseColor: float4;
    Roughness: f32;
}
```

Current record lowering behavior:
- record declarations lower to plain HLSL `struct`s.
- record fields do not receive `SV_Position`/`TEXCOORDn` semantics.
- record types are valid in function parameter/return/local type positions where current type handling already applies.

`with` copy-update expressions (M53b):
- syntax: `base with { Field: value, ... }`
- base type must resolve to a `record` or `stream`.
- duplicate update fields are rejected.
- unknown update fields are rejected.
- field value type compatibility uses existing bounded M6 checks.
- result type is the same as the base expression type.

Aggregate parameter immutability (M53c):
- function/stage parameters of `record` or `stream` type are immutable in body assignment targets.
- direct field mutation like `input.Color = ...` or `surface.Roughness = ...` is rejected for those parameters.
- use `with` to derive modified copies from incoming aggregate parameters.
- local `record`/`stream` variables remain assignable for construction/update patterns (for example `let output: VertexOut; output.Position = ...;`).
- broader `let`/`var` mutability rules are still future work.

Current bounded emission support:
- local declaration initializers
- assignment RHS
- return expressions (lowered through deterministic `__withN` temporaries)

Future work:
- broader stream/record immutability rules are not implemented yet.

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
- `if <expr> { ... }` and `if <expr> { ... } else { ... }` parse into AST (M55b parser/AST)
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
- condition-switch expressions parse into AST (M55b):
  - `switch { case cond => expr ... else => expr }`
  - `switch { case cond -> expr ... else -> expr }`
- subject-switch expressions parse into AST (M56):
  - `switch subject { case value => expr ... else => expr }`
  - `switch subject { case value -> expr ... else -> expr }`

Implemented in M56 (with M55c preserved):
- `if` statements validate and lower to HLSL `if`/`else` blocks.
- condition-switch expressions (`switch { case cond => expr ... else => expr }`) validate and lower in bounded statement contexts (local initializer, assignment RHS, return expression).
- subject-switch expressions (`switch subject { case value => expr ... else => expr }`) validate and lower in the same bounded statement contexts.

Validation/lowering rules (M56):
- `if` condition must be `bool` when known; known non-bool is rejected.
- nested decision ladders in `else { if ... }` shape are rejected; use `switch { case ... else ... }`.
- switch requires at least one `case` and an `else`.
- condition-switch: each `case` condition must be `bool` when known.
- subject-switch: each `case` value must match the subject type when known.
- switch arm values must be type-compatible where known.
- subject-switch lowers deterministically to `if (subject == case)` / `else if` / `else` chains (not HLSL `switch`).
- switch expression use outside supported statement contexts emits a clear M55c diagnostic.

Not currently fully supported in shader function bodies:
- bounded loops are supported only in numeric range form:
  - `for i in start..end { ... }`
  - `for i in start..end step k { ... }`
- loop rules:
  - inclusive `start`, exclusive `end`
  - integer bounds/step when known
  - step defaults to `1`
  - known literal step must be `> 0`
- unsupported loop/control forms:
  - `while` (reserved for future bounded condition-loop design)
  - `break`
  - `continue`
- shader authors should prefer bounded `for` loops for predictable GPU execution.
- `flow`, `state`, and `step` are reserved SDSL-V keywords and cannot be used as local/declaration identifiers.
- `match` is supported in two forms:
  - enum match: `match mode { ShadowMode.None => 0 ... }`
  - fallible match (M65): `match Parse(raw) { ok(v) => v err(_) => 30 }`
    - subject must be fallible
    - both `ok(...)` and `err(...)` arms are required
    - `?` is still preferred for pure propagation; `!` is explicit unwrap
    - fallible match is local handling and returns an infallible result value
    - fallible match HLSL lowering is not implemented in M65
- nested block-control syntax beyond the bounded parser subset
- `discard`
- dynamic arrays/slices

M61 fixed-array note:
- fixed-size array type references are supported: `array<T, N>`.
- `N` must be a positive integer literal in source (`array<f32, 4>` is valid; `array<f32, 0>` and non-literals are invalid).
- array indexing is supported for array-typed expressions: `arr[i]`.
- index expressions must be integer-typed where known.
- fixed-array element assignment is supported in M60: `arr[i] = value;`.
- array element assignment validates element-type compatibility (`array<f32, N>` expects `f32`, `array<float2, N>` expects `float2`, etc.).
- array parameters are immutable for element writes in M60 (`param[i] = value` is rejected); local arrays remain assignable.
- arrays are distinct from shader numeric vector/matrix value types (`float2`, `float3`, `float4`, `float4x4`).
- fixed-array literals are supported in explicit array contexts: `let weights: array<f32, 4> = [1.0, 2.0, 3.0, 4.0];`.
- `[...]` is always an array literal and never a vector/matrix literal.
- array literals require expected array type context in M61 and are validated for length and element type compatibility.
- array literals cannot initialize non-array targets (for vectors/matrices, use constructors like `float3(...)` and `float4x4(...)`).
- dynamic arrays/slices, nested array literals, and inference-only array literals remain future work.

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

**.sdslvtest runner status: WIP / bounded CPU evaluator.**

The `.sdslvtest` runner is a bounded CPU-side evaluator, **not** a full SDSL-V runtime.
It is useful for fast helper/unit-style checks over the currently implemented evaluator subset.

Why this differs from C# xUnit:
- C# xUnit executes normal C# because Roslyn/.NET compile and run the full language/runtime.
- SDSL-V currently has parser/validator/HLSL-emitter coverage plus a small CPU evaluator used by `.sdslvtest`.
- Therefore, only evaluator-implemented constructs execute in `.sdslvtest`.

`.sdslvtest` is the SDSL-V test-file extension.

Current syntax/validation:
- `[Fact]` single-case attribute is supported.
- `[Theory]` with one or more `[InlineData(...)]` rows is supported.
- assertions currently supported:
  - `Assert.True(condition, "message")`
  - `Assert.Equals(actual, expected, "message")`
  - `Assert.Near(actual, expected, tolerance, "message")`
- custom string message is mandatory for supported assert calls.
- non-assert expression statements are rejected by validation.
- `[Fact]` and `[Theory]` cannot be combined on the same test function.
- `[InlineData]` is only valid on `[Theory]` tests.
- theory rows require arity/type compatibility with test parameters.
- supported inline literal kinds for `[InlineData(...)]`: `i32`, `f32`/`float`, `bool` (string literals are rejected for parameter binding in current runner rules).

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
- theory row execution is CPU-side only and runs each row as an independent case (`TestName[0]`, `TestName[1]`, ...).

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
- record/stream construction or field access execution
- `with` expression execution
- array indexing/literal execution
- `if`, `switch`, and bounded `for` statement/expression execution in the runner
- enum `match` execution
- fallibility execution (`?`, `!`, `match ok/err`)
- `when utility` execution
- flow/board runtime execution
- GPU execution
- DXC/SPIR-V paths
- automatic filesystem discovery of `.sdslvtest`
- full shader-function execution pipeline
- broad vector/matrix semantics beyond current bounded evaluator surface

### Test strategy split (current)

- `.sdslvtest`:
  - fast CPU-side helper/unit tests for the bounded evaluator subset.
- GPU/render tests:
  - future/backend validation of compiled shader behavior, render flow, readback, and pixel results.

### Future direction

Full “normal SDSL-V + Assert” execution likely requires a real CPU backend and an IR/lowering path (for example Rust/MIR-style lowering).
That direction overlaps with broader RustOct/full Octest work and is intentionally out of scope for the current shader-language runner.

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
- no flow execution/runtime state machine
- no `suspend`, `remember`, `resume`

`when` role split in current authoring:
- `when utility`: standalone one-shot scored-choice expression for ordinary shader/helper function bodies.
- `when policy`: persistent flow/state policy surface; not accepted in ordinary shader/helper function bodies.

Current `when utility` usage:
```sdslv
return when utility {
    case 100 when a > 0 score a
    case 200 when b > 0 score b
    else -1
};
```

`when utility` behavior:
- highest eligible score wins;
- ties are first-wins (strict `>` update rule);
- `else` is required fallback;
- optionless form lowers to HLSL only in local initializer, assignment RHS, and return expression contexts;
- stateful options (`hysteresis`, `min_commit`) are validated but not lowered for ordinary shader/helper function emission in M66b.

`when policy` status in M67:
- ordinary function/helper usage is rejected with: `when policy is only valid inside flow/state bodies; use when utility for standalone ranked expressions`;
- flow/state `when policy` parsing/validation is reserved for a future flow-policy milestone (do not assume persistent policy lowering in current runtime/emitter paths).

## 11) Feature status table

| Feature | Status |
|---|---|
| record | implemented |
| stream | implemented |
| `with` | implemented (bounded contexts) |
| record/stream parameter immutability | implemented |
| `array<T, N>` | implemented |
| array literals | implemented in explicit array contexts |
| array indexing/assignment | implemented |
| `float2`/`float3`/`float4` constructors | validated |
| `float4x4` constructor | validated conservatively (16 numeric scalars) |
| `if` | implemented |
| condition-switch | implemented |
| subject-switch | implemented |
| bounded `for` | implemented |
| `while` | reserved/future |
| tag enum | implemented |
| enum `match` | implemented |
| fallible `?` / `!` | implemented |
| fallible `match ok/err` | validation implemented, HLSL lowering unsupported |
| `when utility` | implemented, optionless HLSL lowering in local init / assignment RHS / return |
| `when policy` | flow/state-only; rejected in ordinary function/helper bodies |
| flow + board | implemented for parse/validate and bounded acyclic value-returning lowering subset |
| interfaces + generics | implemented (bounded compile/monomorphization path) |
| coordinate-space typing | implemented (bounded checks) |
| tensor/Einstein notation | future design |
| batch/concurrency surface | future / not SDSL-V ordinary shader body yet |

## 12) Non-goals / future work

Known future work items:
- renderer pipeline integration for compiled shader artifacts
- fuller expression/type checking
- shader-body control flow (`if`/loops) expansion as scoped milestones
- full flow-policy lowering/runtime behavior
- stateful `when utility` lowering
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

## 15) M16 renderer artifact intake (metadata-only)

M16 adds a renderer-facing artifact intake boundary without GPU pipeline creation.

APIs in `Engine::render`:
- `BuildRenderPipelinePlan(name, artifact, vertex_entry, pixel_entry)`
- `RenderPipelinePlan` (Name, SourceName, Hlsl, VertexEntry, PixelEntry)
- `RenderPipelinePlanError` (`MissingEntryPoint`, `WrongStage`, `EmptyHlsl`, `DuplicateEntryPoint`)

Behavior notes:
- requested entry names must exist in artifact `EntryPoints` metadata
- requested vertex/pixel entries must match stage kind
- helper functions emitted in HLSL but not present as artifact entries are rejected
- flow helper functions emitted in HLSL but not present as artifact entries are rejected
- compile-alias entries (for example `ForwardFlatMaterial_PS`) are accepted when present
- no DXC or `wgpu` pipeline creation is required for plan creation


## 16) M17 pipeline plan → DXC requests bridge (metadata-only)

M17 adds a deterministic bridge from renderer pipeline plans to DXC compile requests.

Current path:

```text
SdslvShaderArtifact
→ BuildRenderPipelinePlan(...)
→ RenderPipelinePlan
→ BuildDxcRequestsForPipelinePlan(...)
→ PipelineDxcRequests { Vertex, Pixel }
```

APIs in `Engine::render`:
- `PipelineDxcRequests`
- `PipelineDxcRequestError`
- `BuildDxcRequestsForPipelinePlan(plan)`

Behavior notes:
- the bridge reuses M15 `DxcCompileRequest` directly (no parallel request type)
- vertex request uses plan `VertexEntry` metadata (`EntryPoint`, `TargetProfile`)
- pixel request uses plan `PixelEntry` metadata (`EntryPoint`, `TargetProfile`)
- both requests preserve identical HLSL source text and source name
- M15 still owns command construction (`BuildDxcCommand`) and optional process invocation

Still out of scope in M17:
- no DXC invocation required
- no `wgpu` shader-module creation
- no `wgpu::RenderPipeline` creation
- no reflection/resource-layout extraction

## 17) M18 optional pipeline-plan DXC compile boundary

M18 adds a renderer-facing optional compile helper over existing M17 + M15 boundaries.

Current path:

```text
RenderPipelinePlan
→ BuildDxcRequestsForPipelinePlan(...)
→ CompileHlslWithDxc(...) for Vertex and Pixel
→ CompiledPipelineShaders
```

APIs in `Engine::render`:
- `CompiledPipelineShaders`
- `CompilePipelineShadersError`
- `CompilePipelineShadersWithDxc(plan, options)`

Behavior notes:
- request-construction errors are wrapped as `CompilePipelineShadersError::Request(...)`
- vertex compile failures are wrapped as `CompilePipelineShadersError::Vertex(...)`
- pixel compile failures are wrapped as `CompilePipelineShadersError::Pixel(...)`
- normal tests still do not require DXC (nonexistent tool paths validate structured unavailable-tool behavior)

Still out of scope in M18:
- no `wgpu` shader-module creation
- no `wgpu::RenderPipeline` creation
- no shader reflection or bind-layout extraction
- no material/asset pipeline integration

## 18) M19 renderer resource descriptor scaffold boundary

M19 adds a deterministic renderer-side descriptor conversion from M18 compiled shader outputs:

```text
RenderPipelinePlan + CompiledPipelineShaders
→ BuildCompiledPipelineDesc(...)
→ CompiledPipelineDesc
```

APIs in `Engine::render`:
- `CompiledShaderModuleDesc` (`EntryPoint`, `TargetProfile`, `SpirvBytes`)
- `CompiledPipelineDesc` (`Name`, `SourceName`, `Vertex`, `Pixel`)
- `CompiledPipelineDescError`
- `BuildCompiledPipelineDesc(plan, compiled)`

Behavior notes:
- descriptor creation is plain-data only and does not require `wgpu::Device`, windows, or GPU presence.
- vertex/pixel compiled byte payloads must be non-empty.
- entry-point and target-profile metadata must match the originating pipeline plan.
- normal tests use fake compile results (`DxcCompileResult`) and remain GPU-free.

Still out of scope in M19:
- no `wgpu::ShaderModule` creation requirement
- no `wgpu::RenderPipeline` creation
- no reflection/bind-layout extraction
- no material pipeline integration


## 19) M20 render pipeline layout contract (metadata-only)

M20 defines a renderer-side plain-data pipeline-layout contract over M19 compiled shader descriptors:

```text
CompiledPipelineDesc
+ vertex layout metadata
+ color/depth target metadata
→ RenderPipelineLayoutPlan
```

APIs in `Engine::render`:
- `RenderPipelineLayoutOptions`
- `RenderPipelineLayoutPlan`
- `RenderPipelineLayoutPlanError`
- `BuildRenderPipelineLayoutPlan(compiled, options)`

Behavior notes:
- layout validation is deterministic and returns structured errors for common authoring/configuration mistakes
- tests use fake compiled shader bytes and do not require DXC or GPU availability
- `RenderSnapshot` remains runtime snapshot data; layout plans are separate future GPU-pipeline metadata

Still out of scope in M20:
- no `wgpu::ShaderModule` / `wgpu::PipelineLayout` / `wgpu::RenderPipeline` creation
- no reflection-driven input-layout extraction
- no bind-group/material/asset system integration


## 20) M21 `wgpu` resource creation probe (implemented)

M21 adds a GPU-resource-facing descriptor conversion seam while preserving GPU-free test defaults:

```text
RenderPipelineLayoutPlan
→ BuildWgpuRenderPipelineDescriptorPlan(...)
→ WgpuRenderPipelineDescriptorPlan
```

APIs in `Engine::render`:
- `WgpuVertexAttributeDesc`
- `WgpuVertexBufferLayoutDesc`
- `WgpuRenderPipelineDescriptorPlan`
- `WgpuPipelineResourceError`
- `MapVertexFormatToWgpu(...)`
- `MapVertexStepModeToWgpu(...)`
- `MapColorTargetFormatToWgpu(...)`
- `MapDepthFormatToWgpu(...)`
- `BuildWgpuRenderPipelineDescriptorPlan(...)`

Behavior notes:
- conversion is deterministic and plain-data only
- output owns converted vertex-buffer and attribute metadata
- normal tests do not create adapters/devices/surfaces/windows

Still out of scope in M21:
- no draw pass or command submission
- no `wgpu::ShaderModule` creation helper
- no `wgpu::RenderPipeline` creation
- no reflection-driven bind-layout/material pipeline


## M58 authoring notes: fallible seed

You can now write helper methods with fallible signatures (`-> T ! Error`) and use postfix `?` and `!` on fallible expressions. Only `Error` is allowed after `!` in signatures.

`?` is propagation-oriented and intended for fallible function contexts. `!` is explicit unwrap. Fallible `match` is not in M58 yet.

Stage entry-point fallibility and HLSL fallible lowering are intentionally not implemented in M58.


## M58b fallibility validation completion

M58b completes bounded validation for Oct-style fallibility in SDSL-V.

- Fallible signatures are `-> T ! Error`; non-`Error` fallible error types are rejected.
- Stage entry points cannot be fallible in M58/M58b.
- A call to a fallible function is a fallible expression until handled.
- `?` is valid only on fallible expressions and only inside fallible functions.
- `!` is valid only on fallible expressions.
- Unhandled fallible expressions are rejected in expression statements, local initializers, assignment RHS, return expressions, and compound-expression trees.
- `error(...)` is only valid in fallible return position (`return error("...");`).
- Fallible `match` is still future work.
- HLSL emission remains explicitly unsupported for modules that contain fallible functions/expressions (`fallible function emission is not implemented in SDSL-V M58`).


## M62 array vs vector/matrix constructor distinction

M62 tightens the authoring distinction between fixed arrays and shader numeric vectors/matrices:

- `array<T, N>` is fixed-size indexed storage (`arr[i]`, `arr[i] = value`).
- `float2`, `float3`, `float4`, and `float4x4` are shader numeric value types.
- Bracket literals (`[ ... ]`) are array literals only and require explicit array-typed context.
- Bracket literals do not initialize vector/matrix targets (`float3`, `float4`, `float4x4`).
- Use numeric constructors for vector/matrix values (`float2(...)`, `float3(...)`, `float4(...)`, `float4x4(...)`).

Current constructor validation in M62:

- `float2`, `float3`, `float4` validate numeric component counts and numeric argument types.
- `float4x4` validates exact 16 numeric scalar arguments.
- Constructor diagnostics call out wrong arity and non-numeric argument kinds.

Current indexing stance in M62:

- Indexing is array-only (`arr[i]`).
- Vector/matrix indexing is not part of M62; `color[0]` where `color: float4` is rejected.

Out of scope in M62:

- Einstein/tensor notation.
- Oct `vector[...]` / `matrix[[...]]` literal syntax.
- Dynamic arrays/slices.
- Broad overload resolution for constructor signatures.


## M63 fallibility integration hardening

M63 extends M58/M58b fallibility validation across newer expression/statement surfaces.

- Fallibility is checked recursively through array literals, array indexes, array element assignments, `with` updates, constructor arguments, `if` conditions, `switch` conditions/subjects/arm values, and `for` bounds/step expressions.
- These contexts do not hide fallible expressions: unhandled fallible usage emits `fallible expression must be handled with ? or !`.
- Existing diagnostics remain authoritative:
  - `? can only be used inside a fallible function`
  - `? requires a fallible expression`
  - `! unwrap requires a fallible expression`
- Handled `?` / `!` expressions participate in existing type validation as success values (`Fallible<T>` usage validates as `T` for constructor args, array indexes/elements, switch/if/for checks).
- Fallible `match` remains future work.
- HLSL fallible lowering remains unsupported; modules using fallible functions/expressions continue producing the explicit unsupported-emission diagnostic.
- `.sdslvtest` runner behavior remains unsupported for fallible execution, but nested fallible forms in these expression shapes must produce explicit unsupported diagnostics rather than panicking.


## Enum/match authoring status (M64c)

M64b/M64c support semantic validation/type-resolution and deterministic HLSL lowering for tag-only enum + match authoring:

- Enum variants must be qualified as `Enum.Variant`.
- Match subject must be enum-typed.
- Match arms must use the same enum as the subject.
- Match arms must be exhaustive over declared variants.
- Wildcard/default match arms are not available.
- Match arm values must resolve to compatible result types.
- Tag enum declarations lower to deterministic `static const int Enum_Variant = N;` constants in declaration order.
- Enum type refs lower to HLSL `int`.
- Qualified variant refs lower from `Enum.Variant` to `Enum_Variant`.
- Match expressions lower in bounded contexts (local initializer, assignment RHS, return expression) as `if` / `else if` / `else` chains, with final arm as `else` due to validated exhaustiveness.

Still not available in M64c:

- Payload-carrying enum variants.
- Fallible `match ok/err` forms.
- fallible match lowering


### M66b when utility HLSL lowering

- Optionless `when utility` now lowers in bounded statement contexts: local initializer, assignment RHS, and return expression.
- Lowering initializes the result from `else`, tracks `__utility_hasN` + `__utility_scoreN`, and updates only when an eligible case score is strictly greater than the current best score.
- Strict `>` preserves first-tie-wins behavior by source order.
- Stateful utility options (`hysteresis`, `min_commit`) remain parsed/validated but are rejected during HLSL emission with: `stateful when utility options are not lowered in SDSL-V M66b`.
- `when policy` remains flow/state-only and is not part of shader-expression lowering.
- Nested/general expression contexts for `when utility` remain unsupported in M66b and emit the bounded-context unsupported diagnostic.


## Raw HLSL compatibility path (M68)

SDSL-V remains WyrmCoil's reference shader authoring language. WGSL remains a native backend source path. Raw HLSL is also supported as a compatibility/escape-hatch source path for legacy or direct-DXC workflows.

SDSL-V is HLSL-targeting, not HLSL-compatible, and it is not an HLSL superset. Raw HLSL wrappers require explicit entry metadata (name, stage, target profile). WyrmCoil validates this wrapper metadata only; HLSL parse/semantic diagnostics are owned by DXC.

Example:

```rust
let artifact = BuildHlslShaderArtifact(
    "legacy_flat.hlsl",
    hlsl_source,
    vec![
        HlslEntryPoint::Vertex("VSMain"),
        HlslEntryPoint::Pixel("PSMain"),
    ],
)?;
```
