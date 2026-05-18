# Actuator Subsystem Pattern (M82)

This document captures the reusable WyrmCoil actuator-subsystem pattern proven in M72–M81.

It is a documentation/template pass, not a framework pass.

## Purpose

Use Dunewyrm as the control brain while world/domain subsystems execute external capabilities behind explicit actuator boundaries.

Canonical proven path:

```text
Dunewyrm brain
→ ID-only act
→ request store
→ subsystem actuator
→ result store
→ query-id-only mailbox completion
→ next-tick control consumption
```

## Role split

```text
Dunewyrm:
  brain / control logic

Act:
  ID-only request intent

RequestStore:
  rich input payload keyed by id

Subsystem actuator:
  executes external/domain capability

ResultStore:
  rich output/failure payload keyed by id

Mailbox:
  query-id-only completion notification

WorldBlackboard:
  world-owned resource home for request/result stores and subsystem context
```

## Tick-shape diagram

Mailbox visibility uses Dunewyrm staged/visible semantics.

```text
Tick N:
  control frame decides it needs work
  request payload is stored under id
  control emits act(id)

Actuator phase:
  dispatcher sees act(id)
  subsystem executes request
  result/failure stored under id
  mailbox stages Completed(id)

Tick N+1:
  completion message becomes visible
  control consumes Completed(id)
  control looks up result/failure by id
```

## Canonical worked example: Margaret world picking

Current proven M81 path:

```text
WorldBlackboard.RayRequests:
  RayQueryRequest::WorldPick(WorldPickRayQueryRequest { QueryId })

Act:
  execute world pick by query id

Actuator:
  ExecuteWorldPickRequestById(...)

WorldBlackboard context:
  Camera
  Input cursor
  Geometry

WorldBlackboard.RayResults:
  RayQueryOutcome::{Hit, Miss, WorldPickFailure}

Mailbox:
  DwMessage::I32(RayQueryCompleted, query_id)
```

M81 lifecycle rule:

```text
Missing camera/cursor:
  stored WorldPickFailure + completion staged.

Missing request / wrong request kind:
  execution error; no result and no completion.
```

## Boundary rules (explicit)

### 1) Do not put rich payloads in mailbox

Mailbox is for completion/signaling identifiers.

Good:

```text
RayQueryCompleted(query_id)
```

Bad:

```text
RayQueryCompleted(distance, normal_x, normal_y, normal_z, entity_id, ...)
```

Rich payload lives in result stores.

### 2) Do not put rich subsystem data in Dunewyrm board

Dunewyrm board is control memory:

```text
small typed facts
ttl
dirty lanes
decision inputs
```

WorldBlackboard/resources are for:

```text
geometry registries
camera/input resources
request/result stores
asset handles
subsystem state
```

### 3) Acts express intent, not the world

Acts should stay small, often ID-only. If a subsystem needs rich input, put it in a request store keyed by id.

### 4) Completion is visible next tick

Actuators stage completion mailbox messages. Control consumes them after staged→visible promotion at a deterministic tick boundary.

### 5) Executed request failures are result payloads

If request exists and actuator executes but fails because of missing context/domain failure, store failure result and stage completion.

If request is missing or wrong-kind, return execution error and do not stage completion.

## Future subsystem template/checklist

```text
To add a new actuator subsystem:

1. Define request id type or reuse a domain id.
2. Define request payload enum/struct.
3. Add request store or world resource slot.
4. Define result/failure payload enum.
5. Add result store or world resource slot.
6. Define act id(s).
7. Implement ExecuteXRequestById(...).
8. Stage completion mailbox event with id only.
9. Add tests:
   - success result
   - domain failure result + completion
   - missing request error + no completion
   - staged/visible mailbox promotion
   - deterministic snapshots/order
10. Document what lives in request store, result store, mailbox, and world resources.
```

## Future candidate mappings (short)

### Asset loading

```text
RequestStore:
  LoadTexture(asset_path/id)

ResultStore:
  LoadedTexture(handle) or LoadFailed(error)

Mailbox:
  AssetLoadCompleted(request_id)
```

### Audio

```text
RequestStore:
  PlaySound(sound_id, position, volume)

ResultStore:
  PlaybackStarted(handle) / Failed

Mailbox:
  AudioCommandCompleted(request_id)
```

### Pathfinding

```text
RequestStore:
  PathQuery(start, goal, nav_layer)

ResultStore:
  Path(points) / NoPath / Failed

Mailbox:
  PathReady(query_id)
```

### Lightmap/probe baking

```text
RequestStore:
  BakeJob(scene_id, bake_settings_id)

ResultStore:
  BakeProgress / BakeCompleted(artifact_id) / BakeFailed

Mailbox:
  BakeEvent(job_id)
```

## Relationship to Rust ecosystem crates

WyrmCoil direction:

- External Rust crates should become WyrmCoil actuator adapters when they provide domain capabilities.
- Dunewyrm remains the behavior/control brain.
- Do not import external behavior-tree/AI orchestration crates as the core brain.

Good subsystem-crate fit examples:

- ray tracing
- pathfinding
- audio
- physics/collision
- image/asset processing
- networking

Bad fit for core architecture:

- behavior-tree crate replacing Dunewyrm
- generic orchestration framework competing with HFSM+utility control
- reflection/plugin system as core architecture

This boundary keeps control deterministic and testable while still allowing strong domain crates behind explicit actuator seams.


## Asset byte-load actuator seed (M83)

M83 adds the second worked actuator-subsystem example after Margaret world picking.

- `WorldBlackboard.Assets.Requests` stores `AssetRequest::LoadBytes` payloads.
- `ExecuteAssetRequestById(...)` consumes request-id intent, runs utility-planned execution, and stores results in `WorldBlackboard.Assets.Results`.
- Completion mailbox payload remains id-only: `DwMessage::I32(AssetRequestCompleted, request_id)`.
- Missing request is an execution error with no completion staged.
- Existing request with invalid path, missing file, or IO failure stores `AssetResult::LoadFailed(...)` and stages completion.
- M83 supports immediate synchronous byte loading only; no decode, no upload, no hot reload, no async/deferred execution.


## Asset image decode stage (M84)

M84 extends the M83 asset actuator seed with decode requests/results.

- `WorldBlackboard.Assets.Requests` now supports `AssetRequest::DecodeImage` with owned bytes.
- Utility planning now includes `ImmediateImageDecode` in addition to byte-load planning.
- Execution stores either `AssetResult::ImageDecoded` (CPU RGBA8 payload) or `AssetResult::DecodeFailed` (structured kind).
- Completion mailbox remains id-only: `DwMessage::I32(AssetRequestCompleted, request_id)`.
- Deterministic decode surface in M84 is P6 PPM only (8-bit max value 255).

Scope boundaries remain unchanged: no GPU texture upload, no texture/material integration, no async jobs, and no hot reload.


## Asset texture upload mapping (M85)

M85 extends the asset actuator trajectory with a CPU image to texture-upload-plan seam:

```text
AssetResult::ImageDecoded(DecodedImageAsset)
  -> BuildTextureUploadPlan(...)
  -> TextureUploadPlan (plain data, validated RGBA8 payload)
```

Boundary reminder:

- This is not GPU texture creation yet.
- This is not sampler/bind-group/material wiring yet.
- Completion mailbox remains id-only; rich payload remains in stores/plans.
