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

Still intentionally absent after M5:

- DXC invocation
- SPIR-V integration
- Default interface methods / `base.Method()`
- Coordinate-space expression typechecking

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
- WGSL remains an escape hatch, but SDSL-V is the preferred high-level authoring direction.

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

Type model notes:

- `type` declarations support aliases over these underlying data types.
- Semantic aliases (for coordinate spaces) are first-class for pre-lowering type checking.
- Swizzles are expected in the expression subset.
- Vector/matrix operators follow conventional shader arithmetic.

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
- embedded `#test` shader tests
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
