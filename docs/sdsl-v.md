# SDSL-V M0 Language Contract (Design Reference)

## Status and intent

This document defines **SDSL-V M0** as a language contract and implementation reference for future milestones.

M0 was documentation only.

As of **M1 lexer/parser seed**, repository code now includes:

- lexer/tokenizer
- declaration-level parser
- declaration AST
- diagnostics with source spans

After M3:

- Minimal deterministic HLSL emitter is implemented for declaration-level modules (type aliases, streams, vertex/pixel stage signatures, raw preserved bodies).

After M5:

- Explicit compile-time shader instantiation is supported with `compile GenericShader<ConcreteShader> as Alias;`.
- Generic shader `where` interface constraints are validated against concrete shader `implements` declarations.
- Generic shader templates are not emitted directly; compile declarations emit monomorphized concrete stage entry points.
- Interface calls on constrained generic parameters are statically rewritten during emission for the M5 pattern `mat.Method(args) -> ConcreteShader_Method(args)`.

Still intentionally absent after M6:

- DXC invocation
- SPIR-V integration
- Default interface methods / `base.Method()`
- Full shader-wide type inference/typechecking

SDSL-V is inspired by Stride SDSL, but it is not required to be source-compatible with SDSL.

## Core thesis

SDSL-V keeps SDSL's best idea—**streams**—replaces mixin/effect magic with explicit interfaces and monomorphized generics, and adds coordinate-space typing so common rendering bugs become type errors.

Five load-bearing pillars:

1. **Streams**: automatic stage-boundary data weaving.
2. **Interfaces**: explicit shader capability contracts.
3. **Generics**: monomorphized composition/permutation replacement for effect-style composition.
4. **Coordinate-space types**: compile-time prevention of world/view/clip/NDC confusion.
5. **HLSL lowering**: deterministic HLSL text generation; downstream compilation handled by DXC.

## 1) Purpose and pipeline

Target authoring and compilation flow:

```text
SDSL-V -> HLSL -> DXC -> SPIR-V -> wgpu / backend pipeline
```

Contract notes:

- SDSL-V is a **shader authoring language**, not a GPU machine-code compiler.
- HLSL is the intended first lowering target.
- DXC handles backend GPU compilation in later milestones.
- WGSL is also a valid native shader source path, while SDSL-V remains the preferred high-level authoring direction by policy.

## 2) Relationship to SDSL

### Retained concepts

SDSL-V intentionally keeps:

- streams
- shader classes / namespace-style organization
- `stage`
- abstract contract concepts (modeled through interfaces and explicit requirements)
- `override`
- default-method / `base.Method()` direction as a **future** feature, not v0

### Removed concepts

SDSL-V intentionally removes:

- mixin inheritance order resolution
- `.sdfx` effect language
- implicit composition magic

SDSL-V is **SDSL-inspired**, not SDSL-compatible.

## 3) Syntax identity

Illustrative style:

```sdslv
namespace WyrmCoil.Examples;

use WyrmCoil.Core;

type WorldPosition3 = float3 @space(world.position);

type WorldNormal3 = float3 @space(world.normal);

stream Surface {
    WorldPos: WorldPosition3;
    Normal: WorldNormal3;
    Uv: float2;
}

interface IBaseColor {
    fn BaseColor(s: Surface) -> float4;
}

shader FlatColor implements IBaseColor {
    material {
        Color: float4;
    }

    override fn BaseColor(s: Surface) -> float4 {
        return Color;
    }
}
```

Primary keywords in this contract:

- `namespace`
- `use`
- `type`
- `stream`
- `interface`
- `shader`
- `implements`
- `where`
- `fn`
- `stage`
- `material`
- `override`

## 4) Type surface (v0 intent)

Planned early scalar/vector/matrix type surface:

- `bool`
- `i32`
- `u32`
- `f32`
- `float`
- `float2`
- `float3`
- `float4`
- `float4x4`
- `array<T, N>` (fixed-size array type; `N` must be a positive integer literal)

Type model notes:

- `type` declarations support aliases over these underlying data types.
- Semantic aliases (for coordinate spaces) are first-class for pre-lowering type checking.
- Swizzles are expected in the expression subset.
- Vector/matrix operators follow conventional shader arithmetic.
- Arrays are indexed storage collections (`arr[i]`) and are distinct from numeric vector/matrix value types.
- Fixed-array element assignment is supported for assignable array storage (`arr[i] = value;`) with integer index and compatible element type checks.
- Array parameters are immutable for element writes in M60; local arrays remain assignable.
- M61 adds fixed array literals in explicit array-typed contexts: `[a, b, c]`.
- Array literal length must match expected fixed-array length.
- Array literal elements must match expected element type.
- Array literals are never vector/matrix literals; use `float2(...)`, `float3(...)`, `float4(...)`, `float4x4(...)` constructors for numeric vector/matrix values.
- Dynamic arrays/slices, nested array literals, and inference-only array literals remain future work.

