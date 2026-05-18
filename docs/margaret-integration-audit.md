# M71a — Margaret Integration Audit / Plan Pass

## Milestone outcome

**Outcome A — success.**

This pass audited the imported Margaret subtree, ran the requested build/test/style commands, and produced an evidence-based M71b+ integration plan without refactoring Margaret or changing renderer behavior.


## M71b status update

- Margaret crates are now wired into the root Cargo workspace.
- Workspace package/dependency keys required by Margaret manifests are present at root.
- Margaret-owned function/method naming has been mechanically converged to WyrmCoil PascalCase policy.
- `margaret-vk` remains included as a compiling scaffold crate (GPU ray tracing still deferred).
- No camera bridge, ray-query API, renderer-loop integration, or GPU tracing feature work was added in M71b.

## 1) Current Margaret module/crate map

Observed subtree:

```text
src/Margaret/
  margaret-cli/
  margaret-core/
  margaret-cpu/
  margaret-image/
  margaret-testutil/
  margaret-vk/
```

### `margaret-core`

- Manifest: present at `src/Margaret/margaret-core/Cargo.toml`.
- Purpose: shared ray/camera/scene/material/image/render math/types.
- Public module exports in `src/lib.rs`: `camera`, `color`, `image`, `light`, `material`, `math`, `ray`, `render`, `scene`.
- Key public types:
  - Camera: `Camera` + `ray_for_pixel` / `ray_for_subpixel`.
  - Rays/hits: `Ray`, `HitRecord`.
  - Scene: `SceneDescription`, `SceneObject`, `Geometry::TriangleMesh`, `Triangle`.
  - Materials/lights: `MaterialDescription`, `MaterialKind`, `Light`.
  - Render config: `RenderMode`, `RenderDebugMode`, `RenderSettings`.
- Dependencies in manifest: none listed explicitly (std + workspace metadata only).
- Build status: source is present and appears complete, but crate is **not currently included in root workspace**.
- Tests: has inline tests (e.g., camera panic guards).

### `margaret-cpu`

- Manifest: present at `src/Margaret/margaret-cpu/Cargo.toml`.
- Purpose: CPU ray/path tracing backend and triangle intersection/shading path.
- Main API: `CpuRendererBackend` (`new`, `backend_name`, `describe_render`, `render`).
- Uses `margaret-core`, `margaret-image`, dev-dep `margaret-testutil`.
- Build status: implementation appears substantial and self-contained, but **not in root workspace build graph** yet.
- Tests: very large inline test section in `src/lib.rs` (regression scenes and shading checks).

### `margaret-image`

- Manifest: present.
- Purpose: owned image buffer and PPM output helper.
- Main API: `OwnedImage` with `new`, `get_pixel`, `set_pixel`, `write_ppm`.
- Dependency: `margaret-core`.
- Build status: not included in root workspace.
- Tests: no inline tests seen in this file.

### `margaret-testutil`

- Manifest: present.
- Purpose: sample scene/image helpers for tests.
- Main API: `sample_image_size`, `sample_scene`.
- Dependency: `margaret-core`.
- Build status: not included in root workspace.
- Tests: no inline tests in this file.

### `margaret-vk`

- Manifest: present.
- Purpose: Vulkan backend scaffold placeholder.
- Main API: `VulkanRendererBackend` with `new`, `backend_name`, `supports_size`.
- Dependency: `margaret-core` only.
- Build status: not included in root workspace.
- Tests: inline scaffold test validates non-zero size behavior.

### `margaret-cli`

- Manifest: present.
- Purpose: executable integration path for hardcoded scene rendering to PPM.
- Entry points: `run()` in `src/lib.rs` and binary `main()` in `src/main.rs`.
- Dependencies: `margaret-core`, `margaret-cpu`, `margaret-image`.
- Build status: not included in root workspace.
- Tests: no separate test module observed in CLI file.

## 2) Workspace integration status

### Root workspace state (current)

