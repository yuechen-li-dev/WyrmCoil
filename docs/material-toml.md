# Native Material TOML Schema Seed (M90)

This document defines the first native WyrmCoil material asset shape.

Scope: design/schema/documentation only.

Non-goal reminder: this is **not** a MaterialX importer pass, **not** a material compiler pass, **not** a SDSL-V codegen pass, and **not** renderer binding integration.

## Native format position

WyrmCoil material hierarchy:

```text
WyrmCoil material TOML:
  native editable material graph format

MaterialX:
  compatibility/import/export format

SDSL-V:
  generated/reference shader authoring target

HLSL/WGSL:
  backend/escape source paths
```

MaterialX is **not** WyrmCoil's native material source of truth.

MaterialX may be converted into WyrmCoil material TOML.

WyrmCoil material TOML may later export to MaterialX when useful.

## File extension and asset header

Native WyrmCoil material assets use ordinary `.toml`.

Examples:

- `assets/materials/painted_metal.toml`
- `assets/materials/ui_button.toml`
- `assets/materials/lightmapped_wall.toml`

Asset-type discovery is field-based, not extension-based:

```toml
[asset]
type = "material"
version = 1
```

No custom extension (`.wcmat`, `.material`, `.wyrmmat`, etc.) is introduced in M90.

## Flat graph schema shape

M90 uses a flat graph, not nested XML/tree shape.

Each node is one `[[node]]` entry.

Recommended seed shape:

```toml
[asset]
type = "material"
version = 1

[material]
name = "PaintedMetal"
output = "surface"

[[node]]
id = "base_color_tex"
kind = "texture2d"

[node.params]
path = "textures/painted_metal_base.ppm"
color_space = "srgb"

[[node]]
id = "tint"
kind = "constant_float4"

[node.params]
value = [1.0, 0.9, 0.85, 1.0]

[[node]]
id = "base_color"
kind = "multiply"

[node.inputs]
a = "base_color_tex"
b = "tint"

[[node]]
id = "surface"
kind = "standard_surface"

[node.inputs]
base_color = "base_color"
roughness = "roughness"
metallic = "metallic"
```

## Graph rules (validation contract seed)

### IDs

- Every node has a unique `id`.
- IDs are stable, asset-local identifiers.
- IDs are not global GUIDs.
- Global asset identity/path is outside the material graph.
- M90 identifier guidance: keep IDs simple ASCII (`a-z`, `A-Z`, `0-9`, `_`, `-`).

### Kinds

Every node has a `kind`.

Initial node-kind candidates (design candidates, not implementation claims):

- `constant_f32`
- `constant_float2`
- `constant_float3`
- `constant_float4`
- `texture2d`
- `add`
- `multiply`
- `lerp`
- `normal_map`
- `standard_surface`
- `output`

### Inputs (graph edges)

Inputs are named edges in `[node.inputs]`:

```toml
[node.inputs]
base_color = "base_color_tex"
roughness = "roughness_value"
```

Rules:

- Input values reference other node IDs.
- Parent/child relation is derived from input references.
- Do not store duplicate parent+child edge tables.
- This avoids inconsistent graph topology state.

### Params (literal/config values)

Params are literal config, not edges:

```toml
[node.params]
value = 0.8
path = "textures/foo.ppm"
color_space = "srgb"
```

### Editor metadata (optional)

Optional editor-only metadata can live under `[node.editor]`:

```toml
[node.editor]
x = 420
y = 120
collapsed = false
```

Runtime/compiler paths should ignore editor metadata unless a tool explicitly consumes it.

### Output

M90 uses one explicit material output:

```toml
[material]
output = "surface"
```

Multi-output materials are future work.

## Type and semantic validation direction

Later graph validation should check:

- required inputs per node kind;
- param presence/type per node kind;
- output node kind compatibility;
- graph acyclicity;
- all input references resolve;
- unused nodes as warning (candidate), not hard error;
- texture node path resolution and texture/sampler planning seams;
- color-space metadata belongs to texture/storage interpretation, not sampler state;
- sampler intent may come from texture-node params first, with possible explicit sampler nodes later.

## SDSL-V generation direction (design only)

Planned lowering:

```text
Material TOML graph
→ validated material graph IR
→ generated SDSL-V functions/records
→ SDSL-V compiler
→ backend HLSL/WGSL path
```

Illustrative direction:

```toml
[[node]]
id = "base_color"
kind = "multiply"

[node.inputs]
a = "base_color_tex"
b = "tint"
```

could lower into SDSL-V logic conceptually like:

```sdslv
let base_color = base_color_tex * tint;
```

M90 does not implement lowering/codegen.

## Texture/sampler/binding relation to M85–M89

M90 builds on established seams:

- `texture2d` node references texture asset/path and eventually feeds texture upload/resource planning (`TextureUploadPlan`, optional `WgpuTextureResource` seam).
- sampler intent may be defaulted by texture usage initially and may later become explicit node/params mapping to `SamplerPlan`.
- material compiler phase (future) will assign/validate texture/sampler binding layouts.
- M88/M89 already provide lower-level binding-layout and bind-group seams to target later.

M90 does not define the full material binding ownership system.

## MaterialX import/export positioning

MaterialX is an XML interchange graph format.

WyrmCoil native editable material representation is TOML.

Importer/exporter direction:

- MaterialX importer normalizes MaterialX graphs into WyrmCoil material TOML (or the same validated material IR).
- Unsupported MaterialX nodes should produce structured diagnostics.
- MaterialX support must not force XML-shaped internals in engine/runtime data.
- Visual tools/editors should be able to operate directly on WyrmCoil TOML graphs.

## Example 1 — simple constant material

```toml
[asset]
type = "material"
version = 1

[material]
name = "FlatMagenta"
output = "surface"

[[node]]
id = "color"
kind = "constant_float4"

[node.params]
value = [1.0, 0.0, 1.0, 1.0]

[[node]]
id = "surface"
kind = "standard_surface"

[node.inputs]
base_color = "color"
roughness = "roughness"
metallic = "metallic"

[[node]]
id = "roughness"
kind = "constant_f32"

[node.params]
value = 0.5

[[node]]
id = "metallic"
kind = "constant_f32"

[node.params]
value = 0.0
```

## Example 2 — texture + tint material

```toml
[asset]
type = "material"
version = 1

[material]
name = "PaintedMetal"
output = "surface"

[[node]]
id = "base_color_tex"
kind = "texture2d"

[node.params]
path = "textures/painted_metal_base.ppm"
color_space = "srgb"

[[node]]
id = "tint"
kind = "constant_float4"

[node.params]
value = [1.0, 0.9, 0.85, 1.0]

[[node]]
id = "base_color"
kind = "multiply"

[node.inputs]
a = "base_color_tex"
b = "tint"

[[node]]
id = "roughness"
kind = "constant_f32"

[node.params]
value = 0.45

[[node]]
id = "metallic"
kind = "constant_f32"

[node.params]
value = 0.9

[[node]]
id = "surface"
kind = "standard_surface"

[node.inputs]
base_color = "base_color"
roughness = "roughness"
metallic = "metallic"
```

## Non-goals in M90

- no material parser implementation;
- no TOML dependency additions for parsing/runtime;
- no MaterialX parser/importer implementation;
- no SDSL-V code generation implementation;
- no material runtime/bind-group ownership implementation;
- no editor UI implementation;
- no GUID-first asset database/import pipeline;
- no custom material file extension.

## Outcome

M90 target outcome: **Outcome A (success)** for design seed.

The native material TOML shape, relation to MaterialX, relation to SDSL-V, and explicit non-goal boundaries are now documented without adding runtime/compiler implementation.