### Open design point: `float` vs `f32`

M0 keeps this explicit as unresolved until M1/M2 design lock:

- Option A: treat `float` as an alias to `f32`.
- Option B: preserve `float` as a distinct source-level spelling aligned to HLSL style while still lowering to 32-bit float.

Implementation milestones should lock this before parser and type-check details become rigid.

## 5) Coordinate-space types (load-bearing)

Example semantic aliases:

```sdslv
type ObjectPosition3 = float3 @space(object.position);
type WorldPosition3 = float3 @space(world.position);
type ViewPosition3 = float3 @space(view.position);
type ClipPosition4 = float4 @space(clip.position);

type WorldNormal3 = float3 @space(world.normal);
type TangentNormal3 = float3 @space(tangent.normal);
```

Contract behavior:

- Semantic aliases lower to their underlying HLSL data representation (`float3`, `float4`, etc.) **after** validation.
- Type checking occurs before lowering.
- World/view/clip/tangent mismatches are compile-time errors.
- Legal conversions must be explicit via conversion functions (e.g., matrix-transform helpers).

Valid/invalid sketch:

```sdslv
fn ToClip(pos: WorldPosition3, worldToClip: float4x4) -> ClipPosition4 {
    // valid: explicit conversion function body
    return TransformWorldToClip(pos, worldToClip);
}

fn InvalidMix(a: WorldPosition3, b: ClipPosition4) -> WorldPosition3 {
    // invalid: compile error, incompatible coordinate semantics
    return b;
}
```

## 6) Streams

SDSL-V now has sibling aggregate declarations with distinct roles:

- `record` = ordinary value aggregate.
- `stream` = stage-boundary value aggregate.

Records and streams are intentionally not interchangeable.
For value-semantics safety at function/stage boundaries, record/stream parameters are immutable in function bodies: field assignment through such parameters is rejected, and `with` is the supported copy-update path.

Stream declaration:

```sdslv
stream VertexOut {
    Position: ClipPosition4;
    Color: float4;
}
```

Initial lowering direction:

```hlsl
struct VertexOut {
    float4 Position : SV_Position;
    float4 Color : TEXCOORD0;
};
```

Deterministic semantic assignment (initial rule set):

- `ClipPosition4`/`Position` maps to `SV_Position`.
- Other fields map to `TEXCOORDn` in declaration order.
- Rule refinement for normals/tangents/custom semantics is future work.

Record declaration:

```sdslv
record SurfaceData {
    WorldPos: WorldPosition3;
    Normal: WorldNormal3;
    BaseColor: float4;
    Roughness: f32;
}
```

Initial record lowering direction:

```hlsl
struct SurfaceData {
    float3 WorldPos;
    float3 Normal;
    float4 BaseColor;
    float Roughness;
};
```

Record-lowering notes:

- records do not emit stage semantics (`SV_Position`, `TEXCOORDn`, etc.).
- records are plain value structs usable in function signatures and local declarations.
- `with` copy-update expressions are supported in M53b for records and streams in bounded contexts.
- local record/stream variables remain assignable for staged construction/update in M53c (for example, building a local vertex output before returning it).
- broader binding mutability (`let`/`var`) remains future work; M53c only enforces aggregate-parameter immutability at assignment targets rooted in record/stream parameters.

M53b `with` expression:

```sdslv
let adjusted: SurfaceData = surface with {
    Roughness: 0.5,
};
```

Semantics: copy base value, apply listed field updates, return updated value.
Current lowering contexts: local initializers, assignment RHS, and return expressions.

## 7) Shader classes and stages

Illustrative shape:

```sdslv
shader FlatColor {
    stage vertex fn VS(...) -> VertexOut { ... }
    stage pixel fn PS(input: VertexOut) -> float4 { ... }
}
```

Initial stage set:

- `vertex`
- `pixel`
- `compute`

Other stages (geometry/hull/domain/mesh/task/ray stages) are future-only and not in v0 scope.

## 8) Interfaces

Interface + implementation model:

```sdslv
interface IBaseColor {
    fn BaseColor(s: Surface) -> float4;
}

shader FlatMaterial implements IBaseColor {
    override fn BaseColor(s: Surface) -> float4 {
        return Color;
    }
}
```