Root `Cargo.toml` defines a single package (`wyrmcoil`) and does **not** declare a `[workspace]` table nor Margaret members.

### Evidence from metadata/check/test

- `cargo metadata --no-deps` reports only `wyrmcoil` as workspace member.
- `cargo check -q --workspace` succeeds but only checks the existing single-crate workspace (WyrmCoil).
- `cargo test -q --workspace` succeeds but only runs WyrmCoil tests/examples/doc-test units.

### Integration implications

- Margaret crate manifests rely on `*.workspace = true` keys (`version.workspace`, `edition.workspace`, `license.workspace`, and workspace dependency links), so enabling them as members will require adding a real workspace root configuration that provides these fields.
- At M71a time, Margaret paths/manifests are present but not wired into the active build graph.

## 3) Margaret capability inventory

### Present capabilities (evidence-based)

- **Camera ray generation**: pixel/subpixel ray creation with FOV/aspect and forward/up basis (`Camera::ray_for_pixel`, `ray_for_subpixel`).
- **Ray + hit data model**: `Ray`, `HitRecord`.
- **Geometry**: triangle meshes (`Geometry::TriangleMesh`, `Triangle`).
- **Materials**: diffuse, specular reflector, dielectric (`MaterialKind`).
- **Lighting model**: directional light type in core plus emissive-surface handling in CPU renderer.
- **CPU rendering**:
  - debug modes (normals, albedo, depth),
  - lit path tracing mode with sampling/bounces,
  - triangle intersection path,
  - direct + indirect contributions.
- **Image output**: in-memory RGBA8 image and `.ppm` writer.
- **CLI behavior**: argument parsing, mode/size/output selection, hardcoded Cornell-style scene generation, prints render metadata summary.
- **Test utilities**: sample scene + size helpers.
- **Vulkan seam**: scaffold backend type with trivial capability check.

### Not observed in current imported code

- BVH / acceleration structure module or explicit spatial index in Margaret subtree.
- Sphere primitives in scene geometry enum.
- GPU ray tracing implementation in `margaret-vk` (only scaffold currently).

## 4) Camera audit

Current Margaret camera API:

- Constructor inputs are **position + forward + up + vertical FOV**, not explicit look-at target.
- Aspect is derived from requested image size at ray-generation time.
- Rays are generated for integer pixel centers and subpixel offsets (`subpixel_x/y` in `[0,1]`).
- Screen mapping uses:
  - `screen_x = (2u - 1) * half_width`
  - `screen_y = (1 - 2v) * half_height`
  which implies image-space Y-down input (pixel_y increases downward) remapped into camera-up positive Y in view space.
- Basis construction uses `right = forward.cross(up)` and `camera_up = right.cross(forward)`.

Handedness inference:

- With default-style values (`forward = -Z`, `up = +Y`), computed right becomes `+X`; this is consistent with a right-handed world/camera convention in practice.

Compatibility assessment with WyrmCoil now:

- WyrmCoil raster path currently has no integrated camera/projection system in golden path docs; Margaret camera is therefore not directly conflicting yet.
- Recommendation: keep Margaret camera Margaret-owned in M71b/M72 and add a bridge adapter later (avoid immediate Engine camera promotion).

## 5) Ray-query integration seam (proposed, not implemented)

Recommended near-term seam:

```text
Engine/Demo RenderSnapshot or primitive set
  -> Margaret bridge translator
  -> Margaret scene + camera ray query
  -> query hit/debug outputs consumed by Demo/editor tooling
```

Suggested first bridge-owned types (in WyrmCoil layer, not Margaret core rewrite):

- `MargaretRayQueryRequest` (camera parameters + screen coord).
- `MargaretRayQueryHit` (hit/miss + distance + position + normal + object/material ids).
- `MargaretSceneBridge` / `BuildMargaretSceneFromRenderSnapshot`.
- `MargaretCameraBridge` for mapping WyrmCoil camera inputs to Margaret `Camera`.

## 6) Relationship to WyrmCoil renderer