v0 rules:

- Interface satisfaction is explicit via `implements`.
- Methods satisfying interface contracts must be marked `override`.
- Default methods and `base.Method()` dispatch are future features.

## 9) Generics and monomorphization

Example contract sketch:

```sdslv
shader ForwardPass<TMat>
    where TMat : IBaseColor, INormalProvider
{
    stage pixel fn PS(s: Surface, mat: TMat) -> float4 {
        return mat.BaseColor(s);
    }
}
```

Rules:

- Generics are compile-time only.
- Constraints replace effect/permutation composition language.
- Lowered HLSL is monomorphized concrete code per resolved instantiation.
- Exact author-facing instantiation syntax remains an open point for future milestone lock.

## 10) Material block

Example:

```sdslv
shader FlatColor {
    material {
        Color: float4;
        Roughness: f32;
    }
}
```

v0 behavior intent:

- Material members are plain fields.
- Property metadata/annotations are future work.
- Constant-buffer packing/layout rules are compiler-stage work, not M0.

## 11) Functions and expression subset (early)

Planned early support:

- `fn`
- `let`
- `return`
- field access
- function calls
- arithmetic
- swizzles
- basic `if`

Deferred by default:

- loops (`for`, `while`) are deferred unless future milestones require them for motivating examples.

## 12) Future features (not v0)

Explicit future-only list:

- properties
- partial shaders/classes
- nullable composition slots
- `impl Trait`
- Rust-style payload enums
- exhaustive `match`
- default interface methods
- `base.Method()`
- `.sdslvtest` test files with `[Fact]` + `Assert.*` validation (M7a parse/validate only)
- DXC invocation integration
- SPIR-V output integration
- WGSL output path

## 13) Testing strategy

Early SDSL-V development uses Rust's normal `cargo test` harness.

Milestone testing expectations:

- Lexer/parser tests are Rust tests.
- Validation tests are Rust tests over source strings.
- HLSL emission tests are Rust tests over generated text.
- Embedded `#test` shader blocks are a **future language feature**, not the initial harness.

Sequence expectation:

1. Rust tests first.
2. Custom shader-language harness only after parser/typechecker/evaluator foundations exist.

## 14) HLSL lowering model

Planned lowering behavior:

- Namespaces map to deterministic prefixes/mangled names.
- Streams map to HLSL structs.
- Semantic coordinate-space aliases lower to underlying HLSL scalar/vector types after type checking.
- Interfaces and generics are compile-time authoring constructs.
- HLSL output is deterministic text.
- DXC integration is future work.

## 15) Diagnostics philosophy

Diagnostics should be precise, local, and author-facing.

Examples:

```text
error: cannot pass ClipPosition4 to parameter WorldPosition3
  expected: WorldPosition3
  found: ClipPosition4
```

```text
error: shader FlatColor implements IBaseColor but missing override BaseColor(Surface) -> float4
```

## 16) Milestone decomposition

### SDSL-V M0 — Language contract

- Docs only.

### SDSL-V M1 — Lexer/parser seed

Parse declarations for:

- namespace
- use
- type aliases
- streams
- interfaces
- shaders
- material blocks
- function signatures
- stage signatures

### SDSL-V M2 — AST validation (implemented)

Now validates declaration-level structure:

- duplicate top-level declarations and duplicate `use` paths
- duplicate stream/material/method/stage member names
- interface method-body prohibition and duplicate interface methods
- shader implements/override requirements and override signature shape
- stage-name support (`vertex`, `pixel`, `compute`) and stage-body requirement
- generic parameter/where-constraint shape and interface existence

Still intentionally out of scope in M2:

- HLSL emission
- function-body semantic/type analysis
- expression checking

### SDSL-V M3 — Minimal HLSL emission (implemented)

Lowering currently emits deterministic HLSL text for:

- header comment + optional namespace comment
- type aliases (`typedef`) including `@space(...)` comment retention
- stream declarations as HLSL structs
- deterministic stream semantics: `ClipPosition4`/`@space(clip.position)` maps to `SV_Position`, all other fields map to `TEXCOORDn` in declaration order
- vertex/pixel stage signatures with shader-name mangling (`Shader_StageMethod`)
- pixel return semantic `: SV_Target`
- stage body raw-text preservation from M1 body storage

Intentional M3 limits:

- no DXC invocation
- no SPIR-V generation
- no renderer integration
- no function-body parsing/transformation
- no generic monomorphization; generic shader emission returns diagnostics

### SDSL-V M4 — Function body subset (implemented)

Implemented parse/lower subset:

- statements: `let` (typed, optional initializer), assignment, `return`, empty `;`
- expressions: identifiers, integer/float literals, `true`/`false`, field access/swizzles, function/constructor calls
- arithmetic: `+`, `-`, `*`, `/` with precedence and parenthesized grouping
- optional unary minus

M4 intentionally rejects unsupported body statements (such as `if`, loops, and `match`) with diagnostics and does not type-check expressions yet.

### SDSL-V M5 — Interfaces and generics

Implement:

- `where` constraints
- explicit `implements`
- monomorphized output

### SDSL-V M6 — Coordinate-space type checking

Enforce semantic alias compatibility.

### SDSL-V M7 — Shader tests seed

Design/implement embedded shader tests after sufficient compiler infrastructure exists.

### SDSL-V M8 — DXC / renderer integration

Feed generated HLSL into the downstream runtime pipeline.

### SDSL-V M6 — Coordinate-space type checking (implemented)

M6 adds a bounded body-validation pass for coordinate-space semantic aliases (`type X = floatN @space(...)`).

Current M6 checks:
- local `let` initializer compatibility
- assignment compatibility
- return-expression compatibility
- function-call argument compatibility
- stream field type propagation through field access

Compatibility behavior:
- semantic aliases are nominally distinct from each other
- plain aliases are transparent through underlying type mapping
- explicit conversion functions are allowed via declared signatures
- known/known mismatches produce diagnostics; unknown expression types stay permissive

Current limits:
- no full expression/global inference
- built-in function modeling is limited
- semantic swizzle typing is conservative (swizzle resolves to builtin vector/scalar types)
- no DXC/SPIR-V integration

## 17) Non-goals (M0)

- no parser in M0
- no compiler in M0
- no SDSL backward-compatibility requirement
- no `.sdfx`
- no mixin-order composition
- no shader test harness yet
- no DXC invocation yet
- no renderer integration yet
- no material asset pipeline yet

## Optional spec fixtures

If `.sdslv` examples are added in future docs passes, they are specification fixtures and documentation aids only until compiler milestones exist.

## Convergence statement

This M0 document intentionally narrows implementation risk:

- It defines what v0 should and should not do.
- It marks unresolved points explicitly instead of pretending they are settled.
- It anchors future parser/typechecker/lowering milestones to concrete staged outcomes.


## M1 implementation note

Current M1 parser behavior parses declaration structure only and does not semantically parse function bodies. Function and stage bodies are accepted as balanced brace blocks and preserved as raw body spans/text for future milestones.


## SDSL-V M7a/M7b test-file syntax and minimal runner

M7a introduced parse/validation support for dedicated test files with extension `.sdslvtest`.
M7b adds a minimal CPU-side test runner (`RunTestSource`) for executing `[Fact]` test bodies over a bounded scalar subset.

Current test syntax:
- top-level `namespace` and `use`
- attributes before functions (only `[Fact]` is supported)
- `[Fact] fn TestName() { ... }` test functions
- local `let` declarations and `Assert.*(...)` expression statements

Supported assertions in M7a:
- `Assert.True(condition, "message")`
- `Assert.Equals(actual, expected, "message")`
- `Assert.Near(actual, expected, tolerance, "message")`

Validation rules in M7a:
- only `[Fact]` is supported; unknown attributes plus `[Theory]`/`[Case]` are rejected
- `[Fact]` cannot have arguments and test functions cannot declare parameters
- duplicate test function names are rejected
- assertion custom message is mandatory and must be a string literal
- unknown `Assert.*` methods are rejected
- non-assert expression statements are rejected


M7b runner execution subset:
- statements: `let` (with initializer or scalar default), assignment to local identifiers, assert expression statements, empty statements
- expressions: scalar literals, local identifiers, `+ - * /`, unary `-`, parentheses, comparisons (`== != < <= > >=`)
- built-in calls: `abs`, `min`, `max`, `clamp`, `saturate`
- assertions executed: `Assert.True`, `Assert.Equals`, `Assert.Near`

M7b result surface:
- structured run result with overall pass/fail, pre-execution diagnostics, and per-test case failures
- parse/validation diagnostics are returned without panics and tests are not executed when diagnostics exist
- assertion failures include the custom message text and are collected within a test (continue-on-assert-fail)

M7b intentional limits:
- no shader function execution
- no GPU execution
- no DXC/SPIR-V integration
- no file-system test discovery
- unsupported statements/calls fail the test case with explicit runtime failure messages

Intentional non-goals in M7a:
- no test execution/runtime
- no CPU evaluator
- no DXC/SPIR-V integration
- no renderer integration

Compiler development and validation continue to use Rust `cargo test`.


## SDSL-V M9 shader flows (parser/validation design tightening)

M9 keeps shader flows Octomata-inspired and extends M8 with fixed-shape flow-local board declarations. This is still compile-time flow structure authoring, not runtime scheduling.

Supported syntax in M9:
- top-level `flow Name(params) -> ReturnType { ... }`
- optional `board { Name: Type; ... }` block inside a flow, before any `state`
- `state Name { ... }`
- `when { case <expr> -> goto <State> | return <expr>; ... else -> goto <State> | return <expr> }`
- direct `goto State;` and `return Expr;` state statements

Board contract in M9:
- board fields are fixed-shape declarations (`Name: Type;`) with no initializers
- board is flow-local memory for future shader/material composition control data
- board shape is declared up front and is not dynamically extensible
- board is not GPU mutable storage, not general application state, and is not lowered/emitted yet

M9 flow + board validation rules:
- flow names participate in top-level uniqueness checks
- flow must contain at least one state
- state names must be unique per flow
- each state must contain at least one statement
- each `when` must contain at least one `case` and must include `else`
- every `goto` target must resolve to a state in the same flow
- at most one `board` block per flow
- `board` block must appear before the first `state`
- board block must contain at least one field
- board field names must be unique per board
- unknown/unsupported board field types are rejected

Current non-goals (unchanged for M9):
- no flow lowering to HLSL yet
- no flow execution/runtime-state-machine behavior
- no board writes/mutation statements in state bodies yet
- no utility `when`
- no `suspend`, `remember`, or `resume`
- no generic flows

## SDSL-V M10 flow board reads (guard/return validation)

M10 keeps flows as authoring-only control structure and adds **board read name-resolution** inside flow state expressions.

Supported board-read syntax in M10:
- `board.FieldName`

Validated expression sites in M10:
- `when` case conditions (`case board.Flag -> ...`, `case board.Mode == 2 -> ...`)
- `when` case return expressions (`case cond -> return board.Mode`)
- `when` else return expressions (`else -> return board.Mode`)
- direct state return expressions (`return board.Mode;`)

Validation behavior in M10:
- `board.<Field>` is resolved against the current flow-local `board { ... }` declaration.
- unknown board fields are diagnosed (for example: `unknown board field 'SelectedMode' in flow 'ShadowVariant'`).
- if a flow has no board block, any `board.<Field>` reference is diagnosed.
- inside flow state expressions, `board` is reserved; flow parameters named `board` are rejected.

Current limits in M10:
- board writes are still unsupported.
- flow lowering/emission is still unsupported.
- flow execution/runtime state-machine behavior is still unsupported.
- utility `when`, `suspend`, `remember`, and `resume` remain out of scope.
- full flow expression typechecking remains future work.

Emitter behavior in M10:
- HLSL emission returns a diagnostic when a module contains flow declarations: `flow emission supports acyclic value-returning subset in SDSL-V M13`.


## SDSL-V M12 flow board writes and bounded flow type validation

M12 extends flow parse/validation-only semantics with controlled flow-local board writes and bounded type checks.

Supported new state statement syntax:
- `board.Field = expr;` (target must be exactly `board.<field>`).

M12 flow validation adds:
- board-write target validation (`unknown board field ...`, missing-board write diagnostics).
- board-write RHS type compatibility checks against declared board field types.
- guard-condition bool checks for known condition types (`case expr -> ...` requires `bool` when known).
- flow return-expression type checks for direct `return` and `when ... -> return ...` actions.
- flow parameters are available in flow expression type resolution.

M12 intentionally still does not implement:
- flow lowering or execution
- definite assignment / reachability analysis
- utility `when`, `suspend`, `remember`, `resume`

Emitter behavior remains parse/validate-only for flows:
- modules with flows return `flow emission supports acyclic value-returning subset in SDSL-V M13`.

## SDSL-V M14 — Shader artifact / entry-point contract (implemented)

M14 adds a structured artifact boundary between validated SDSL-V source and future backend compilation.

Current artifact API:
- `CompileSourceToShaderArtifact(source_name: &str, source: &str) -> Result<SdslvShaderArtifact, Vec<SdslvDiagnostic>>`
- `BuildShaderArtifact(source_name: &str, module: &SdslvModule) -> Result<SdslvShaderArtifact, Vec<SdslvDiagnostic>>`

Current artifact data:
- `SourceName` (caller-provided identity, no filesystem discovery)
- `Hlsl` (deterministic emitted HLSL text)
- `EntryPoints` (deterministic stage entry metadata)