Boundary recommendation:

- `Engine::render` remains raster/window/backend flow.
- Margaret remains ray/reference/query subsystem.
- Do not route WyrmCoil present loop through Margaret CPU renderer.
- Use Margaret as supplemental subsystem for picking, debug probes, and reference validation.

## 7) Future Vulkan/GPU ray seam (`margaret-vk`)

Findings:

- `margaret-vk` is currently a **placeholder scaffold**, not a Vulkan ray tracing backend.
- No `ash`/`wgpu`/`vulkano` dependency usage appears in this crate manifest.
- Current code compiles in principle as plain Rust if wired, but provides no GPU ray features.

Recommendation:

- Keep GPU ray tracing deferred.
- If Margaret enters workspace in M71b, `margaret-vk` can remain included only as scaffold **or** be gated as optional member until real backend work begins, to avoid milestone confusion.

## 8) `wyrmfmt` style audit

Commands run:

- `cargo run --bin wyrmfmt -- check --lang rust src tests examples`
- `cargo run --bin wyrmfmt -- check --lang rust src/Margaret`

Results summary:

- Both commands fail with the same naming-policy violations concentrated in Margaret code.
- Violation pattern: project-owned function/method names in snake_case across core/cpu/image/testutil/cli/vk.
- Count observed in tool output: ~100+ findings (heavy concentration in `margaret-cpu/src/lib.rs`, then `margaret-core`, then CLI/image/testutil/vk).
- Trait method exemptions were not a dominant issue in this report; most flags are ordinary inherent/free functions.
- Test function names are currently being flagged in Margaret CPU tests as well, which may require confirming `wyrmfmt` test-exclusion behavior for this subtree.

M71b rename strategy recommendation:

1. Perform crate-by-crate mechanical rename guided by `wyrmfmt` suggestions.
2. Start `margaret-core` APIs first, then dependent crates (`margaret-image`, `margaret-testutil`, `margaret-cpu`, `margaret-cli`, `margaret-vk`).
3. Keep behavior unchanged; run crate-local tests after each step.
4. Handle public API breakage in one coordinated pass with compile-first ordering.

## 9) Naming/style conversion risk audit

High-risk rename areas for M71b:

- Public API methods used across Margaret crates (core->cpu->cli chain).
- Test helper function renames in huge `margaret-cpu` test block.
- Any trait impl methods (keep trait-contract spelling unchanged where applicable).
- CLI entry/argument parser helpers referenced across file.
- Potential false positives around tests depending on current `wyrmfmt` behavior.

Lower-risk areas:

- Local variables/parameters are already snake_case and should remain unchanged.
- No heavy macro/serde API surface observed in Margaret files reviewed.

## 10) Proposed staged integration plan

### M71b — Workspace + style integration (no feature bridge yet)

- Add true workspace root configuration and include Margaret crates as members.
- Provide required `[workspace.package]` and `[workspace.dependencies]` values used by Margaret manifests.
- Ensure `cargo check/test --workspace` genuinely includes Margaret crates.
- Apply `wyrmfmt`-guided PascalCase renames for Margaret-owned functions/methods only.
- Add crate-boundary docs (what remains Margaret-owned vs WyrmCoil bridge-owned).

### M72 — Camera/ray query bridge (no raster camera takeover)

- Introduce bridge API translating WyrmCoil camera/query inputs into Margaret camera/rays.
- Add unit tests for center/corner ray direction expectations and Y-axis convention mapping.
- Keep Margaret camera in Margaret; expose adapter in WyrmCoil integration module.

### M73 — RenderSnapshot/primitive to Margaret scene bridge

- Implement translation from selected WyrmCoil primitive snapshots into Margaret triangle scene.
- Add deterministic intersection tests (known geometry + expected hit distance/normal).

### M74 — Picking/query API consumer layer

- Expose screen coordinate -> ray -> hit query entry point usable by editor/demo gameplay.
- Return stable hit payload (entity/material/object identifiers where available).