Entry metadata includes:
- generated entry function name (`Shader_Method` pattern)
- stage kind (`vertex`, `pixel`, `compute` enum)
- shader name + method name
- default target profile mapping (`vs_6_0`, `ps_6_0`, `cs_6_0`)

Entry-point collection contract in M14:
- collect only shader `stage` methods that correspond to emitted shader entry functions
- generic shader templates are not listed directly
- `compile ... as Alias` entries are listed using the compile alias name
- ordinary helper methods and flow helper functions are not listed as entry points

M14 remains metadata-only:
- no DXC invocation
- no SPIR-V generation
- no renderer integration
- no reflection/bind-layout extraction


## SDSL-V M15 — DXC boundary / toolchain probe (implemented)

M15 adds an optional backend toolchain boundary from M14 shader artifacts to DXC invocation.

Public API surface in `Engine::shader::sdslv`:
- `DxcOptions` (`DxcPath`, `OutputSpirv`, `ExtraArgs`)
- `DxcCompileRequest` + `DxcCompileRequest::FromArtifactEntry(...)`
- `DxcCompileResult`
- `DxcError` (`ToolUnavailable`, `IoError`, `CompileFailed`, `OutputMissing`, `EntryPointNotFound`)
- `FindDxc(...)`
- `BuildDxcCommand(...)`
- `CompileHlslWithDxc(...)`
- `CompileArtifactEntryWithDxc(...)`

M15 boundary behavior:
- DXC is optional and tool-detected; normal tests do not require DXC.
- Command construction is deterministic/testable without spawning processes.
- Artifact entry metadata (`Name`, `TargetProfile`) feeds request construction directly.
- `OutputSpirv` defaults to `true` and adds `-spirv` when enabled.
- Real DXC invocation is isolated and writes temporary input/output files with best-effort cleanup.

Still intentionally out of scope after M15:
- renderer / `wgpu` pipeline integration
- shader reflection and bind-layout extraction
- asset pipeline/caching/discovery

## SDSL-V M16 — Renderer artifact intake / pipeline plan contract (implemented)

M16 adds a metadata-only renderer boundary that consumes `SdslvShaderArtifact` and produces deterministic pipeline-planning data.

Current renderer API surface in `Engine::render`:
- `RenderPipelinePlan`
- `ShaderStagePlan`
- `RenderPipelinePlanError`
- `BuildRenderPipelinePlan(...)`

M16 boundary behavior:
- validates requested vertex/pixel entry names against artifact `EntryPoints` metadata
- enforces stage-kind matching (`Vertex` and `Pixel`)
- rejects missing entries and duplicate entry metadata names with structured errors
- rejects empty artifact HLSL text
- preserves artifact metadata (`SourceName`, `Hlsl`, entry names/profiles) inside the plan

M16 is intentionally planning-only:
- no `wgpu::RenderPipeline` construction
- no DXC invocation requirement
- no shader reflection or bind-layout extraction
- no material/asset pipeline integration
## SDSL-V M17 — Pipeline plan to DXC requests bridge (implemented)

M17 connects the M16 renderer pipeline-plan metadata boundary to the M15 DXC request boundary.

Current bridge API surface in `Engine::render`:
- `PipelineDxcRequests` (`Vertex`, `Pixel`)
- `PipelineDxcRequestError` (`EmptyHlsl`, `EmptyEntryPoint`, `EmptyTargetProfile`)
- `BuildDxcRequestsForPipelinePlan(...)`

M17 boundary behavior:
- consumes a validated `RenderPipelinePlan`
- builds deterministic vertex and pixel `DxcCompileRequest` values
- preserves `SourceName` and full `Hlsl` payload for both requests
- maps entry metadata directly from `plan.VertexEntry` and `plan.PixelEntry`
- keeps DXC command building/invocation ownership in M15 (`BuildDxcCommand`, optional compile path)

M17 is intentionally metadata-only:
- no DXC invocation requirement
- no `wgpu` shader-module or render-pipeline creation
- no shader reflection/bind-layout extraction
- no material or asset pipeline expansion

Future work after M17 remains explicit:
- real backend compilation and shader-bytecode lifecycle
- `wgpu` shader-module creation and render-pipeline wiring
- reflection/resource-layout integration

## SDSL-V M18 — Optional plan-level DXC compile (implemented)

M18 adds an optional renderer compile boundary on top of M17 request construction.

Current compile API surface in `Engine::render`:
- `CompiledPipelineShaders` (`Vertex`, `Pixel`)
- `CompilePipelineShadersError` (`Request`, `Vertex`, `Pixel`)
- `CompilePipelineShadersWithDxc(plan, options)`

M18 boundary behavior:
- builds vertex/pixel `DxcCompileRequest` values using `BuildDxcRequestsForPipelinePlan(...)`
- compiles vertex request through M15 `CompileHlslWithDxc(...)`
- compiles pixel request through M15 `CompileHlslWithDxc(...)`
- returns structured request/vertex/pixel failures without panics
- keeps DXC optional; unavailable tools surface as structured `DxcError` values

M18 intentionally still does not do:
- `wgpu` shader-module creation
- `wgpu::RenderPipeline` creation
- shader reflection/resource-layout extraction
- material/asset pipeline expansion

## SDSL-V M19 — Renderer shader/pipeline resource descriptor scaffold (implemented)

M19 adds a renderer-facing plain-data resource boundary after optional M18 DXC compile results.

Current renderer API surface in `Engine::render`:
- `CompiledShaderModuleDesc` (`EntryPoint`, `TargetProfile`, `SpirvBytes`)
- `CompiledPipelineDesc` (`Name`, `SourceName`, `Vertex`, `Pixel`)
- `CompiledPipelineDescError`
- `BuildCompiledPipelineDesc(plan, compiled)`

M19 boundary behavior:
- consumes `RenderPipelinePlan` + `CompiledPipelineShaders`
- validates non-empty compiled bytes for vertex/pixel stages
- validates entry-point and target-profile metadata alignment with the plan
- preserves deterministic plan/source metadata and compiled stage bytes in plain descriptors

M19 remains intentionally scaffold-only:
- no `wgpu::ShaderModule` creation path is required
- no `wgpu::RenderPipeline` creation
- no reflection/resource-layout extraction
- no material or asset-pipeline expansion


## SDSL-V M20 — Render pipeline layout contract (implemented)

M20 adds the next renderer metadata layer after M19:

```text
CompiledPipelineDesc
+ vertex buffer layout metadata
+ color/depth target metadata
→ RenderPipelineLayoutPlan
```

Implemented APIs in `Engine::render` include:

- `VertexFormat`, `VertexAttributeDesc`, `VertexStepMode`, `VertexBufferLayoutDesc`
- `ColorTargetFormat`, `ColorTargetDesc`, `DepthFormat`, `DepthStencilDesc`
- `RenderPipelineLayoutOptions`, `RenderPipelineLayoutPlan`
- `RenderPipelineLayoutPlanError`
- `BuildRenderPipelineLayoutPlan(compiled, options)`

M20 behavior:

- deterministic plain-data plan construction
- validation for empty name, missing vertex buffers, zero stride, empty attributes, duplicate attribute locations, duplicate attribute names, out-of-bounds attribute offsets, and missing compiled shader bytes
- no DXC or GPU requirements in tests

M20 remains intentionally scaffold-only:

- no shader reflection or automatic input-layout extraction
- no `wgpu` pipeline-layout/render-pipeline creation
- no material, bind-group, or asset-pipeline expansion
- no conversion from runtime `RenderSnapshot` into draw buffers yet


## M57 control-flow status

Implemented in M56 (building on M55c):
- `if` statement validation and deterministic HLSL lowering.
- condition-switch expression validation and bounded HLSL lowering in:
  - local initializer (`let x: T = switch { ... };`)
  - assignment RHS (`x = switch { ... };`)
  - return expression (`return switch { ... };`)
- subject-switch expression validation and bounded HLSL lowering in the same contexts:
  - `switch subject { case value => result ... else => fallback }`
  - `switch subject { case value -> result ... else -> fallback }`

M56 switch split:
- condition-switch: `switch { case condition => value ... else => fallback }`
- subject-switch: `switch subject { case value => result ... else => fallback }`

M56 validation/lowering rules:
- `if` condition must be `bool` when known (`if condition must be bool; found ...`).
- nested decision ladders are rejected for `else { if ... }` single-statement shape.
- condition-switch case conditions must be `bool` when known.
- subject-switch case values must type-match the subject when known.
- switch arms must be type-compatible where known.
- subject-switch lowers to deterministic `if (subject == case_value) ... else if ... else ...` chains.
- no switch fallthrough (expression arms only).
- unsupported switch expression contexts are diagnosed rather than lowered with placeholders.

Implemented in M57 (building on M56):
- bounded numeric `for` loops:
  - `for i in start..end { ... }`
  - `for i in start..end step k { ... }`