### M75+ — GPU ray seam activation (deferred)

- Expand `margaret-vk` only after CPU bridge path is stable and validated.
- Keep this independent from WyrmCoil raster present-loop ownership.

## Command log summary (M71a)

Executed:

- `cargo fmt -- --check` ✅
- `cargo test -q --lib` ✅ (passes; warnings only)
- `cargo test -q` ✅ (passes; warnings + ignored optional GPU/window probes)
- `cargo check --examples` ✅ (passes; warnings only)
- `cargo run --bin wyrmfmt -- check --lang rust src tests examples` ❌ (expected style violations in Margaret naming)
- `cargo metadata --no-deps` ✅ (shows workspace currently contains only `wyrmcoil`)
- `cargo check -q --workspace` ✅ (checks only current single-member workspace)
- `cargo test -q --workspace` ✅ (tests only current single-member workspace)
- `cargo run --bin wyrmfmt -- check --lang rust src/Margaret` ❌ (same Margaret naming violations)

## Concise final assessment

- **What was audited:** Margaret crate structure, APIs, camera/ray/scene capabilities, workspace inclusion state, style debt via `wyrmfmt`, and integration seams.
- **What works now:** WyrmCoil root checks/tests pass; Margaret source tree is coherent and appears implementation-complete for CPU reference tracing path.
- **What fails now:** `wyrmfmt` naming policy for Margaret (large violation set). Margaret is not in active root workspace graph.
- **What should happen next:** M71b should first establish real workspace membership + style convergence, then M72/M73 bridge/query integration.
- **Milestone result:** **Outcome A (success)** for M71a audit/plan scope.

## M72 — Margaret Ray Query Actuator Boundary / Camera Ray Seed

M72 adds a small `Engine::ray` boundary that keeps Margaret-owned camera math behind an actuator-shaped bridge.

Boundary shape now documented and tested:

- **brain** (`Dunewyrm` frame): emits a `BuildCameraRay` act and passes `ScreenX`, `ScreenY`, and `RayQueryId` through board keys.
- **hands** (`Engine::ray::margaret`): `MargaretCameraRayAdapter::BuildCameraRay(...)` resolves normalized viewport coordinates `[0,1]` to Margaret pixel/subpixel coordinates and writes the result into `RayQueryStore`.
- **eyes** (`DwMailbox`): actuator enqueues `DwMessage::I32(MailKinds::RayQueryCompleted, query_id)`; message is staged this tick and visible on next tick.
- **world/store** (`RayQueryStore`): rich ray result payload (`origin`, `direction`) is persisted by query id and retrieved out-of-band from mailbox payload.

M72 camera convention notes:

- Uses Margaret `Camera` with `position + forward + up + vertical_fov_degrees`.
- Uses normalized viewport input (`ScreenX`, `ScreenY`) in `[0,1]` at engine boundary.
- Adapter performs viewport-to-pixel/subpixel conversion and preserves Margaret Y-down input mapping behavior.

Non-goals still unchanged after M72:

- No scene intersection bridge.
- No picking API.
- No RenderSnapshot-to-Margaret scene translation.
- No GPU ray tracing feature work.

## M73 — Triangle Hit Query Bridge

M73 extends the M72 actuator integration from camera-ray generation to direct triangle hit/miss ray queries using Margaret-backed math and intersection logic through `Engine::ray::margaret::ExecuteTriangleRayQuery(...)`.

Boundary remains unchanged:

- Brain (Dunewyrm control) requests query execution via act id.
- Hands (Margaret bridge) execute CPU triangle intersection from a WyrmCoil-facing `TriangleRayQueryRequest` and `RayTriangleScene`.
- Eyes (mailbox) carry completion-only `DwMessage::I32(MailKinds::RayQueryCompleted, query_id)`.
- Memory (`RayQueryStore`) stores rich hit/miss outcomes keyed by `RayQueryId`.

Scope deliberately remains bounded:

- No RenderSnapshot-to-Margaret scene translation yet.
- No picking API yet.
- No GPU tracing path.
- No renderer feature coupling.

M73 also demonstrates that board scalar lanes are still enough for completion routing via `query_id`, while rich query payloads can live in request objects and rich results remain in `RayQueryStore`.


## M74 status update

- Added `RayQueryRequestStore` to hold rich query requests keyed by `RayQueryId`.
- Added `RayQueryRequest` variants for camera-ray and triangle-ray query payloads.
- Added request-by-id execution helper at the Margaret bridge boundary.
- Dunewyrm act and mailbox payloads remain query-id only (`DwMessage::I32(kind, query_id)`).
- Rich request payloads live in request store; rich outcomes continue to live in `RayQueryStore`.
- This pass removes board-scalar lane pressure from M72/M73 query payload growth.
- Non-goals unchanged: no picking API, no RenderSnapshot bridge, no GPU tracing.

## M75 status update

M75 adds a picking-ready composition API in the Engine ray Margaret bridge:

- normalized viewport coordinate (`ScreenX`, `ScreenY`) + Margaret camera adapter + triangle scene
- composed into one execution path that produces final `RayQueryOutcome::Hit` or `RayQueryOutcome::Miss`
- request/result-store + query-id completion mailbox boundary remains intact

Boundary details:

- request store carries `RayQueryRequest::PickTriangle(PickRayQueryRequest)` payloads
- result store carries final pick outcome (`Hit` or `Miss`) under the same query id
- mailbox carries completion only: `DwMessage::I32(completion_kind, query_id)`
- no RenderSnapshot/entity/world picking bridge yet
- no GPU ray tracing changes

Coordinate convention:

- `ScreenX` and `ScreenY` are normalized viewport coordinates
- M75 validates both coordinates are finite and inside `[0, 1]`
- Y-axis mapping remains inherited from the M72 Margaret camera adapter path

## M76 status update

M76 adds a narrow demo/bootstrap geometry bridge from `RenderSnapshot` visible primitives into `Engine::ray::RayTriangleScene`:

- Path used: `RenderSnapshot -> BuildVisiblePrimitiveDemoBatch(...) -> RayTriangleScene`.
- Coordinate bridge convention: 2D visible primitive vertices `(x, y)` are projected to a fixed ray-query plane `z = -1.0` (configurable option).
- Deterministic triangle ids are assigned as `triangle_id = render_item_index * 2 + triangle_index_in_item`.
- Bridge emits deterministic source mappings (`TriangleId -> RenderItemIndex + EntityId + TriangleIndexInItem`) so pick hits can be interpreted back to source render/demo data.

Scope boundary (unchanged):

- This is not a full world/entity picking system.
- This is not a material bridge or scene-graph bridge.
- This is not a renderer ownership change.
- This is not a GPU tracing pass.

## M77 visible picking seed status update

M77 adds a visible RenderSnapshot picking API seed over the M76 demo bridge.

- New helper: `PickVisibleRenderSnapshot(...)` in `Engine::ray`.
- Input: normalized viewport coordinates (`ScreenX`, `ScreenY`) in `[0,1]`, Margaret camera adapter, `RenderSnapshot`, and bridge options.
- Flow: build `RenderSnapshotRayScene` from visible primitives, execute M75 pick query, resolve triangle hit id through `TriangleSources` metadata.
- Output: `VisiblePickResult::{Hit,Miss}` where hit carries `QueryId`, `EntityId`, `RenderItemIndex`, `TriangleId`, `TriangleIndexInItem`, `Distance`, `Position`, and `Normal`.
- Error policy: missing source metadata returns structured `VisiblePickError::MissingTriangleSource { TriangleId }`.

Boundary reminder:

- This is still demo/bootstrap visible-geometry picking over `RenderSnapshot`.
- No scene graph ownership pass, no material bridge pass, and no GPU ray tracing.
- Future true world/entity picking should introduce richer geometry ownership/registration beyond visible demo primitives.

## M78 world-resource follow-up