- range semantics are inclusive start, exclusive end.
- `step` defaults to `1`.
- start/end bounds must be integer-typed when known.
- `step` must be integer-typed when known and must be greater than zero for statically known integer literals.
- loop variable is scoped to the loop body and validated as an integer local.
- HLSL lowering is deterministic bounded `for`:
  - `for (int i = start; i < end; i = i + step) { ... }`
- `while` is reserved for a future bounded condition-loop design and is currently rejected with an explicit diagnostic.
- bounded `for` is the supported loop form for predictable GPU execution.
- `flow`, `state`, and `step` are reserved SDSL-V keywords and cannot be used as identifiers.

Still future work:
- `while` loops
- `break` / `continue`
- `match` and enum-payload pattern matching
- generalized statement-expression lowering beyond bounded M55c contexts


## M58 fallibility seed

SDSL-V M58 adds fallible signatures in the form `fn F() -> T ! Error` and postfix fallible operators `expr?` (propagate) and `expr!` (explicit unwrap). Only `Error` is supported as the fallible error type in M58. Fallible `match` is deferred.

In M58, stage functions cannot be fallible. HLSL lowering for fallible syntax is not implemented and should be treated as unsupported for emission/runtime paths.


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


## M63 fallibility integration across arrays/constructors/control flow

M63 is a validation hardening pass (no new syntax):

- The Oct-style rule remains: if an expression can fail, callers must handle it with `?` or `!`.
- Recursive fallibility checks apply across array literals, array indexing, array element assignment, `with` update values/base, vector/matrix constructor arguments, `if` conditions, condition-switch and subject-switch conditions/subjects/case values, switch arm values, and bounded `for` start/end/step expressions.
- Unhandled fallible usage in these positions is rejected with the standard diagnostic: `fallible expression must be handled with ? or !`.
- Postfix `?` / `!` keep existing validation diagnostics and are type-checked as success-value expressions for downstream checks (`bool` conditions, integer indexes/bounds/step, constructor argument compatibility, array element compatibility).

Still out of scope in M63:

- fallible `match`
- tuple syntax/types/returns/destructuring
- HLSL fallibility lowering or error ABI generation


## M64b enum/match semantic validation + type resolution

M64b completes semantic validation for tag-only enums and enum `match` expressions:

- Enum names are recognized as valid named types in function signatures and local annotations.
- Qualified enum variant expressions (`Enum.Variant`) validate and resolve as enum-typed values.
- `match` subject must be enum-typed.
- `match` arms must reference variants from the subject enum.
- Duplicate and unknown arms are rejected.
- Enum `match` must be exhaustive across all declared variants (no wildcard/default arm in M64b).
- Match arm value types must be compatible, and match expression type resolves from arm result type compatibility.
- Fallibility traversal includes match subject and arm value expressions.

Still out of scope in M64b:

- Enum payload variants.
- Fallible `match ok/err` forms.
- (Implemented in M64c) deterministic HLSL lowering for tag enums, enum type refs, enum variant refs, and bounded-context match expressions.

## M64c enum/match HLSL lowering

M64c completes deterministic HLSL emission for tag-only enum + exhaustive match authoring.

- Tag enums lower to deterministic integer constants in declaration order:
  - `enum ShadowMode { None; Hard; Soft; }`
  - emits `static const int ShadowMode_None = 0;`, `ShadowMode_Hard = 1;`, `ShadowMode_Soft = 2;`.
- Enum type references lower to `int` in parameters, locals, record fields, and compatible array element contexts.
- Qualified variant references lower from `Enum.Variant` to `Enum_Variant`.
- Exhaustive `match` lowers in bounded statement contexts (local initializer, assignment RHS, return expression) as deterministic `if` / `else if` / `else` chains.
- Because exhaustiveness is validated before emission, the final match arm lowers as `else`.
- Nested/non-bounded match-expression contexts produce:
  - `match expression is not supported in this expression context in SDSL-V M64c`

Still out of scope in M64c:
- payload-carrying enum variants
- fallible match lowering
- wildcard/default match arms
### Fallible `match` (M65)

SDSL-V supports a fallible branch form for Oct-style local handling:

```sdslv
match Parse(raw) {
    ok(v) => v
    err(_) => 30
}
```

- Subject must be a fallible expression.
- Exactly one `ok(binding)` arm and one `err(binding)` arm are required.
- `=>` and `->` are both accepted as arm arrows.
- `err(_)` is valid to ignore the error value.
- Arm result types must be compatible.
- Fallible `match` handles the subject and yields an infallible result expression.
- Enum `match` remains a separate form (`Enum.Variant` arms) and enum payload matching is still future work.
- HLSL lowering for fallible match is intentionally not implemented in M65.