M78 introduces an engine-owned `WorldBlackboard` seed (`src/Engine/world.rs`) to hold world/subsystem resources used by ray-query flows.

Current M78 placement:

- `WorldGeometryRegistry` for pickable triangle registration (entity id + stable triangle id + triangle points).
- `RayQueryRequestStore` and `RayQueryStore` owned as world resources.

Boundary reminder:

- Dunewyrm board remains control working memory (TTL/dirty decision facts).
- WorldBlackboard remains world-owned typed resources and subsystem stores.

Non-goals unchanged:

- no ECS/archetype work,
- no scene graph or hierarchy transforms,
- no renderer pipeline coupling,
- no full picking/world-entity integration yet.

M79 can now consume `WorldGeometryRegistry` as a world-picking source instead of relying on the temporary RenderSnapshot visible-primitive bridge.

## M79 status update

M79 adds a world-owned picking bridge from `WorldGeometryRegistry` to `RayTriangleScene` with explicit source metadata mapping.

New path:

- `WorldGeometryRegistry` (`Engine::world`)
- `BuildRayTriangleSceneFromWorldGeometryRegistry(...)` (`Engine::ray`)
- `PickWorldGeometryRegistry(...)` (`Engine::ray`)
- `WorldPickResult::{Hit, Miss}` with world source metadata (`EntityId`, `TriangleId`)

Boundary reminder:

- RenderSnapshot bridge (M76/M77) remains for visible/bootstrap picking.
- WorldGeometryRegistry bridge (M79) is the world-owned pick source seed for subsystem-friendly geometry.
- No scene graph ownership pass.
- No material bridge.
- No physics/collision system.
- No GPU tracing feature work.

WorldBlackboard relation:

- `WorldBlackboard.Geometry` is the intended world-owned geometry source for this bridge.
- Request-store ownership of full world geometry is intentionally deferred.
- Future milestones can add camera/input world resources and request/actuator wiring around this helper.


## M80 status update

M80 adds a minimal world-resource picking seam in `Engine::world` + `Engine::ray`:

- World-owned camera resource (config shape) that builds a `MargaretCameraRayAdapter` on demand.
- World-owned input cursor resource with normalized `[0, 1]` finite validation.
- `PickWorldBlackboard(...)` helper that composes blackboard camera/input/geometry into existing M79 world-geometry picking.

### M81 — Actuatorized world pick request

- Request/store shape now includes `RayQueryRequest::WorldPick(WorldPickRayQueryRequest { QueryId })` in `WorldBlackboard.RayRequests`.
- `ExecuteWorldPickRequestById(...)` executes world pick by id against blackboard-owned `Camera`, `Input`, and `Geometry`.
- Results are persisted in the shared `RayQueryStore` as `RayQueryOutcome::{Hit, Miss, WorldPickFailure}` keyed by `RayQueryId`.
- Missing camera/cursor/invalid cursor are stored as `WorldPickFailure` outcomes (actuator executed), and completion is still staged.
- Mailbox remains query-id-only completion (`DwMessage::I32(kind, query_id)`); rich result payload stays in blackboard result memory.
- This pass does not add editor UI, winit wiring, ECS/scene graph, or GPU tracing.

This remains a helper-level ergonomic pass only. It does not integrate a window event loop, does not replace render camera ownership, and does not introduce scene-graph/ECS or GPU tracing features.


## M82 status update

M82 records Margaret world picking (M81) as the first full WyrmCoil actuator-subsystem worked example.

Reference document: `docs/actuator-subsystems.md`.

Pattern reminder:

- Dunewyrm emits id-only act intent.
- Rich request payload is stored in `WorldBlackboard.RayRequests`.
- Actuator executes by id (`ExecuteWorldPickRequestById(...)`).
- Rich hit/miss/failure outcome is stored in `WorldBlackboard.RayResults`.
- Mailbox completion is id-only and staged (`DwMessage::I32(kind, query_id)`).
- Control consumes completion next tick and resolves rich result from store.
